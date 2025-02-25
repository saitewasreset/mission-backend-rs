use std::collections::{HashMap, HashSet};
use common::info::{PlayerInfo, APIBrothers, OverallInfo};
use crate::cache::mission::MissionCachedInfo;
use common::RE_SPOT_TIME_THRESHOLD;
use crate::{APIResponse, DbPool};
use actix_web::{
    get,
    web::{self, Data, Json},
};

use crate::db::models::*;
use crate::db::schema::*;
use diesel::prelude::*;
use log::error;
use crate::cache::manager::get_db_redis_conn;


fn generate(
    cached_mission_list: &[MissionCachedInfo],
    player_id_to_name: &HashMap<i16, String>,
    watchlist_player_id_list: &[i16],
) -> APIBrothers {
    let watchlist_player_id_set = watchlist_player_id_list
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let mut player_map = HashMap::new();

    for mission in cached_mission_list {
        for player_info in &mission.player_info {
            if watchlist_player_id_set.contains(&player_info.player_id) {
                continue;
            }
            let player_entry = player_map
                .entry(player_info.player_id)
                .or_insert(PlayerInfo {
                    game_count: 0,
                    last_spot: 0,
                    presence_time: 0,
                    spot_count: 0,
                    timestamp_list: Vec::new(),
                });

            player_entry.game_count += 1;
            if mission.mission_info.begin_timestamp > player_entry.last_spot {
                player_entry.last_spot = mission.mission_info.begin_timestamp;
            }

            player_entry.presence_time += player_info.present_time as i32;

            player_entry
                .timestamp_list
                .push(mission.mission_info.begin_timestamp);
        }
    }

    for (_, player_info) in player_map.iter_mut() {
        player_info.timestamp_list.sort_unstable();
        let mut last_timestamp = player_info.timestamp_list[0];
        for &timestamp in &player_info.timestamp_list {
            if timestamp - last_timestamp > RE_SPOT_TIME_THRESHOLD {
                player_info.spot_count += 1;
            }
            last_timestamp = timestamp;
        }
    }

    let player_count = player_map.len() as i32;
    let total_spot_count = player_map.values().map(|x| x.spot_count).sum::<i32>();

    let player_average_spot = total_spot_count as f64 / player_map.len() as f64;

    let player_ge_two_count = player_map.values().filter(|x| x.game_count >= 2).count();
    let player_ge_two_percent = player_ge_two_count as f64 / player_count as f64;

    let player_spot_count = player_map.values().filter(|x| x.spot_count >= 1).count();
    let player_spot_percent = player_spot_count as f64 / player_count as f64;

    APIBrothers {
        overall: OverallInfo {
            player_average_spot,
            unfamiliar_player_count: player_count,
            player_ge_two_percent,
            player_spot_percent,
        },
        player: player_map
            .into_iter()
            .map(|(player_id, player_info)| {
                (
                    player_id_to_name.get(&player_id).unwrap().clone(),
                    player_info,
                )
            })
            .collect(),
    }
}

#[get("/brothers")]
async fn get_brothers_info(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
) -> Json<APIResponse<APIBrothers>> {
    if let Ok((mut db_conn, mut redis_conn)) = get_db_redis_conn(&db_pool, &redis_client) {
        let result = web::block(move || {
            let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
                .map_err(|e| e.to_string())?;

            let player_list = player::table
                .select(Player::as_select())
                .load(&mut db_conn)
                .map_err(|e| e.to_string())?;

            let player_id_to_name = player_list
                .into_iter()
                .map(|player| (player.id, player.player_name))
                .collect::<HashMap<_, _>>();

            let watchlist_player_id_list: Vec<i16> = player::table
                .select(player::id)
                .filter(player::friend.eq(true))
                .load::<i16>(&mut db_conn).map_err(|e| e.to_string())?;

            Ok::<_, String>(generate(
                &cached_mission_list,
                &player_id_to_name,
                &watchlist_player_id_list,
            ))
        }).await.unwrap();

        Json(APIResponse::from_result(result, "cannot get brothers info"))
    } else {
        error!("cannot get db connection");
        Json(APIResponse::internal_error())
    }
}
