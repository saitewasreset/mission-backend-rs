use super::WeaponDamageInfo;
use crate::cache::mission::MissionCachedInfo;
use crate::db::schema::*;
use crate::{APIResponse, DbPool};
use actix_web::web;
use actix_web::{
    get,
    web::{Data, Json},
};
use diesel::prelude::*;
use log::error;
use std::collections::{HashMap, HashSet};
use crate::cache::manager::{get_db_redis_conn, CacheManager};

#[get("/weapon")]
async fn get_damage_weapon(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
    cache_manager: Data<CacheManager>,
) -> Json<APIResponse<HashMap<String, WeaponDamageInfo>>> {
    let mapping = cache_manager.get_mapping();

    let weapon_game_id_to_character_game_id = mapping.weapon_character.clone();
    let weapon_mapping = mapping.weapon_mapping.clone();

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
            &weapon_game_id_to_character_game_id,
            &weapon_mapping,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    match result {
        Ok(x) => Json(APIResponse::ok(x)),
        Err(e) => {
            error!("cannot get weapon damage info: {}", e);
            Json(APIResponse::internal_error())
        }
    }
}

fn generate(
    cached_mission_list: &[MissionCachedInfo],
    invalid_mission_id_list: &[i32],
    weapon_game_id_to_character_game_id: &HashMap<String, String>,
    weapon_mapping: &HashMap<String, String>,
) -> HashMap<String, WeaponDamageInfo> {
    let invalid_mission_id_set = invalid_mission_id_list
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let cached_mission_list = cached_mission_list
        .into_iter()
        .filter(|item| !invalid_mission_id_set.contains(&item.mission_info.id))
        .collect::<Vec<_>>();

    let mut result = HashMap::new();

    for mission in cached_mission_list {
        for (weapon_game_id, pack) in &mission.weapon_damage_info {
            let damage = pack
                .detail
                .values()
                .filter(|&val| val.taker_type != 1)
                .map(|val| val.total_amount)
                .sum::<f64>();

            let friendly_fire = pack
                .detail
                .values()
                .filter(|&val| val.taker_type == 1)
                .map(|val| val.total_amount)
                .sum::<f64>();

            let hero_game_id = weapon_game_id_to_character_game_id
                .get(weapon_game_id)
                .map(|inner| inner.clone())
                .unwrap_or(String::from("Unknown"));

            let mapped_name = weapon_mapping
                .get(weapon_game_id)
                .map(|inner| inner.clone())
                .unwrap_or(weapon_game_id.clone());

            let entry = result.entry(weapon_game_id).or_insert(WeaponDamageInfo {
                damage,
                friendly_fire,
                hero_game_id,
                mapped_name,
                valid_game_count: 0,
            });

            entry.damage += damage;
            entry.friendly_fire += friendly_fire;
            entry.valid_game_count += 1;
        }
    }

    result.into_iter().map(|(k, v)| (k.clone(), v)).collect()
}
