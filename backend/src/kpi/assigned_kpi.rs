use actix_web::{get, post, web, HttpRequest};
use actix_web::web::{Bytes, Data, Json};
use log::error;
use common::APIResponse;
use crate::{api_parse_json_body, AppState, DbPool};
use std::collections::HashMap;
use diesel::associations::HasTable;
use diesel::prelude::*;
use common::kpi::{APIAssignedKPI, APIDeleteAssignedKPI};
use common::kpi::{KPIComponent, PlayerAssignedKPIInfo};
use crate::db::models::{AssignedKPI, Player};
use crate::db::schema::assigned_kpi;
use crate::DbConn;

#[derive(Insertable)]
#[diesel(table_name = assigned_kpi)]
struct NewAssignedKPI {
    pub mission_id: i32,
    pub player_id: i16,
    pub target_kpi_component: i16,
    pub kpi_component_delta_value: f64,
    pub total_delta_value: f64,
    pub note: Option<String>,
}

fn parse_api_assigned_kpi(api_assigned_kpi: APIAssignedKPI, player_id: i16) -> Vec<NewAssignedKPI> {
    let mut result = Vec::new();

    for (kpi_component, delta_value) in api_assigned_kpi.player_assigned_kpi_info.by_component {
        result.push(NewAssignedKPI {
            mission_id: api_assigned_kpi.mission_id,
            player_id,
            target_kpi_component: kpi_component.into(),
            kpi_component_delta_value: delta_value,
            total_delta_value: 0.0,
            note: Some(api_assigned_kpi.player_assigned_kpi_info.note.clone()),
        });
    }

    if let Some(first_element) = result.first_mut() {
        first_element.total_delta_value = api_assigned_kpi.player_assigned_kpi_info.overall.unwrap_or_default();
    }

    result
}

pub fn check_assigned_kpi_exist(db_conn: &mut DbConn, target_mission_id: i32, target_player_id: i16) -> Result<bool, String> {
    use crate::db::schema::assigned_kpi::dsl::*;

    let assigned_kpi_record = AssignedKPI::table()
        .select(AssignedKPI::as_select())
        .filter(mission_id.eq(target_mission_id))
        .filter(player_id.eq(target_player_id))
        .first(db_conn)
        .optional()
        .map_err(|e| format!("cannot query assigned_kpi: {}", e))?;

    Ok(assigned_kpi_record.is_some())
}

pub fn add_assigned_kpi(db_conn: &mut DbConn, api_assigned_kpi: APIAssignedKPI, player_id: i16) -> Result<(), String> {
    let new_assigned_kpi_list = parse_api_assigned_kpi(api_assigned_kpi, player_id);

    diesel::insert_into(assigned_kpi::table)
        .values(&new_assigned_kpi_list)
        .execute(db_conn)
        .map_err(|e| format!("cannot insert assigned_kpi: {}", e))?;

    Ok(())
}

pub fn delete_assigned_kpi(db_conn: &mut DbConn, target: APIDeleteAssignedKPI, target_player_id: i16) -> Result<(), String> {
    use crate::db::schema::assigned_kpi::dsl::*;

    diesel::delete(assigned_kpi.filter(mission_id.eq(target.mission_id)).filter(player_id.eq(target_player_id)))
        .execute(db_conn)
        .map_err(|e| format!("cannot delete assigned_kpi: {}", e))?;

    Ok(())
}

pub fn get_player_id(db_conn: &mut DbConn, target_player_name: &str) -> Result<Option<i16>, String> {
    use crate::db::schema::player::dsl::*;

    let result = player
        .select(Player::as_select())
        .filter(player_name.eq(target_player_name))
        .first(db_conn)
        .optional()
        .map_err(|e| format!("cannot query player: {}", e))?;

    Ok(result.map(|p| p.id))
}

pub fn get_assigned_kpi_info(db_conn: &mut DbConn) -> Result<Vec<APIAssignedKPI>, String> {
    let player_list = Player::table()
        .select(Player::as_select())
        .load(db_conn)
        .map_err(|e| format!("cannot query player: {}", e))?;

    let player_id_to_name = player_list
        .into_iter()
        .map(|p| (p.id, p.player_name))
        .collect::<HashMap<i16, String>>();

    let db_result = AssignedKPI::table()
        .select(AssignedKPI::as_select())
        .load(db_conn)
        .map_err(|e| format!("cannot query assigned_kpi: {}", e))?;

    let mut identity_to_info: HashMap<(i32, String), PlayerAssignedKPIInfo> = HashMap::new();

    for assigned_kpi in db_result {
        let mission_id = assigned_kpi.mission_id;
        let player_name = player_id_to_name.get(&assigned_kpi.player_id).cloned().unwrap_or("Unknown".to_string());

        let entry = identity_to_info.entry((mission_id, player_name)).or_insert_with(|| {
            PlayerAssignedKPIInfo {
                by_component: HashMap::new(),
                overall: None,
                note: assigned_kpi.note.clone().unwrap_or_default(),
            }
        });

        if assigned_kpi.kpi_component_delta_value != 0.0 {
            entry.by_component.insert(
                (assigned_kpi.target_kpi_component as usize)
                    .try_into()
                    .unwrap_or(KPIComponent::Kill),
                assigned_kpi.kpi_component_delta_value);
        }

        if assigned_kpi.total_delta_value != 0.0 {
            entry.overall = Some(assigned_kpi.total_delta_value);
        }
    }

    Ok(identity_to_info.into_iter().map(|((mission_id, player_name), info)| {
        APIAssignedKPI {
            mission_id,
            player_name,
            player_assigned_kpi_info: info,
        }
    }).collect())
}

#[post("/set_assigned_kpi")]
pub async fn api_set_assigned_kpi(
    requests: HttpRequest,
    app_state: Data<AppState>,
    db_pool: Data<DbPool>,
    body: Bytes,
) -> Json<APIResponse<()>> {
    if !app_state.check_session(&requests) {
        return Json(APIResponse::unauthorized());
    }

    match api_parse_json_body::<APIAssignedKPI>(body) {
        Err(e) => Json(APIResponse::bad_request(&e)),
        Ok(set_assigned_kpi) => {
            let result = web::block(move || {
                let mut conn = db_pool.get().map_err(|e| format!("cannot get db connection from pool: {}", e))?;

                if let Some(player_id) = get_player_id(&mut conn, &set_assigned_kpi.player_name)? {
                    if check_assigned_kpi_exist(&mut conn, set_assigned_kpi.mission_id, player_id)? {
                        return Ok(APIResponse::bad_request("assigned kpi already exist"));
                    }

                    add_assigned_kpi(&mut conn, set_assigned_kpi, player_id)?;

                    Ok::<_, String>(APIResponse::ok(()))
                } else {
                    Ok(APIResponse::bad_request("player does not exist"))
                }
            })
                .await
                .unwrap();

            match result {
                Ok(response) => Json(response),
                Err(e) => {
                    error!("cannot set assigned kpi: {}", e);
                    Json(APIResponse::internal_error())
                }
            }
        }
    }
}

#[post("/delete_assigned_kpi")]
pub async fn api_delete_assigned_kpi(
    requests: HttpRequest,
    app_state: Data<AppState>,
    db_pool: Data<DbPool>,
    body: Bytes,
) -> Json<APIResponse<()>> {
    if !app_state.check_session(&requests) {
        return Json(APIResponse::unauthorized());
    }

    match api_parse_json_body::<APIDeleteAssignedKPI>(body) {
        Err(e) => Json(APIResponse::bad_request(&e)),
        Ok(set_assigned_kpi) => {
            let result = web::block(move || {
                let mut conn = db_pool.get().map_err(|e| format!("cannot get db connection from pool: {}", e))?;

                if let Some(player_id) = get_player_id(&mut conn, &set_assigned_kpi.player_name)? {
                    if !check_assigned_kpi_exist(&mut conn, set_assigned_kpi.mission_id, player_id)? {
                        return Ok(APIResponse::bad_request("target does not exist"));
                    }

                    delete_assigned_kpi(&mut conn, set_assigned_kpi, player_id)?;

                    Ok::<_, String>(APIResponse::ok(()))
                } else {
                    Ok(APIResponse::bad_request("player does not exist"))
                }
            })
                .await
                .unwrap();

            match result {
                Ok(response) => Json(response),
                Err(e) => {
                    error!("cannot delete assigned kpi: {}", e);
                    Json(APIResponse::internal_error())
                }
            }
        }
    }
}

#[get("/assigned_kpi")]
pub async fn api_get_assigned_kpi(
    db_pool: Data<DbPool>,
) -> Json<APIResponse<Vec<APIAssignedKPI>>> {
    let result = web::block(move || {
        let mut conn = db_pool.get().map_err(|e| format!("cannot get db connection from pool: {}", e))?;

        get_assigned_kpi_info(&mut conn)
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