use super::player::generate_player_kpi;
use crate::cache::kpi::CachedGlobalKPIState;
use crate::cache::mission::{MissionCachedInfo, MissionKPICachedInfo};
use crate::db::models::*;
use crate::db::schema::*;
use crate::{APIResponse, DbPool};
use crate::{KPIConfig, FLOAT_EPSILON};
use actix_web::{
    get,
    web::{self, Data, Json},
};
use diesel::prelude::*;
use log::error;
use serde::Serialize;
use std::collections::HashMap;
use crate::cache::manager::{get_db_redis_conn, CacheError, CacheManager};

#[derive(Serialize)]
pub struct PlayerBotKPIInfo {
    #[serde(rename = "deltaPercent")]
    pub delta_percent: f64,
    pub overall: f64,
    pub recent: f64,
}

fn generate_bot_kpi_info(
    cached_mission_list: &[MissionCachedInfo],
    mission_kpi_cached_info_list: &[MissionKPICachedInfo],
    invalid_mission_id_list: &[i32],
    watchlist_player_id_list: &[i16],
    player_id_to_name: &HashMap<i16, String>,
    global_kpi_state: &CachedGlobalKPIState,
    kpi_config: &KPIConfig,
) -> HashMap<String, PlayerBotKPIInfo> {
    let player_kpi_info = generate_player_kpi(
        cached_mission_list,
        mission_kpi_cached_info_list,
        invalid_mission_id_list,
        watchlist_player_id_list,
        player_id_to_name,
        global_kpi_state,
        kpi_config,
    );

    let mut result = HashMap::with_capacity(player_kpi_info.len());

    for (player_game_id, player_info) in player_kpi_info {
        let mut player_mission_info_list = player_info
            .by_character
            .values()
            .flat_map(|player_character_info| player_character_info.mission_list.clone())
            .collect::<Vec<_>>();

        player_mission_info_list.sort_unstable_by(|a, b| a.begin_timestamp.cmp(&b.begin_timestamp));

        let prev_mission_count = match player_mission_info_list.len() * 8 / 10 {
            0..10 => 10,
            x => x,
        };

        let prev_mission_count = if prev_mission_count >= player_mission_info_list.len() {
            player_mission_info_list.len()
        } else {
            prev_mission_count
        };

        let prev_list = &player_mission_info_list[0..prev_mission_count];
        let recent_list = &player_mission_info_list[prev_mission_count..];

        let prev_player_index = prev_list.iter().map(|item| item.player_index).sum::<f64>();
        let prev_weighted_sum = prev_list
            .iter()
            .map(|item| item.mission_kpi * item.player_index)
            .sum::<f64>();

        let prev_player_kpi = prev_weighted_sum / prev_player_index;

        let overall_player_index = player_mission_info_list
            .iter()
            .map(|item| item.player_index)
            .sum::<f64>();
        let overall_weighted_sum = player_mission_info_list
            .iter()
            .map(|item| item.mission_kpi * item.player_index)
            .sum::<f64>();

        let overall_player_kpi = overall_weighted_sum / overall_player_index;

        let recent_player_index = recent_list
            .iter()
            .map(|item| item.player_index)
            .sum::<f64>();
        let recent_weighted_sum = recent_list
            .iter()
            .map(|item| item.mission_kpi * item.player_index)
            .sum::<f64>();
        let recent_player_kpi = match recent_player_index.abs() {
            0.0..FLOAT_EPSILON => overall_player_kpi,
            _ => recent_weighted_sum / recent_player_index,
        };

        let delta_percent = (recent_player_kpi - prev_player_kpi) / prev_player_kpi;

        result.insert(
            player_game_id,
            PlayerBotKPIInfo {
                delta_percent,
                overall: overall_player_kpi,
                recent: recent_player_kpi,
            },
        );
    }

    result
}

#[get("/bot_kpi_info")]
async fn get_bot_kpi_info(
    cache_manager: Data<CacheManager>,
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
) -> Json<APIResponse<HashMap<String, PlayerBotKPIInfo>>> {
    match get_db_redis_conn(&db_pool, &redis_client) {
        Ok((mut db_conn, mut redis_conn)) => {
            if let Some(kpi_config) = cache_manager.get_kpi_config() {
                let result: Result<_, CacheError> = web::block(move || {
                    let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)?;

                    let mission_kpi_cached_info_list = MissionKPICachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)?;

                    let global_kpi_state = CachedGlobalKPIState::try_get_cached(&mut redis_conn)?;

                    let invalid_mission_id_list = mission_invalid::table
                        .select(mission_invalid::mission_id)
                        .load::<i32>(&mut db_conn).map_err(|e| CacheError::InternalError(e.to_string()))?;

                    let player_list = player::table
                        .select(Player::as_select())
                        .load::<_>(&mut db_conn).map_err(|e| CacheError::InternalError(e.to_string()))?;

                    let watchlist_player_id_list = player_list
                        .iter()
                        .filter(|player| player.friend)
                        .map(|player| player.id)
                        .collect::<Vec<_>>();

                    let player_id_to_name = player_list
                        .into_iter()
                        .map(|player| (player.id, player.player_name))
                        .collect();

                    let result = generate_bot_kpi_info(
                        &cached_mission_list,
                        &mission_kpi_cached_info_list,
                        &invalid_mission_id_list,
                        &watchlist_player_id_list,
                        &player_id_to_name,
                        &global_kpi_state,
                        &kpi_config,
                    );

                    Ok(result)
                })
                    .await
                    .unwrap();

                Json(APIResponse::from_result(result, "cannot get bot kpi info"))
            } else {
                Json(APIResponse::config_required("kpi"))
            }
        }
        Err(e) => {
            error!("cannot get db connection: {}", e);
            Json(APIResponse::internal_error())
        }
    }
}
