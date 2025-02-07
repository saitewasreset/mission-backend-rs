use super::{DeltaData, GeneralInfo};
use crate::cache::mission::MissionCachedInfo;
use crate::db::schema::*;
use crate::hazard_id_to_real;
use crate::{APIResponse, DbPool};
use actix_web::{
    get,
    web::{self, Data, Json},
};
use diesel::prelude::*;
use std::collections::HashSet;
use crate::cache::manager::get_db_redis_conn;

#[get("/")]
async fn get_general(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
) -> Json<APIResponse<GeneralInfo>> {
    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;

        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let invalid_mission_id_list: Vec<i32> = mission_invalid::table
            .select(mission_invalid::mission_id)
            .load(&mut db_conn).map_err(|e| format!("cannot get invalid mission list from db: {}", e))?;

        let watchlist_player_id_list: Vec<i16> = player::table
            .select(player::id)
            .filter(player::friend.eq(true))
            .load(&mut db_conn).map_err(|e| format!("cannot get watchlist from db: {}", e))?;


        let result = generate(
            &cached_mission_list,
            &invalid_mission_id_list,
            &watchlist_player_id_list,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    Json(APIResponse::from_result(result, "cannot get general info"))
}

fn generate(
    cached_mission_list: &[MissionCachedInfo],
    invalid_mission_id_list: &[i32],
    watchlist_player_id_list: &[i16],
) -> GeneralInfo {
    let game_count = cached_mission_list.len() as i32;

    let invalid_mission_id_set = invalid_mission_id_list
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let cached_mission_list = cached_mission_list
        .iter()
        .filter(|item| !invalid_mission_id_set.contains(&item.mission_info.id))
        .collect::<Vec<_>>();

    let valid_game_count = cached_mission_list.len();

    let valid_rate = valid_game_count as f64 / game_count as f64;

    let total_total_mission_time = cached_mission_list
        .iter()
        .map(|item| item.mission_info.mission_time as i64)
        .sum::<i64>();
    let prev_count = match valid_game_count * 8 / 10 {
        0..10 => 10,
        x => x,
    };

    let prev_count = if prev_count >= valid_game_count {
        valid_game_count
    } else {
        prev_count
    };

    let average_mission_time = DeltaData::from_slice(
        &cached_mission_list,
        prev_count,
        |iter| {
            let len = iter.len();

            if len == 0 {
                0
            } else {
                (iter.map(|item| item.mission_info.mission_time as i64)
                    .sum::<i64>()
                    / len as i64) as i16
            }
        },
    );

    let unique_player_id_set = cached_mission_list
        .iter()
        .flat_map(|item| {
            item.player_info
                .iter()
                .map(|player_info| player_info.player_id)
        })
        .collect::<HashSet<_>>();

    let unique_player_count = unique_player_id_set.len() as i32;

    let watchlist_player_id_set = watchlist_player_id_list
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let open_room_rate = DeltaData::from_slice(
        &cached_mission_list,
        prev_count,
        |iter| {
            let len = iter.len();

            if len == 0 {
                0.0
            } else {
                iter.filter(|item| {
                    for player_info in &item.player_info {
                        if !watchlist_player_id_set.contains(&player_info.player_id) {
                            return true;
                        }
                    }
                    false
                })
                    .count() as f64
                    / len as f64
            }
        },
    );

    let pass_rate = DeltaData::from_slice(
        &cached_mission_list,
        prev_count,
        |iter| {
            let len = iter.len();

            if len == 0 {
                0.0
            } else {
                iter.filter(|item| item.mission_info.result == 0)
                    .count() as f64
                    / len as f64
            }
        },
    );

    let average_difficulty = DeltaData::from_slice(
        &cached_mission_list,
        prev_count,
        |iter| {
            let len = iter.len();

            if len == 0 {
                0.0
            } else {
                iter.map(|item| hazard_id_to_real(item.mission_info.hazard_id))
                    .sum::<f64>()
                    / len as f64
            }
        },
    );

    let average_kill_num = DeltaData::from_slice(
        &cached_mission_list,
        prev_count,
        |iter| {
            let len = iter.len();

            if len == 0 {
                0
            } else {
                (iter.map(|item| {
                    item.kill_info
                        .values()
                        .map(|player_data| {
                            player_data
                                .values()
                                .map(|pack| pack.total_amount)
                                .sum::<i32>()
                        })
                        .sum::<i32>()
                })
                    .sum::<i32>() as f64
                    / len as f64) as i16
            }
        },
    );

    let average_damage = DeltaData::from_slice(
        &cached_mission_list,
        prev_count,
        |iter| {
            let len = iter.len();

            if len == 0 {
                0.0
            } else {
                iter.map(|item| {
                    item.damage_info
                        .values()
                        .map(|player_data| {
                            player_data
                                .values()
                                .map(|pack| pack.total_amount)
                                .sum::<f64>()
                        })
                        .sum::<f64>()
                })
                    .sum::<f64>()
                    / len as f64
            }
        },
    );

    let average_death_num_per_player = DeltaData::from_slice(
        &cached_mission_list,
        prev_count,
        |iter| {
            let len = iter.len();

            if len == 0 {
                0.0
            } else {
                iter.map(|item| &item.player_info)
                    .map(|player_info_list| {
                        player_info_list
                            .iter()
                            .map(|player_info| player_info.death_num as f64)
                            .sum::<f64>()
                            / player_info_list.len() as f64
                    })
                    .sum::<f64>()
                    / len as f64
            }
        },
    );


    let average_minerals_mined = DeltaData::from_slice(
        &cached_mission_list,
        prev_count,
        |iter| {
            let len = iter.len();

            if len == 0 {
                0.0
            } else {
                iter.map(|item| {
                    item.resource_info
                        .values()
                        .map(|player_resource_info| player_resource_info.values().sum::<f64>())
                        .sum::<f64>()
                })
                    .sum::<f64>()
                    / len as f64
            }
        },
    );

    let average_supply_count_per_player = DeltaData::from_slice(
        &cached_mission_list,
        prev_count,
        |iter| {
            let len = iter.len();

            if len == 0 {
                0.0
            } else {
                iter.map(|item| {
                    item.supply_info
                        .values()
                        .map(|player_supply_list| player_supply_list.len() as f64)
                        .sum::<f64>()
                        / item.player_info.len() as f64
                })
                    .sum::<f64>()
                    / len as f64
            }
        },
    );

    let average_reward_credit = DeltaData::from_slice(
        &cached_mission_list,
        prev_count,
        |iter| {
            let len = iter.len();

            if len == 0 {
                0.0
            } else {
                iter.map(|item| item.mission_info.reward_credit)
                    .sum::<f64>()
                    / len as f64
            }
        },
    );

    GeneralInfo {
        game_count,
        valid_rate,
        total_mission_time: total_total_mission_time,
        average_mission_time,
        unique_player_count,
        open_room_rate,
        pass_rate,
        average_difficulty,
        average_kill_num,
        average_damage,
        average_death_num_per_player,
        average_minerals_mined,
        average_supply_count_per_player,
        average_reward_credit,
    }
}
