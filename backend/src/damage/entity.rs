use common::damage::EntityDamageInfo;
use crate::cache::mission::MissionCachedInfo;
use crate::db::schema::*;
use crate::{APIResponse, DbPool};
use actix_web::{
    get,
    web::{self, Data, Json},
};
use diesel::prelude::*;
use std::collections::{HashMap, HashSet};
use crate::cache::manager::{get_db_redis_conn, CacheManager};

#[get("/entity")]
async fn get_damage_entity(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
    cache_manager: Data<CacheManager>,
) -> Json<APIResponse<EntityDamageInfo>> {
    let entity_mapping = cache_manager.get_mapping().entity_mapping;

    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;

        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let invalid_mission_id_list: Vec<i32> = mission_invalid::table
            .select(mission_invalid::mission_id)
            .load(&mut db_conn).map_err(|e| format!("cannot get invalid mission list from db: {}", e))?;


        let result = generate(
            &cached_mission_list,
            &invalid_mission_id_list,
            entity_mapping,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    Json(APIResponse::from_result(result, "cannot get entity damage info"))
}

fn generate(
    cached_mission_list: &[MissionCachedInfo],
    invalid_mission_id_list: &[i32],
    entity_game_id_to_name: HashMap<String, String>,
) -> EntityDamageInfo {
    let invalid_mission_id_set = invalid_mission_id_list
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let cached_mission_list = cached_mission_list
        .iter()
        .filter(|item| !invalid_mission_id_set.contains(&item.mission_info.id))
        .collect::<Vec<_>>();

    let mut damage_map: HashMap<&String, f64> = HashMap::new();
    let mut kill_map: HashMap<&String, i32> = HashMap::new();

    for mission in cached_mission_list {
        for data in mission.damage_info.values() {
            for (entity_game_id, pack) in data {
                if pack.taker_type != 1 {
                    let entry = damage_map.entry(entity_game_id).or_default();
                    *entry += pack.total_amount;
                }
            }
        }

        for data in mission.kill_info.values() {
            for (entity_game_id, pack) in data {
                let entry = kill_map.entry(entity_game_id).or_default();
                *entry += pack.total_amount;
            }
        }
    }

    EntityDamageInfo {
        damage: damage_map
            .into_iter()
            .map(|(k, v)| (k.clone(), v))
            .collect(),
        kill: kill_map.into_iter().map(|(k, v)| (k.clone(), v)).collect(),
        entity_mapping: entity_game_id_to_name,
    }
}
