pub mod delete_mission;

use crate::{db::schema::player, APIResponse, AppState, DbPool};
use actix_web::{
    post,
    web::{self, Buf, Bytes, Data, Json},
    HttpRequest,
};
use diesel::prelude::*;
use diesel::{insert_into, update};
use log::{error, warn};
use std::fs;
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

    let to_delete_mission_list: Vec<i32> = match serde_json::from_reader(body.reader()) {
        Ok(x) => x,
        Err(e) => {
            warn!("cannot parse payload body as json: {}", e);
            return Json(APIResponse::bad_request(
                "cannot parse payload body as json",
            ));
        }
    };

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

pub fn scoped_config(cfg: &mut web::ServiceConfig) {
    cfg.service(load_mapping);
    cfg.service(load_watchlist);
    cfg.service(load_kpi);
    cfg.service(api_delete_mission);
}

pub fn api_parse_json_body<T: serde::de::DeserializeOwned>(
    body: Bytes,
) -> Result<T, String> {
    match serde_json::from_reader(body.reader()) {
        Ok(x) => Ok(x),
        Err(e) => {
            warn!("cannot parse payload body as json: {}", e);
            Err("cannot parse payload body as json".to_string())
        }
    }
}