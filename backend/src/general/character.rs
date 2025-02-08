use common::general::{CharacterChoiceInfo, CharacterGeneralData, CharacterGeneralInfo};
use crate::cache::mission::MissionCachedInfo;
use crate::db::models::*;
use crate::db::schema::*;
use crate::{APIResponse, DbPool};
use actix_web::{
    get,
    web::{self, Data, Json},
};
use diesel::prelude::*;
use std::collections::{HashMap, HashSet};
use crate::cache::manager::{get_db_redis_conn, CacheManager};

#[get("/character")]
async fn get_character_general_info(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
    cache_manager: Data<CacheManager>,
) -> Json<APIResponse<CharacterGeneralInfo>> {
    let character_game_id_to_name = cache_manager.get_mapping().character_mapping;

    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;


        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let invalid_mission_id_list: Vec<i32> = mission_invalid::table
            .select(mission_invalid::mission_id)
            .load(&mut db_conn).map_err(|e| format!("cannot get invalid mission list from db: {}", e))?;


        let character_list = character::table
            .select(Character::as_select())
            .load(&mut db_conn).map_err(|e| format!("cannot get character list from db: {}", e))?;


        let character_id_to_game_id = character_list
            .into_iter()
            .map(|x| (x.id, x.character_game_id))
            .collect::<HashMap<_, _>>();


        let result = generate(
            &cached_mission_list,
            &invalid_mission_id_list,
            &character_id_to_game_id,
            character_game_id_to_name,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    Json(APIResponse::from_result(result, "cannot get character general info"))
}

#[get("/character_info")]
async fn get_character_choice_info(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
    cache_manager: Data<CacheManager>,
) -> Json<APIResponse<CharacterChoiceInfo>> {
    let character_game_id_to_name = cache_manager.get_mapping().character_mapping;

    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;


        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let invalid_mission_id_list: Vec<i32> = mission_invalid::table
            .select(mission_invalid::mission_id)
            .load(&mut db_conn).map_err(|e| format!("cannot get invalid mission list from db: {}", e))?;


        let character_list = character::table
            .select(Character::as_select())
            .load(&mut db_conn).map_err(|e| format!("cannot get character list from db: {}", e))?;


        let character_id_to_game_id = character_list
            .into_iter()
            .map(|x| (x.id, x.character_game_id))
            .collect::<HashMap<_, _>>();

        let result = generate_choice_info(
            &cached_mission_list,
            &invalid_mission_id_list,
            &character_id_to_game_id,
            character_game_id_to_name,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    Json(APIResponse::from_result(result, "cannot get character choice info"))
}

fn generate(
    cached_mission_list: &[MissionCachedInfo],
    invalid_mission_id_list: &[i32],
    character_id_to_game_id: &HashMap<i16, String>,
    character_game_id_to_name: HashMap<String, String>,
) -> CharacterGeneralInfo {
    let invalid_mission_id_set = invalid_mission_id_list
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let cached_mission_list = cached_mission_list
        .iter()
        .filter(|info| !invalid_mission_id_set.contains(&info.mission_info.id))
        .collect::<Vec<_>>();

    let mut player_index_list_by_character: HashMap<&String, Vec<f64>> = HashMap::new();
    let mut revive_num_list_by_character: HashMap<&String, Vec<i32>> = HashMap::new();
    let mut death_num_list_by_character: HashMap<&String, Vec<i32>> = HashMap::new();
    let mut minerals_mined_list_by_character: HashMap<&String, Vec<f64>> = HashMap::new();
    let mut supply_count_list_by_character: HashMap<&String, Vec<i32>> = HashMap::new();
    let mut supply_efficiency_list_by_character: HashMap<&String, Vec<f64>> = HashMap::new();

    for mission in cached_mission_list {
        for player_info in &mission.player_info {
            let character_game_id = character_id_to_game_id
                .get(&player_info.character_id)
                .unwrap();

            player_index_list_by_character
                .entry(character_game_id)
                .or_default()
                .push(
                    mission
                        .player_index
                        .get(&player_info.player_id)
                        .copied()
                        .unwrap_or(0.0),
                );
            revive_num_list_by_character
                .entry(character_game_id)
                .or_default()
                .push(player_info.revive_num as i32);
            death_num_list_by_character
                .entry(character_game_id)
                .or_default()
                .push(player_info.death_num as i32);
            minerals_mined_list_by_character
                .entry(character_game_id)
                .or_default()
                .push(match mission.resource_info.get(&player_info.player_id) {
                    Some(x) => x.values().sum::<f64>(),
                    None => 0.0,
                });
            supply_count_list_by_character
                .entry(character_game_id)
                .or_default()
                .push(match mission.supply_info.get(&player_info.player_id) {
                    Some(x) => x.len() as i32,
                    None => 0,
                });

            let player_supply_efficiency_list = mission
                .supply_info
                .get(&player_info.player_id)
                .into_iter()
                .flatten()
                .map(|x| 2.0 * x.ammo)
                .collect::<Vec<_>>();

            supply_efficiency_list_by_character
                .entry(character_game_id)
                .or_default()
                .extend(player_supply_efficiency_list);
        }
    }

    let mut character_data = HashMap::new();

    for &character_game_id in player_index_list_by_character.keys() {
        character_data.insert(
            character_game_id.clone(),
            CharacterGeneralData {
                player_index: player_index_list_by_character[character_game_id]
                    .iter()
                    .sum::<f64>(),
                revive_num: revive_num_list_by_character[character_game_id]
                    .iter()
                    .sum::<i32>() as f64
                    / revive_num_list_by_character[character_game_id].len() as f64,
                death_num: death_num_list_by_character[character_game_id]
                    .iter()
                    .sum::<i32>() as f64
                    / death_num_list_by_character[character_game_id].len() as f64,
                minerals_mined: minerals_mined_list_by_character[character_game_id]
                    .iter()
                    .sum::<f64>()
                    / minerals_mined_list_by_character[character_game_id].len() as f64,
                supply_count: supply_count_list_by_character[character_game_id]
                    .iter()
                    .sum::<i32>() as f64
                    / supply_count_list_by_character[character_game_id].len() as f64,
                supply_efficiency: supply_efficiency_list_by_character[character_game_id]
                    .iter()
                    .sum::<f64>()
                    / supply_efficiency_list_by_character[character_game_id].len() as f64,
            },
        );
    }

    CharacterGeneralInfo {
        character_data,
        character_mapping: character_game_id_to_name,
    }
}

fn generate_choice_info(
    cached_mission_list: &[MissionCachedInfo],
    invalid_mission_id_list: &[i32],
    character_id_to_game_id: &HashMap<i16, String>,
    character_game_id_to_name: HashMap<String, String>,
) -> CharacterChoiceInfo {
    let invalid_mission_id_set = invalid_mission_id_list
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let cached_mission_list = cached_mission_list
        .iter()
        .filter(|info| !invalid_mission_id_set.contains(&info.mission_info.id))
        .collect::<Vec<_>>();

    let mut character_choice_count: HashMap<String, i32> = HashMap::new();

    for mission in cached_mission_list {
        for player_info in &mission.player_info {
            let character_game_id = character_id_to_game_id
                .get(&player_info.character_id)
                .unwrap();

            *character_choice_count
                .entry(character_game_id.clone())
                .or_default() += 1;
        }
    }

    CharacterChoiceInfo {
        character_choice_count,
        character_mapping: character_game_id_to_name,
    }
}
