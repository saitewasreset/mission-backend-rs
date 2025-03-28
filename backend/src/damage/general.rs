use common::damage::{FriendlyFireData, OverallDamageInfo, PlayerDamageInfo, PlayerFriendlyFireInfo};
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

struct MissionFriendlyFireInfo {
    pub causer_id: i16,
    pub taker_id: i16,
    pub causer_name: String,
    pub taker_name: String,
    pub total_amount: f64,
}

#[get("/")]
async fn get_overall_damage_info(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
    cache_manager: Data<CacheManager>,
) -> Json<APIResponse<OverallDamageInfo>> {
    let entity_mapping = cache_manager.get_mapping().entity_mapping;

    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;


        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let invalid_mission_id_list: Vec<i32> = mission_invalid::table
            .select(mission_invalid::mission_id)
            .load(&mut db_conn)
            .map_err(|e| format!("cannot get invalid mission list from db: {}", e))?;

        let player_list: Vec<Player> =
            player::table.select(Player::as_select()).load(&mut db_conn)
                .map_err(|e| format!("cannot get player list from db: {}", e))?;

        let player_id_list = player_list
            .iter()
            .filter(|item| item.friend)
            .map(|item| item.id)
            .collect::<Vec<_>>();

        let player_id_to_name = player_list
            .iter()
            .map(|item| (item.id, item.player_name.clone()))
            .collect::<HashMap<_, _>>();


        let result = generate_for_mission_list(
            &cached_mission_list,
            &invalid_mission_id_list,
            &player_id_list,
            &player_id_to_name,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();


    Json(APIResponse::from_result(result.map(|(prev, overall)| {
        OverallDamageInfo {
            info: overall,
            prev_info: prev,
            entity_mapping,
        }
    }), "cannot get overall damage info"))
}

fn generate_for_mission_list(
    cached_mission_list: &[MissionCachedInfo],
    invalid_mission_id_list: &[i32],
    player_id_list: &[i16],
    player_id_to_name: &HashMap<i16, String>,
) -> (
    HashMap<String, PlayerDamageInfo>,
    HashMap<String, PlayerDamageInfo>,
) {
    let invalid_mission_set = invalid_mission_id_list
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let cached_mission_list = cached_mission_list
        .iter()
        .filter(|item| !invalid_mission_set.contains(&item.mission_info.id))
        .collect::<Vec<_>>();

    let mut mission_by_player: HashMap<i16, Vec<&MissionCachedInfo>> =
        HashMap::with_capacity(player_id_list.len());

    let player_id_set = player_id_list.iter().copied().collect::<HashSet<_>>();

    for mission in cached_mission_list {
        for player_info in &mission.player_info {
            if player_id_set.contains(&player_info.player_id) {
                mission_by_player
                    .entry(player_info.player_id)
                    .or_default()
                    .push(mission);
            }
        }
    }

    let mut overall = HashMap::with_capacity(player_id_list.len());
    let mut prev = HashMap::with_capacity(player_id_list.len());

    for (player_id, player_mission_list) in mission_by_player {
        let overall_list = &player_mission_list[..];

        let mut recent_count = player_mission_list.len() / 10;

        if recent_count < 10 {
            recent_count = 10.min(player_mission_list.len());
        }

        let prev_limit = player_mission_list.len() - recent_count;

        let prev_list = &player_mission_list[..prev_limit];

        overall.insert(
            player_id_to_name.get(&player_id).unwrap().clone(),
            generate_for_player(player_id, &player_id_set, player_id_to_name, overall_list),
        );

        prev.insert(
            player_id_to_name.get(&player_id).unwrap().clone(),
            generate_for_player(player_id, &player_id_set, player_id_to_name, prev_list),
        );
    }

    (prev, overall)
}

fn generate_for_player(
    player_id: i16,
    friend_player_id_set: &HashSet<i16>,
    player_id_to_name: &HashMap<i16, String>,
    player_cached_mission_list: &[&MissionCachedInfo],
) -> PlayerDamageInfo {
    let player_name_to_id = player_id_to_name
        .iter()
        .map(|(k, v)| (v, *k))
        .collect::<HashMap<_, _>>();

    let mut damage_map: HashMap<String, f64> = HashMap::new();
    let mut kill_map: HashMap<String, i32> = HashMap::new();

    let mut mission_ff_map: HashMap<i32, Vec<MissionFriendlyFireInfo>> = HashMap::new();

    let mut ff_cause_map: HashMap<String, FriendlyFireData> = HashMap::new();
    let mut ff_take_map: HashMap<String, FriendlyFireData> = HashMap::new();

    for cached_mission_info in player_cached_mission_list {
        if let Some(damage_by_entity) = cached_mission_info.damage_info.get(&player_id) {
            damage_by_entity
                .iter()
                .filter(|(_, &pack)| pack.taker_type != 1)
                .for_each(|(entity_game_id, &pack)| {
                    let entry = damage_map.entry(entity_game_id.clone()).or_default();

                    *entry += pack.total_amount;
                })
        }

        if let Some(kill_by_entity) = cached_mission_info.kill_info.get(&player_id) {
            kill_by_entity.iter().for_each(|(entity_game_id, pack)| {
                let entry = kill_map.entry(entity_game_id.clone()).or_default();

                *entry += pack.total_amount;
            })
        }

        cached_mission_info
            .damage_info
            .iter()
            .for_each(|(causer_player_id, taker_map)| {
                let causer_player_name = player_id_to_name.get(causer_player_id).unwrap();
                taker_map
                    .iter()
                    .filter(|(_, &pack)| pack.taker_type == 1)
                    .for_each(|(taker_name, pack)| {
                        let mission_ff_list = mission_ff_map
                            .entry(cached_mission_info.mission_info.id)
                            .or_default();
                        mission_ff_list.push(MissionFriendlyFireInfo {
                            causer_id: *causer_player_id,
                            taker_id: *player_name_to_id.get(&taker_name).unwrap(),
                            causer_name: causer_player_name.clone(),
                            taker_name: taker_name.clone(),
                            total_amount: pack.total_amount,
                        });
                    })
            })
    }

    for (_, ff_info_list) in mission_ff_map {
        for info in ff_info_list {
            if info.causer_id == player_id && info.taker_id != player_id {
                let entry = ff_cause_map.entry(info.taker_name).or_default();
                entry.damage += info.total_amount;
                entry.game_count += 1;
                entry.show = friend_player_id_set.contains(&info.taker_id);
            }

            if info.taker_id == player_id && info.causer_id != player_id {
                let entry = ff_take_map.entry(info.causer_name).or_default();
                entry.damage += info.total_amount;
                entry.game_count += 1;
                entry.show = friend_player_id_set.contains(&info.causer_id);
            }
        }
    }

    let result_ff_cause_map = ff_map_filter_watchlist(ff_cause_map);
    let result_ff_take_map = ff_map_filter_watchlist(ff_take_map);

    let total_supply_count = player_cached_mission_list
        .iter()
        .map(|item| {
            item.supply_info
                .iter()
                .filter(|(&current_player_id, _)| current_player_id == player_id)
                .map(|(_, pack)| pack.len() as i32)
                .sum::<i32>()
        })
        .sum::<i32>();

    let average_supply_count = total_supply_count as f64 / player_cached_mission_list.len() as f64;

    PlayerDamageInfo {
        damage: damage_map,
        kill: kill_map,
        ff: PlayerFriendlyFireInfo {
            cause: result_ff_cause_map,
            take: result_ff_take_map,
        },
        average_supply_count,
        valid_game_count: player_cached_mission_list.len() as i32,
    }
}

fn ff_map_filter_watchlist(origin_map: HashMap<String, FriendlyFireData>) -> HashMap<String, FriendlyFireData> {
    let mut out_map = HashMap::with_capacity(origin_map.len());

    for (taker_name, data) in origin_map {
        if data.show {
            out_map.insert(taker_name, data);
        } else {
            let entry = out_map.entry(String::new()).or_default();
            entry.damage += data.damage;
        }
    }

    out_map
}