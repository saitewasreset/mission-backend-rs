pub mod delete_mission;
pub mod mission_invalid;

use crate::{api_parse_json_body, db::schema::player, APIResponse, AppState, DbPool};
use actix_web::{get, post, web::{self, Buf, Bytes, Data, Json}, HttpRequest};
use diesel::prelude::*;
use diesel::{insert_into, update};
use log::error;
use std::fs;
use common::admin::{APIMissionInvalid, APISetMissionInvalid};
use crate::cache::manager::CacheManager;

#[derive(Insertable)]
#[diesel(table_name = player)]
struct NewPlayer {
    pub player_name: String,
    pub friend: bool,
}

#[post("/load_mapping")]
async fn load_mapping(
    requests: HttpRequest,
    app_state: Data<AppState>,
    cache_manager: Data<CacheManager>,
    body: Bytes,
) -> Json<APIResponse<()>> {
    if !app_state.check_session(&requests) {
        return Json(APIResponse::unauthorized());
    }


    match api_parse_json_body(body) {
        Err(e) => Json(APIResponse::bad_request(&e)),
        Ok(mapping) => {
            let write_path = app_state.instance_path.as_path().join("./mapping.json");

            match fs::write(&write_path, serde_json::to_vec(&mapping).unwrap()) {
                Err(e) => {
                    error!(
                "cannot write mapping to {}: {}",
                write_path.to_string_lossy(),
                e
            );
                    Json(APIResponse::internal_error())
                }
                Ok(()) => {
                    cache_manager.update_mapping(mapping);
                    Json(APIResponse::ok(()))
                }
            }
        }
    }
}

#[post("/load_watchlist")]
async fn load_watchlist(
    requests: HttpRequest,
    app_state: Data<AppState>,
    db_pool: Data<DbPool>,
    body: Bytes,
) -> Json<APIResponse<()>> {
    if !app_state.check_session(&requests) {
        return Json(APIResponse::unauthorized());
    }

    let watchlist: Vec<String> = match serde_json::from_reader(body.reader()) {
        Ok(x) => x,
        Err(e) => {
            return Json(APIResponse::bad_request(&format!(
                "cannot parse payload as json: {}",
                e
            )));
        }
    };

    let watchlist = watchlist
        .into_iter()
        .map(|player_name| NewPlayer {
            player_name,
            friend: true,
        })
        .collect::<Vec<_>>();

    let result = web::block(move || {
        let mut conn = match db_pool.get() {
            Ok(x) => x,
            Err(e) => {
                error!("cannot get db connection from pool: {}", e);
                return Err(());
            }
        };

        match update(player::table)
            .set(player::friend.eq(false))
            .execute(&mut conn)
        {
            Ok(_) => {}
            Err(e) => {
                error!("cannot update db: {}", e);
                return Err(());
            }
        };

        match insert_into(player::table)
            .values(&watchlist)
            .on_conflict(player::player_name)
            .do_update()
            .set(player::friend.eq(true))
            .execute(&mut conn)
        {
            Ok(_) => {}
            Err(e) => {
                error!("cannot update db: {}", e);
                return Err(());
            }
        };

        Ok(())
    })
        .await
        .unwrap();

    match result {
        Ok(()) => Json(APIResponse::ok(())),
        Err(()) => Json(APIResponse::internal_error()),
    }
}

#[post("/load_kpi")]
async fn load_kpi(
    requests: HttpRequest,
    app_state: Data<AppState>,
    cache_manager: Data<CacheManager>,
    body: Bytes,
) -> Json<APIResponse<()>> {
    if !app_state.check_session(&requests) {
        return Json(APIResponse::unauthorized());
    }

    match api_parse_json_body(body) {
        Err(e) => Json(APIResponse::bad_request(&e)),
        Ok(kpi_config) => {
            let write_path = app_state.instance_path.as_path().join("./kpi_config.json");

            match fs::write(&write_path, serde_json::to_vec(&kpi_config).unwrap()) {
                Err(e) => {
                    error!(
                "cannot write kpi config to {}: {}",
                write_path.to_string_lossy(),
                e
            );
                    Json(APIResponse::internal_error())
                }
                Ok(()) => {
                    cache_manager.update_kpi_config(kpi_config);
                    Json(APIResponse::ok(()))
                }
            }
        }
    }
}

#[post("/delete_mission")]
async fn api_delete_mission(
    requests: HttpRequest,
    app_state: Data<AppState>,
    db_pool: Data<DbPool>,
    body: Bytes,
) -> Json<APIResponse<()>> {
    if !app_state.check_session(&requests) {
        return Json(APIResponse::unauthorized());
    }

    match api_parse_json_body::<Vec<i32>>(body) {
        Err(e) => Json(APIResponse::bad_request(&e)),
        Ok(to_delete_mission_list) => {
            let result = web::block(move || {
                let mut conn = db_pool.get().map_err(|e| format!("cannot get db connection from pool: {}", e))?;

                for mission_id in to_delete_mission_list {
                    delete_mission::delete_mission(&mut conn, mission_id)?;
                }

                Ok::<_, String>(())
            })
                .await
                .unwrap();

            match result {
                Ok(()) => Json(APIResponse::ok(())),
                Err(e) => {
                    error!("cannot delete mission: {}", e);
                    Json(APIResponse::internal_error())
                }
            }
        }
    }
}

#[post("/set_mission_invalid")]
async fn api_set_mission_invalid(
    requests: HttpRequest,
    app_state: Data<AppState>,
    db_pool: Data<DbPool>,
    body: Bytes,
) -> Json<APIResponse<()>> {
    if !app_state.check_session(&requests) {
        return Json(APIResponse::unauthorized());
    }

    match api_parse_json_body::<APISetMissionInvalid>(body) {
        Err(e) => Json(APIResponse::bad_request(&e)),
        Ok(set_invalid) => {
            let result = web::block(move || {
                let mut conn = db_pool.get().map_err(|e| format!("cannot get db connection from pool: {}", e))?;

                if set_invalid.invalid {
                    if mission_invalid::check_invalid_record_exist(&mut conn, set_invalid.mission_id)? {
                        mission_invalid::delete_mission_invalid(&mut conn, set_invalid.mission_id)?;
                    }
                    mission_invalid::add_mission_invalid(&mut conn, set_invalid.mission_id, set_invalid.reason)?;
                } else {
                    mission_invalid::delete_mission_invalid(&mut conn, set_invalid.mission_id)?;
                }

                Ok::<_, String>(APIResponse::ok(()))
            })
                .await
                .unwrap();

            match result {
                Ok(response) => Json(response),
                Err(e) => {
                    error!("cannot set mission invalid: {}", e);
                    Json(APIResponse::internal_error())
                }
            }
        }
    }
}

#[get("/mission_invalid")]
async fn api_get_mission_invalid(
    requests: HttpRequest,
    app_state: Data<AppState>,
    db_pool: Data<DbPool>,
) -> Json<APIResponse<Vec<APIMissionInvalid>>> {
    if !app_state.check_session(&requests) {
        return Json(APIResponse::unauthorized());
    }

    let result = web::block(move || {
        let mut conn = db_pool.get().map_err(|e| format!("cannot get db connection from pool: {}", e))?;

        mission_invalid::get_mission_invalid(&mut conn)
    })
        .await
        .unwrap();

    match result {
        Ok(response) => Json(APIResponse::ok(response)),
        Err(e) => {
            error!("cannot get mission invalid: {}", e);
            Json(APIResponse::internal_error())
        }
    }
}

pub fn scoped_config(cfg: &mut web::ServiceConfig) {
    cfg.service(load_mapping);
    cfg.service(load_watchlist);
    cfg.service(load_kpi);
    cfg.service(api_delete_mission);
    cfg.service(api_set_mission_invalid);
    cfg.service(api_get_mission_invalid);
}

