use crate::cache::kpi::*;
use crate::cache::mission::*;
use crate::db::models::*;
use crate::db::schema::*;
use common::kpi::{KPIConfig, PlayerCharacterKPIInfo, PlayerKPIInfo, PlayerMissionKPIInfo};
use crate::mission::mission_info::generate_mission_kpi_full;
use common::mission::MissionKPIInfoFull;
use crate::{APIResponse, AppState, DbPool};
use actix_web::{get, web::{self, Data, Json}, HttpRequest};
use diesel::prelude::*;
use std::collections::{HashMap, HashSet};
use crate::cache::manager::{get_db_redis_conn, CacheManager};


pub fn generate_player_kpi(
    cached_mission_list: &[MissionCachedInfo],
    mission_kpi_cached_info_list: &[MissionKPICachedInfo],
    invalid_mission_id_list: &[i32],
    watchlist_player_id_list: &[i16],
    player_id_to_name: &HashMap<i16, String>,
    global_kpi_state: &CachedGlobalKPIState,
    kpi_config: &KPIConfig,
) -> HashMap<String, PlayerKPIInfo> {
    let player_name_to_id = player_id_to_name
        .iter()
        .map(|(id, name)| (name, *id))
        .collect::<HashMap<_, _>>();

    let watchlist_player_name_set = watchlist_player_id_list
        .iter()
        .map(|id| player_id_to_name.get(id).unwrap())
        .collect::<HashSet<_>>();

    let invalid_mission_id_set = invalid_mission_id_list
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let mission_kpi_cached_info_list = mission_kpi_cached_info_list
        .iter()
        .filter(|item| !invalid_mission_id_set.contains(&item.mission_id))
        .collect::<Vec<_>>();

    let mission_id_to_cached_info = cached_mission_list
        .iter()
        .map(|mission_info| (mission_info.mission_info.id, mission_info))
        .collect::<HashMap<_, _>>();

    let mission_kpi_by_mission_id = mission_kpi_cached_info_list
        .iter()
        .map(|mission_kpi_info| {
            (
                mission_kpi_info.mission_id,
                (
                    mission_kpi_info.mission_id,
                    generate_mission_kpi_full(
                        mission_kpi_info,
                        player_id_to_name,
                        global_kpi_state,
                        kpi_config,
                    ),
                ),
            )
        })
        .collect::<HashMap<_, _>>();

    // character -> [(mission_id, MissionKPIInfoFull)]
    type PlayerMissionKPIInfoFullListByCharacter<'a> = HashMap<&'a String, Vec<(i32, &'a MissionKPIInfoFull)>>;

    let mut player_name_to_character_type_to_mission_list: HashMap<
        &String,
        PlayerMissionKPIInfoFullListByCharacter,
    > = HashMap::new();

    for (mission_id, mission_kpi_info_list) in mission_kpi_by_mission_id.values() {
        for mission_kpi_info in mission_kpi_info_list {
            let player_name = &mission_kpi_info.player_name;
            let character_type = &mission_kpi_info.kpi_character_type;

            player_name_to_character_type_to_mission_list
                .entry(player_name)
                .or_default()
                .entry(character_type)
                .or_default()
                .push((*mission_id, mission_kpi_info));
        }
    }

    let mut result = HashMap::new();

    for (player_name, character_type_to_mission_list) in
        player_name_to_character_type_to_mission_list
    {
        if !watchlist_player_name_set.contains(player_name) {
            continue;
        }
        let mut total_player_player_index = 0.0;
        let mut player_kpi_weighted_sum = 0.0;

        let mut by_character = HashMap::new();
        for (character_type, mission_list) in character_type_to_mission_list {
            let mut total_character_player_index = 0.0;
            let mut mission_kpi_weighted_sum = 0.0;

            let mut result_mission_list = Vec::new();

            for (mission_id, mission_kpi_info) in mission_list {
                let mission_info = *mission_id_to_cached_info.get(&mission_id).unwrap();
                let player_index = *mission_info
                    .player_index
                    .get(player_name_to_id.get(player_name).unwrap())
                    .unwrap();
                result_mission_list.push(PlayerMissionKPIInfo {
                    mission_id,
                    begin_timestamp: mission_info.mission_info.begin_timestamp,
                    player_index,
                    mission_kpi: mission_kpi_info.mission_kpi,
                });

                total_character_player_index += player_index;
                mission_kpi_weighted_sum += player_index * mission_kpi_info.mission_kpi;

                total_player_player_index += player_index;
                player_kpi_weighted_sum += player_index * mission_kpi_info.mission_kpi;
            }

            let player_character_kpi_info = PlayerCharacterKPIInfo {
                player_index: total_character_player_index,
                character_kpi: mission_kpi_weighted_sum / total_character_player_index,
                character_kpi_type: character_type.to_string(),
                mission_list: result_mission_list,
            };

            by_character.insert(character_type.to_string(), player_character_kpi_info);
        }

        let player_kpi_info = PlayerKPIInfo {
            player_index: total_player_player_index,
            player_kpi: player_kpi_weighted_sum / total_player_player_index,
            by_character,
        };

        result.insert(player_name.clone(), player_kpi_info);
    }

    result
}

#[get("/player_kpi")]
async fn get_player_kpi(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
    cache_manager: Data<CacheManager>,
    app_state: Data<AppState>,
    request: HttpRequest,
) -> Json<APIResponse<HashMap<String, PlayerKPIInfo>>> {
    if !app_state.check_session(&request) {
        return Json(APIResponse::unauthorized());
    }

    if let Some(kpi_config) = cache_manager.get_kpi_config() {
        let result = web::block(move || {
            let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
                .map_err(|e| format!("cannot get connection: {}", e))?;

            let player_list = player::table
                .select(Player::as_select())
                .load(&mut db_conn)
                .map_err(|e| format!("cannot get player list: {}", e))?;

            let watchlist_player_id_list = player_list
                .iter()
                .filter(|item| item.friend)
                .map(|item| item.id)
                .collect::<Vec<_>>();

            let player_id_to_name = player_list
                .into_iter()
                .map(|player| (player.id, player.player_name))
                .collect::<HashMap<_, _>>();

            let invalid_mission_id_list = mission_invalid::table
                .select(mission_invalid::mission_id)
                .load::<i32>(&mut db_conn)
                .map_err(|e| format!("cannot get invalid mission id list: {}", e))?;

            let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
                .map_err(|e| format!("cannot get cached mission info: {}", e))?;

            let mission_kpi_cached_info_list = MissionKPICachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
                .map_err(|e| format!("cannot get cached mission kpi info: {}", e))?;

            let global_kpi_state = CachedGlobalKPIState::try_get_cached(&mut redis_conn)
                .map_err(|e| format!("cannot get cached global kpi state: {}", e))?;

            let result = generate_player_kpi(
                &cached_mission_list,
                &mission_kpi_cached_info_list,
                &invalid_mission_id_list,
                &watchlist_player_id_list,
                &player_id_to_name,
                &global_kpi_state,
                &kpi_config,
            );

            Ok::<_, String>(result)
        })
            .await
            .unwrap();

        Json(APIResponse::from_result(result, "cannot get player kpi"))
    } else {
        Json(APIResponse::config_required("kpi"))
    }
}
