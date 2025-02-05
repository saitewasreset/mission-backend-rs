use super::{APIMission, MissionInfo, MissionList};
use crate::cache::mission::MissionCachedInfo;
use crate::{
    db::models::{Mission, MissionInvalid, MissionType},
    db::schema::*,
    APIResponse, DbPool,
};
use actix_web::{
    get,
    web::{self, Data, Json},
};
use diesel::prelude::*;
use diesel::{RunQueryDsl, SelectableHelper};
use log::error;
use std::collections::HashMap;
use std::sync::Arc;
use crate::cache::manager::{get_db_redis_conn, CacheManager};

#[get("/api_mission_list")]
async fn get_api_mission_list(db_pool: Data<DbPool>) -> Json<APIResponse<Vec<APIMission>>> {
    let inner_pool = (*db_pool).clone();

    let mission_type_map = match web::block(|| load_mission_type_map(inner_pool))
        .await
        .unwrap()
    {
        Ok(x) => x,
        Err(()) => {
            return Json(APIResponse::internal_error());
        }
    };

    let inner_pool = (*db_pool).clone();
    let mission_list = match web::block(|| load_mission_list(inner_pool)).await.unwrap() {
        Ok(x) => x,
        Err(()) => {
            return Json(APIResponse::internal_error());
        }
    };

    let result: Vec<APIMission> = mission_list
        .into_iter()
        .map(|item| APIMission::from_mission(&mission_type_map, item))
        .collect();

    Json(APIResponse::ok(result))
}

fn load_mission_list(db_pool: Arc<DbPool>) -> Result<Vec<Mission>, ()> {
    use crate::db::schema::*;
    let mut conn = match db_pool.get() {
        Ok(conn) => conn,
        Err(e) => {
            error!("cannot get db connection from pool: {}", e);
            return Err(());
        }
    };

    match mission::table.load(&mut conn) {
        Ok(data) => Ok(data),
        Err(e) => {
            error!("cannot load mission from db: {}", e);
            Err(())
        }
    }
}

fn load_mission_type_map(db_pool: Arc<DbPool>) -> Result<HashMap<i16, String>, ()> {
    use crate::db::schema::*;
    let mut conn = match db_pool.get() {
        Ok(conn) => conn,
        Err(e) => {
            error!("cannot get db connection from pool: {}", e);
            return Err(());
        }
    };

    let mission_type_list: Vec<MissionType> = match mission_type::table.load(&mut conn) {
        Ok(x) => x,
        Err(e) => {
            error!("cannot load mission type from db: {}", e);
            return Err(());
        }
    };

    let mut table = HashMap::with_capacity(mission_type_list.len());

    for mission_type in mission_type_list {
        table.insert(mission_type.id, mission_type.mission_type_game_id);
    }

    Ok(table)
}

#[get("/mission_list")]
async fn get_mission_list(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
    cache_manager: Data<CacheManager>,
) -> Json<APIResponse<MissionList>> {
    let mission_type_game_id_to_name = cache_manager.get_mapping().mission_type_mapping;

    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;


        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let invalid_mission_id_list = mission_invalid::table
            .select(MissionInvalid::as_select())
            .load(&mut db_conn).map_err(|e| format!("cannot get invalid mission list from db: {}", e))?;


        let mission_type_list = mission_type::table
            .select(MissionType::as_select())
            .load(&mut db_conn).map_err(|e| format!("cannot get mission type list from db: {}", e))?;

        let mission_type_id_to_game_id = mission_type_list
            .into_iter()
            .map(|item| (item.id, item.mission_type_game_id))
            .collect::<HashMap<_, _>>();

        let result = generate(
            &cached_mission_list,
            &invalid_mission_id_list,
            &mission_type_id_to_game_id,
            mission_type_game_id_to_name,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    match result {
        Ok(x) => Json(APIResponse::ok(x)),
        Err(e) => {
            error!("cannot get mission list: {}", e);
            Json(APIResponse::internal_error())
        }
    }
}

pub fn generate(
    cached_mission_list: &[MissionCachedInfo],
    invalid_mission_list: &[MissionInvalid],
    mission_type_id_to_game_id: &HashMap<i16, String>,
    mission_type_game_id_to_name: HashMap<String, String>,
) -> MissionList {
    let mut mission_list = Vec::with_capacity(cached_mission_list.len());

    let invalid_mission_id_map = invalid_mission_list
        .into_iter()
        .map(|item| (item.mission_id, item))
        .collect::<HashMap<_, _>>();

    for mission in cached_mission_list {
        let current_mission_info = &mission.mission_info;

        let mission_invalid = invalid_mission_id_map.contains_key(&current_mission_info.id);
        let mission_invalid_reason = match mission_invalid {
            true => invalid_mission_id_map
                .get(&current_mission_info.id)
                .map(|item| item.reason.clone())
                .unwrap_or_else(|| "".to_string()),
            false => "".to_string(),
        };

        let mission_type_id = mission_type_id_to_game_id
            .get(&current_mission_info.mission_type_id)
            .unwrap();

        mission_list.push(MissionInfo {
            mission_id: current_mission_info.id,
            begin_timestamp: current_mission_info.begin_timestamp,
            mission_time: current_mission_info.mission_time,
            mission_type_id: mission_type_id.clone(),
            hazard_id: current_mission_info.hazard_id,
            mission_result: current_mission_info.result,
            reward_credit: current_mission_info.reward_credit,
            mission_invalid,
            mission_invalid_reason,
        });
    }

    MissionList {
        mission_info: mission_list,
        mission_type_mapping: mission_type_game_id_to_name,
    }
}
