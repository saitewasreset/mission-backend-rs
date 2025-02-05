use super::{MissionTypeData, MissionTypeInfo};
use crate::cache::mission::MissionCachedInfo;
use crate::db::models::MissionType;
use crate::db::schema::*;
use crate::hazard_id_to_real;
use crate::{APIResponse, DbPool};
use actix_web::{
    get,
    web::{self, Data, Json},
};
use diesel::prelude::*;
use std::collections::{HashMap, HashSet};
use crate::cache::manager::{get_db_redis_conn, CacheManager};

#[get("/mission_type")]
async fn get_mission_type(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
    cache_manager: Data<CacheManager>,
) -> Json<APIResponse<MissionTypeInfo>> {
    let mission_type_game_id_to_name = cache_manager.get_mapping().mission_type_mapping;

    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;


        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let invalid_mission_id_list: Vec<i32> = mission_invalid::table
            .select(mission_invalid::mission_id)
            .load(&mut db_conn)
            .map_err(|e| format!("cannot get invalid mission list from db: {}", e))?;


        let mission_type_list = mission_type::table
            .select(MissionType::as_select())
            .load(&mut db_conn)
            .map_err(|e| format!("cannot get mission type list from db: {}", e))?;

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

    Json(APIResponse::from_result(result, "cannot get mission type info"))
}

fn generate(
    cached_mission_list: &[MissionCachedInfo],
    invalid_mission_id_list: &[i32],
    mission_type_id_to_game_id: &HashMap<i16, String>,
    mission_type_game_id_to_name: HashMap<String, String>,
) -> MissionTypeInfo {
    let invalid_mission_id_set = invalid_mission_id_list
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let cached_mission_list = cached_mission_list
        .into_iter()
        .filter(|info| !invalid_mission_id_set.contains(&info.mission_info.id))
        .collect::<Vec<_>>();

    let mut mission_list_by_type: HashMap<i16, Vec<&MissionCachedInfo>> = HashMap::new();

    let mut result = HashMap::with_capacity(mission_list_by_type.len());

    for mission in cached_mission_list {
        let mission_type = mission.mission_info.mission_type_id;
        mission_list_by_type
            .entry(mission_type)
            .or_insert_with(Vec::new)
            .push(mission);
    }

    for (mission_type_id, mission_list) in mission_list_by_type {
        let total_difficulty = mission_list
            .iter()
            .map(|item| hazard_id_to_real(item.mission_info.hazard_id))
            .sum::<f64>();

        let total_mission_time = mission_list
            .iter()
            .map(|item| item.mission_info.mission_time as i32)
            .sum::<i32>();

        let total_reward_credit = mission_list
            .iter()
            .map(|item| item.mission_info.reward_credit)
            .sum::<f64>();

        let pass_count = mission_list
            .iter()
            .filter(|item| item.mission_info.result == 0)
            .count();
        let mission_count = mission_list.len();

        let mission_type_game_id = mission_type_id_to_game_id
            .get(&mission_type_id)
            .unwrap()
            .clone();
        result.insert(
            mission_type_game_id,
            MissionTypeData {
                average_difficulty: total_difficulty / mission_count as f64,
                average_mission_time: total_mission_time as f64 / mission_count as f64,
                average_reward_credit: total_reward_credit / mission_count as f64,
                credit_per_minute: total_reward_credit / (total_mission_time as f64 / 60.0),
                mission_count: mission_count as i32,
                pass_rate: pass_count as f64 / mission_count as f64,
            },
        );
    }

    MissionTypeInfo {
        mission_type_data: result,
        mission_type_map: mission_type_game_id_to_name,
    }
}
