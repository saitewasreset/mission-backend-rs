use super::{PlayerData, PlayerInfo};
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

#[get("/player")]
async fn get_player(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
    cache_manager: Data<CacheManager>,
) -> Json<APIResponse<PlayerInfo>> {
    let character_game_id_to_name = cache_manager.get_mapping().character_mapping;


    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;


        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let invalid_mission_id_list: Vec<i32> = mission_invalid::table
            .select(mission_invalid::mission_id)
            .load(&mut db_conn)
            .map_err(|e| format!("cannot get invalid mission list from db: {}", e))?;

        let player_list = player::table
            .select(Player::as_select())
            .load(&mut db_conn)
            .map_err(|e| format!("cannot get player list from db: {}", e))?;

        let watchlist_player_id_list: Vec<i16> = player_list
            .iter()
            .filter(|x| x.friend)
            .map(|x| x.id)
            .collect();

        let player_id_to_name = player_list
            .into_iter()
            .map(|x| (x.id, x.player_name))
            .collect();

        let character_list = character::table
            .select(Character::as_select())
            .load(&mut db_conn)
            .map_err(|e| format!("cannot get character list from db: {}", e))?;

        let character_id_to_game_id = character_list
            .into_iter()
            .map(|x| (x.id, x.character_game_id))
            .collect::<HashMap<_, _>>();

        let result = generate(
            &cached_mission_list,
            &invalid_mission_id_list,
            &watchlist_player_id_list,
            &player_id_to_name,
            &character_id_to_game_id,
            character_game_id_to_name,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    Json(APIResponse::from_result(result, "cannot get player info"))
}

fn generate(
    cached_mission_list: &[MissionCachedInfo],
    invalid_mission_id_list: &[i32],
    watchlist_player_id_list: &[i16],
    player_id_to_name: &HashMap<i16, String>,
    character_id_to_game_id: &HashMap<i16, String>,
    character_game_id_to_name: HashMap<String, String>,
) -> PlayerInfo {
    let invalid_mission_id_set = invalid_mission_id_list
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let watchlist_player_id_set = watchlist_player_id_list
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let cached_mission_list = cached_mission_list
        .iter()
        .filter(|item| !invalid_mission_id_set.contains(&item.mission_info.id))
        .collect::<Vec<_>>();

    let mut mission_list_by_player: HashMap<i16, Vec<&MissionCachedInfo>> = HashMap::new();

    for mission in cached_mission_list {
        for player_info in &mission.player_info {
            if !watchlist_player_id_set.contains(&player_info.player_id) {
                continue;
            }
            mission_list_by_player
                .entry(player_info.player_id)
                .or_default()
                .push(mission);
        }
    }

    let mut overall_player_data_map = HashMap::with_capacity(mission_list_by_player.len());
    let mut prev_player_data_map = HashMap::with_capacity(mission_list_by_player.len());

    for (player_id, player_mission_list) in mission_list_by_player {
        let prev_count = match player_mission_list.len() * 8 / 10 {
            0..10 => 10,
            x => x,
        };

        let prev_count = if prev_count > player_mission_list.len() {
            player_mission_list.len()
        } else {
            prev_count
        };

        let prev_mission_list = &player_mission_list[0..prev_count];

        let overall_data =
            generate_for_player(&player_mission_list[..], character_id_to_game_id, player_id);
        let prev_data = generate_for_player(prev_mission_list, character_id_to_game_id, player_id);

        let player_name = player_id_to_name.get(&player_id).unwrap();

        overall_player_data_map.insert(player_name.clone(), overall_data);
        prev_player_data_map.insert(player_name.clone(), prev_data);
    }

    PlayerInfo {
        character_map: character_game_id_to_name,
        player_data: overall_player_data_map,
        prev_player_data: prev_player_data_map,
    }
}

fn generate_for_player(
    player_mission_list: &[&MissionCachedInfo],
    character_id_to_game_id: &HashMap<i16, String>,
    player_id: i16,
) -> PlayerData {
    let average_death_num = player_mission_list
        .iter()
        .map(|item| {
            for player_info in &item.player_info {
                if player_info.player_id == player_id {
                    return player_info.death_num as i32;
                }
            }
            unreachable!();
        })
        .sum::<i32>() as f64
        / player_mission_list.len() as f64;

    let average_minerals_mined = player_mission_list
        .iter()
        .map(|item| match item.resource_info.get(&player_id) {
            Some(info) => info.values().sum::<f64>(),
            None => 0.0,
        })
        .sum::<f64>()
        / player_mission_list.len() as f64;

    let average_revive_num = player_mission_list
        .iter()
        .map(|item| {
            for player_info in &item.player_info {
                if player_info.player_id == player_id {
                    return player_info.revive_num as i32;
                }
            }
            unreachable!();
        })
        .sum::<i32>() as f64
        / player_mission_list.len() as f64;

    let average_supply_count = player_mission_list
        .iter()
        .map(|item| match item.supply_info.get(&player_id) {
            Some(info) => info.len(),
            None => 0,
        })
        .sum::<usize>() as f64
        / player_mission_list.len() as f64;

    let supply_efficiency_list: Vec<f64> = player_mission_list
        .iter()
        .flat_map(|item| item.supply_info.get(&player_id).into_iter().flatten())
        .map(|x| x.ammo)
        .collect();

    let average_supply_efficiency =
        2.0 * supply_efficiency_list.iter().sum::<f64>() / supply_efficiency_list.len() as f64;

    let mut character_info: HashMap<&String, i32> = HashMap::new();

    for mission in player_mission_list {
        for player_info in &mission.player_info {
            if player_info.player_id == player_id {
                let entry = character_info
                    .entry(
                        character_id_to_game_id
                            .get(&player_info.character_id)
                            .unwrap(),
                    )
                    .or_default();

                *entry += 1;
            }
        }
    }

    PlayerData {
        average_death_num,
        average_minerals_mined,
        average_revive_num,
        average_supply_count,
        average_supply_efficiency,
        character_info: character_info
            .into_iter()
            .map(|(k, v)| (k.clone(), v))
            .collect(),
        valid_mission_count: player_mission_list.len() as i32,
    }
}
