use std::collections::HashMap;

use common::mission::{MissionDamageInfo, MissionGeneralData, MissionGeneralInfo, MissionGeneralPlayerInfo, MissionKPIComponent, MissionKPIInfo, MissionKPIInfoFull, MissionResourceInfo, MissionWeaponDamageInfo, PlayerDamageInfo, PlayerFriendlyFireInfo, PlayerResourceData};
use crate::cache::kpi::CachedGlobalKPIState;
use crate::cache::mission::{MissionCachedInfo, MissionKPICachedInfo};
use crate::db::models::*;
use common::kpi::{KPIComponent, KPIConfig};
use crate::AppState;

use crate::db::schema::*;
use crate::{APIResponse, DbPool};
use actix_web::{get, web::{self, Data, Json}, HttpRequest};
use diesel::prelude::*;
use common::{CORRECTION_ITEMS, NITRA_GAME_ID};
use crate::cache::manager::{get_db_redis_conn, CacheManager};

fn generate_mission_general_info(
    cached_mission_list: &[MissionCachedInfo],
    invalid_mission_list: &[MissionInvalid],
    mission_id: i32,
) -> Option<MissionGeneralInfo> {
    let mut mission_invalid = None;

    for invalid_mission in invalid_mission_list {
        if invalid_mission.mission_id == mission_id {
            mission_invalid = Some(invalid_mission);
            break;
        }
    }

    for mission in cached_mission_list {
        if mission.mission_info.id == mission_id {
            return Some(MissionGeneralInfo {
                mission_id,
                mission_begin_timestamp: mission.mission_info.begin_timestamp,
                mission_invalid: mission_invalid.is_some(),
                mission_invalid_reason: mission_invalid.map_or_else(
                    || "".to_string(),
                    |invalid_mission| invalid_mission.reason.clone(),
                ),
            });
        }
    }

    None
}

fn generate_mission_player_character(
    cached_mission_list: &[MissionCachedInfo],
    player_id_to_name: &HashMap<i16, String>,
    character_id_to_game_id: &HashMap<i16, String>,
    mission_id: i32,
) -> Option<HashMap<String, String>> {
    for mission in cached_mission_list {
        if mission.mission_info.id == mission_id {
            let mut result = HashMap::new();
            for player_info in &mission.player_info {
                let character_game_id = character_id_to_game_id
                    .get(&player_info.character_id)
                    .unwrap();
                let player_name = player_id_to_name.get(&player_info.player_id).unwrap();
                result.insert(player_name.clone(), character_game_id.clone());
            }
            return Some(result);
        }
    }

    None
}

fn generate_mission_general(
    cached_mission_list: &[MissionCachedInfo],
    player_id_to_name: &HashMap<i16, String>,
    character_id_to_game_id: &HashMap<i16, String>,
    mission_type_id_to_game_id: &HashMap<i16, String>,
    mission_id: i32,
) -> Option<MissionGeneralData> {
    let target_mission = cached_mission_list
        .iter()
        .find(|mission| mission.mission_info.id == mission_id)?;

    let mut mission_player_info = HashMap::with_capacity(target_mission.player_info.len());

    for player_info in &target_mission.player_info {
        let character_game_id = character_id_to_game_id
            .get(&player_info.character_id)
            .unwrap();
        let player_name = player_id_to_name.get(&player_info.player_id).unwrap();
        mission_player_info.insert(
            player_name.clone(),
            MissionGeneralPlayerInfo {
                character_game_id: character_game_id.clone(),
                player_rank: player_info.player_rank,
                character_rank: player_info.character_rank,
                character_promotion: player_info.character_promotion,
                present_time: player_info.present_time,
                revive_num: player_info.revive_num,
                death_num: player_info.death_num,
                player_escaped: player_info.player_escaped,
            },
        );
    }

    let mission_type_game_id = mission_type_id_to_game_id
        .get(&target_mission.mission_info.mission_type_id)
        .unwrap();

    let total_damage = target_mission
        .damage_info
        .values()
        .flat_map(|player_damage_data| player_damage_data.values())
        .filter(|pack| pack.taker_type != 1)
        .map(|pack| pack.total_amount)
        .sum::<f64>();

    let total_kill = target_mission
        .kill_info
        .values()
        .flat_map(|player_kill_map| player_kill_map.values())
        .map(|pack| pack.total_amount)
        .sum::<i32>();

    let total_nitra = target_mission
        .resource_info
        .values()
        .filter_map(|player_data| player_data.get(NITRA_GAME_ID))
        .copied()
        .sum::<f64>();

    let total_minerals = target_mission
        .resource_info
        .values()
        .flat_map(|player_data| player_data.values())
        .sum::<f64>();

    let total_supply_count = target_mission
        .supply_info
        .values()
        .map(|v| v.len() as i16)
        .sum::<i16>();

    Some(MissionGeneralData {
        begin_timestamp: target_mission.mission_info.begin_timestamp,
        hazard_id: target_mission.mission_info.hazard_id,
        mission_result: target_mission.mission_info.result,
        mission_time: target_mission.mission_info.mission_time,
        mission_type_id: mission_type_game_id.clone(),
        player_info: mission_player_info,
        reward_credit: target_mission.mission_info.reward_credit,
        total_damage,
        total_kill,
        total_minerals,
        total_nitra,
        total_supply_count,
    })
}

fn generate_mission_damage(
    cached_mission_list: &[MissionCachedInfo],
    player_id_to_name: &HashMap<i16, String>,
    entity_game_id_to_name: HashMap<String, String>,
    mission_id: i32,
) -> Option<MissionDamageInfo> {
    let target_mission = cached_mission_list
        .iter()
        .find(|mission| mission.mission_info.id == mission_id)?;

    // causer -> taker -> amount
    let mut ff_causer_taker_map: HashMap<&String, HashMap<&String, f64>> =
        HashMap::with_capacity(target_mission.player_info.len());
    let mut ff_taker_causer_map: HashMap<&String, HashMap<&String, f64>> =
        HashMap::with_capacity(target_mission.player_info.len());

    let mut info: HashMap<String, PlayerDamageInfo> =
        HashMap::with_capacity(target_mission.player_info.len());

    for (causer_player_id, player_damage_map) in &target_mission.damage_info {
        let causer_player_name = player_id_to_name.get(causer_player_id).unwrap();

        for (taker_game_id, pack) in player_damage_map {
            if pack.taker_type != 1 {
                continue;
            }

            if pack.taker_id == *causer_player_id {
                continue;
            }

            ff_causer_taker_map
                .entry(causer_player_name)
                .or_default()
                .insert(taker_game_id, pack.total_amount);

            ff_taker_causer_map
                .entry(taker_game_id)
                .or_default()
                .insert(causer_player_name, pack.total_amount);
        }
    }

    for player_info in &target_mission.player_info {
        let player_id = player_info.player_id;
        let player_name = player_id_to_name.get(&player_id).unwrap();

        let player_damage = target_mission
            .damage_info
            .get(&player_id)
            .iter()
            .flat_map(|x| x.iter())
            .filter(|(_, pack)| pack.taker_type != 1)
            .map(|(k, v)| (k.clone(), v.total_amount))
            .collect::<HashMap<_, _>>();

        let player_kill = target_mission
            .kill_info
            .get(&player_id)
            .iter()
            .flat_map(|x| x.iter())
            .map(|(k, v)| (k.clone(), v.total_amount))
            .collect::<HashMap<_, _>>();

        let clone_inner_kv = |x: &HashMap<&String, HashMap<&String, f64>>| {
            x.get(player_name)
                .map(|ff_map| {
                    ff_map
                        .iter()
                        .map(|(k, v)| ((*k).clone(), *v))
                        .collect()
                })
                .unwrap_or_default()
        };

        let ff_data = PlayerFriendlyFireInfo {
            cause: clone_inner_kv(&ff_causer_taker_map),
            take: clone_inner_kv(&ff_taker_causer_map),
        };

        let supply_count = target_mission
            .supply_info
            .get(&player_id)
            .map(|player_supply_list| player_supply_list.len() as i16)
            .unwrap_or(0);

        info.insert(
            player_name.clone(),
            PlayerDamageInfo {
                damage: player_damage,
                kill: player_kill,
                ff: ff_data,
                supply_count,
            },
        );
    }

    Some(MissionDamageInfo {
        info,
        entity_mapping: entity_game_id_to_name,
    })
}

fn generate_mission_weapon_damage(
    cached_mission_list: &[MissionCachedInfo],
    weapon_game_id_to_character_game_id: &HashMap<String, String>,
    weapon_game_id_to_name: &HashMap<String, String>,
    mission_id: i32,
) -> Option<HashMap<String, MissionWeaponDamageInfo>> {
    let target_mission = cached_mission_list
        .iter()
        .find(|mission| mission.mission_info.id == mission_id)?;

    let mut result = HashMap::new();

    for (weapon_game_id, weapon_pack) in &target_mission.weapon_damage_info {
        let damage = weapon_pack
            .detail
            .values()
            .filter(|pack| pack.taker_type != 1)
            .map(|pack| pack.total_amount)
            .sum::<f64>();

        let friendly_fire = weapon_pack
            .detail
            .values()
            .filter(|pack| pack.taker_type == 1)
            .map(|pack| pack.total_amount)
            .sum::<f64>();

        let character_game_id = weapon_game_id_to_character_game_id
            .get(weapon_game_id)
            .cloned()
            .unwrap_or("Unknown".into());

        let mapped_name = weapon_game_id_to_name
            .get(weapon_game_id)
            .unwrap_or(weapon_game_id)
            .clone();

        result.insert(
            weapon_game_id.clone(),
            MissionWeaponDamageInfo {
                damage,
                friendly_fire,
                character_game_id,
                mapped_name,
            },
        );
    }

    Some(result)
}

fn generate_mission_resource(
    cached_mission_list: &[MissionCachedInfo],
    player_id_to_name: &HashMap<i16, String>,
    resource_game_id_to_name: &HashMap<String, String>,
    mission_id: i32,
) -> Option<MissionResourceInfo> {
    let target_mission = cached_mission_list
        .iter()
        .find(|mission| mission.mission_info.id == mission_id)?;
    let mut resource_info_by_player = HashMap::with_capacity(target_mission.player_info.len());

    for player_info in &target_mission.player_info {
        let player_id = player_info.player_id;
        let player_name = player_id_to_name.get(&player_id).unwrap();

        let resource_data = target_mission
            .resource_info
            .get(&player_id)
            .cloned()
            .unwrap_or_default();

        let supply_data = target_mission
            .supply_info
            .get(&player_id)
            .cloned()
            .unwrap_or_default();

        resource_info_by_player.insert(
            player_name.clone(),
            PlayerResourceData {
                resource: resource_data,
                supply: supply_data,
            },
        );
    }

    Some(MissionResourceInfo {
        data: resource_info_by_player,
        resource_mapping: resource_game_id_to_name.clone(),
    })
}

pub fn generate_mission_kpi_full(
    mission_kpi_cached_info: &MissionKPICachedInfo,
    player_id_to_name: &HashMap<i16, String>,
    global_kpi_state: &CachedGlobalKPIState,
    kpi_config: &KPIConfig,
) -> Vec<MissionKPIInfoFull> {
    let mut result = Vec::with_capacity(mission_kpi_cached_info.raw_kpi_data.len());

    let mut mission_correction_factor_sum = HashMap::new();
    let mut mission_correction_factor = HashMap::new();

    for &kpi_component in CORRECTION_ITEMS {
        for character_type in mission_kpi_cached_info.player_id_to_kpi_character.values() {
            let correction_factor = global_kpi_state
                .character_correction_factor
                .get(character_type)
                .unwrap()
                .get(&kpi_component)
                .map(|x| x.correction_factor)
                .unwrap();

            *mission_correction_factor_sum
                .entry(kpi_component)
                .or_insert(0.0) += correction_factor;
        }
    }

    for &kpi_component in CORRECTION_ITEMS {
        mission_correction_factor.insert(
            kpi_component,
            mission_correction_factor_sum[&kpi_component]
                / global_kpi_state.standard_correction_sum[&kpi_component],
        );
    }

    for (player_id, raw_kpi_data) in &mission_kpi_cached_info.raw_kpi_data {
        let player_name = player_id_to_name.get(player_id).unwrap().clone();

        let kpi_character_type = mission_kpi_cached_info
            .player_id_to_kpi_character
            .get(player_id)
            .unwrap();

        let weighted_kill = raw_kpi_data
            .get(&KPIComponent::Kill)
            .unwrap()
            .weighted_value;
        let weighted_damage = raw_kpi_data
            .get(&KPIComponent::Damage)
            .unwrap()
            .weighted_value;
        let priority_damage = raw_kpi_data
            .get(&KPIComponent::Priority)
            .unwrap()
            .weighted_value;
        let revive_num = raw_kpi_data
            .get(&KPIComponent::Revive)
            .unwrap()
            .weighted_value;
        let death_num = raw_kpi_data
            .get(&KPIComponent::Death)
            .unwrap()
            .weighted_value;
        let friendly_fire = raw_kpi_data
            .get(&KPIComponent::FriendlyFire)
            .unwrap()
            .source_value;
        let nitra = raw_kpi_data
            .get(&KPIComponent::Nitra)
            .unwrap()
            .weighted_value;
        let supply_count = raw_kpi_data
            .get(&KPIComponent::Supply)
            .unwrap()
            .weighted_value;
        let weighted_resource = raw_kpi_data
            .get(&KPIComponent::Minerals)
            .unwrap()
            .weighted_value;

        let mut player_kpi_component_list = Vec::new();

        let mut player_mission_kpi_weighted_sum = 0.0;
        let mut player_mission_kpi_max_sum = 0.0;

        let mut component_name_to_component = HashMap::new();

        for (kpi_component, kpi_data) in raw_kpi_data {
            let component_name = kpi_component.to_string_zh();

            component_name_to_component.insert(component_name.clone(), kpi_component);

            let corrected_index = match mission_correction_factor.get(kpi_component) {
                Some(factor) => (kpi_data.raw_index * factor).min(1.0),
                None => kpi_data.raw_index,
            };

            let transformed_index = match global_kpi_state
                .transform_range
                .get(kpi_character_type)
                .unwrap()
                .get(kpi_component)
            {
                Some(range_info) => {
                    let mut range_index = 0;

                    for (i, transform_range) in range_info.iter().enumerate() {
                        if corrected_index > transform_range.source_range.0 {
                            range_index = i;
                        } else {
                            break;
                        }
                    }

                    let transform_range = range_info[range_index];

                    corrected_index * transform_range.transform_coefficient.0
                        + transform_range.transform_coefficient.1
                }
                None => corrected_index,
            };

            let current_weight =
                kpi_config.character_component_weight[kpi_character_type][kpi_component];

            player_kpi_component_list.push(MissionKPIComponent {
                name: component_name,
                source_value: kpi_data.source_value,
                weighted_value: kpi_data.weighted_value,
                mission_total_weighted_value: kpi_data.mission_total_weighted_value,
                raw_index: kpi_data.raw_index,
                corrected_index,
                transformed_index,
                weight: current_weight,
            });

            player_mission_kpi_weighted_sum += transformed_index * current_weight;
            player_mission_kpi_max_sum += kpi_component.max_value() * current_weight;
        }

        player_kpi_component_list.sort_unstable_by(|a, b| {
            let a_index: i16 = (**component_name_to_component.get(&a.name).unwrap()).into();
            let b_index: i16 = (**component_name_to_component.get(&b.name).unwrap()).into();

            a_index.cmp(&b_index)
        });

        result.push(MissionKPIInfoFull {
            player_name,
            kpi_character_type: kpi_character_type.to_string(),
            weighted_kill,
            weighted_damage,
            priority_damage,
            revive_num,
            death_num,
            friendly_fire,
            nitra,
            supply_count,
            weighted_resource,
            component: player_kpi_component_list,
            mission_kpi: player_mission_kpi_weighted_sum / player_mission_kpi_max_sum,
        });
    }

    result.sort_unstable_by(|a, b| a.player_name.cmp(&b.player_name));

    result
}

#[get("/{mission_id}/info")]
async fn get_general_info(
    db_pool: Data<DbPool>,
    path: web::Path<i32>,
    redis_client: Data<redis::Client>,
) -> Json<APIResponse<MissionGeneralInfo>> {
    let mission_id = path.into_inner();

    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;


        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let invalid_mission_list: Vec<_> = mission_invalid::table
            .select(MissionInvalid::as_select())
            .load(&mut db_conn).map_err(|e| format!("cannot get invalid mission list from db: {}", e))?;


        let result =
            generate_mission_general_info(&cached_mission_list, &invalid_mission_list, mission_id);

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    Json(APIResponse::from_result_option(result, "cannot get mission general info"))
}

#[get("/{mission_id}/basic")]
async fn get_player_character(
    db_pool: Data<DbPool>,
    path: web::Path<i32>,
    redis_client: Data<redis::Client>,
) -> Json<APIResponse<HashMap<String, String>>> {
    let mission_id = path.into_inner();

    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;


        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let player_list = player::table.select(Player::as_select()).load(&mut db_conn).map_err(|e| format!("cannot get player list: {}", e))?;

        let player_id_to_name = player_list
            .into_iter()
            .map(|player| (player.id, player.player_name))
            .collect::<HashMap<_, _>>();

        let character_list = character::table
            .select(Character::as_select())
            .load(&mut db_conn).map_err(|e| format!("cannot get character list: {}", e))?;


        let character_id_to_game_id = character_list
            .into_iter()
            .map(|character| (character.id, character.character_game_id))
            .collect::<HashMap<_, _>>();

        let result = generate_mission_player_character(
            &cached_mission_list,
            &player_id_to_name,
            &character_id_to_game_id,
            mission_id,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    Json(APIResponse::from_result_option(result, "cannot get player character info"))
}

#[get("/{mission_id}/general")]
async fn get_mission_general(
    db_pool: Data<DbPool>,
    path: web::Path<i32>,
    redis_client: Data<redis::Client>,
) -> Json<APIResponse<MissionGeneralData>> {
    let mission_id = path.into_inner();

    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;


        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let player_list = player::table.select(Player::as_select()).load(&mut db_conn).map_err(|e| format!("cannot get player list: {}", e))?;

        let player_id_to_name = player_list
            .into_iter()
            .map(|player| (player.id, player.player_name))
            .collect::<HashMap<_, _>>();

        let character_list = character::table
            .select(Character::as_select())
            .load(&mut db_conn).map_err(|e| format!("cannot get character list: {}", e))?;


        let character_id_to_game_id = character_list
            .into_iter()
            .map(|character| (character.id, character.character_game_id))
            .collect::<HashMap<_, _>>();

        let mission_type_list = mission_type::table
            .select(MissionType::as_select())
            .load(&mut db_conn).map_err(|e| format!("cannot get mission type list: {}", e))?;

        let mission_type_id_to_game_id = mission_type_list
            .into_iter()
            .map(|mission_type| (mission_type.id, mission_type.mission_type_game_id))
            .collect::<HashMap<_, _>>();

        let result = generate_mission_general(
            &cached_mission_list,
            &player_id_to_name,
            &character_id_to_game_id,
            &mission_type_id_to_game_id,
            mission_id,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    Json(APIResponse::from_result_option(result, "cannot get mission general info"))
}

#[get("/{mission_id}/damage")]
async fn get_mission_damage(
    db_pool: Data<DbPool>,
    path: web::Path<i32>,
    redis_client: Data<redis::Client>,
    cache_manager: Data<CacheManager>,
) -> Json<APIResponse<MissionDamageInfo>> {
    let mission_id = path.into_inner();

    let entity_game_id_to_name = cache_manager.get_mapping().entity_mapping;

    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;


        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let player_list = player::table.select(Player::as_select()).load(&mut db_conn)
            .map_err(|e| format!("cannot get player list: {}", e))?;

        let player_id_to_name = player_list
            .into_iter()
            .map(|player| (player.id, player.player_name))
            .collect::<HashMap<_, _>>();

        let result = generate_mission_damage(
            &cached_mission_list,
            &player_id_to_name,
            entity_game_id_to_name,
            mission_id,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    Json(APIResponse::from_result_option(result, "cannot get mission damage info"))
}

#[get("/{mission_id}/weapon")]
async fn get_mission_weapon_damage(
    db_pool: Data<DbPool>,
    path: web::Path<i32>,
    redis_client: Data<redis::Client>,
    cache_manager: Data<CacheManager>,
) -> Json<APIResponse<HashMap<String, MissionWeaponDamageInfo>>> {
    let mission_id = path.into_inner();
    let mapping = cache_manager.get_mapping();

    let weapon_game_id_to_name = mapping.weapon_mapping;
    let weapon_game_id_to_character_game_id = mapping.weapon_character;

    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;


        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let result = generate_mission_weapon_damage(
            &cached_mission_list,
            &weapon_game_id_to_character_game_id,
            &weapon_game_id_to_name,
            mission_id,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    Json(APIResponse::from_result_option(result, "cannot get mission weapon damage info"))
}

#[get("/{mission_id}/resource")]
async fn get_mission_resource_info(
    db_pool: Data<DbPool>,
    path: web::Path<i32>,
    redis_client: Data<redis::Client>,
    cache_manager: Data<CacheManager>,
) -> Json<APIResponse<MissionResourceInfo>> {
    let mission_id = path.into_inner();

    let resource_game_id_to_name = cache_manager.get_mapping().resource_mapping;

    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;


        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let player_list = player::table
            .select(Player::as_select())
            .load(&mut db_conn)
            .map_err(|e| format!("cannot get player list: {}", e))?;

        let player_id_to_name = player_list
            .into_iter()
            .map(|player| (player.id, player.player_name))
            .collect::<HashMap<_, _>>();

        let result = generate_mission_resource(
            &cached_mission_list,
            &player_id_to_name,
            &resource_game_id_to_name,
            mission_id,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    Json(APIResponse::from_result_option(result, "cannot get mission resource info"))
}

#[get("/{mission_id}/kpi_full")]
async fn get_mission_kpi_full(
    db_pool: Data<DbPool>,
    path: web::Path<i32>,
    redis_client: Data<redis::Client>,
    app_state: Data<AppState>,
    cache_manager: Data<CacheManager>,
    request: HttpRequest,
) -> Json<APIResponse<Vec<MissionKPIInfoFull>>> {
    if !app_state.check_access_token(&request) {
        return Json(APIResponse::unauthorized());
    }

    let mission_id = path.into_inner();

    if let Some(kpi_config) = cache_manager.get_kpi_config() {
        let result = get_mission_kpi_base(db_pool, redis_client, kpi_config, mission_id).await;

        Json(APIResponse::from_result_option(result, "cannot get mission kpi info"))
    } else {
        Json(APIResponse::config_required("kpi"))
    }
}

#[get("/{mission_id}/kpi")]
async fn get_mission_kpi(
    db_pool: Data<DbPool>,
    path: web::Path<i32>,
    redis_client: Data<redis::Client>,
    cache_manager: Data<CacheManager>,
) -> Json<APIResponse<Vec<MissionKPIInfo>>> {
    let mission_id = path.into_inner();

    if let Some(kpi_config) = cache_manager.get_kpi_config() {
        let result = get_mission_kpi_base(db_pool, redis_client, kpi_config, mission_id)
            .await
            .map(|r|
                r.map(|x|
                    x
                        .into_iter()
                        .map(|item| item.into())
                        .collect::<Vec<_>>()));


        Json(APIResponse::from_result_option(result, "cannot get mission kpi info"))
    } else {
        Json(APIResponse::config_required("kpi"))
    }
}

async fn get_mission_kpi_base(db_pool: Data<DbPool>,
                              redis_client: Data<redis::Client>,
                              kpi_config: KPIConfig,
                              mission_id: i32, ) -> Result<Option<Vec<MissionKPIInfoFull>>, String> {
    web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;


        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let mut found = false;

        for mission in &cached_mission_list {
            if mission.mission_info.id == mission_id {
                found = true;
                break;
            }
        }

        if !found {
            return Ok(None);
        }

        let player_list = player::table
            .select(Player::as_select())
            .load(&mut db_conn)
            .map_err(|e| format!("cannot get player list: {}", e))?;

        let player_id_to_name = player_list
            .into_iter()
            .map(|player| (player.id, player.player_name))
            .collect::<HashMap<_, _>>();

        let global_kpi_state = CachedGlobalKPIState::try_get_cached(&mut redis_conn)
            .map_err(|e| format!("cannot get global kpi state: {}", e))?;

        let mission_kpi_cached_info = MissionKPICachedInfo::try_get_cached(&mut redis_conn, mission_id)
            .map_err(|e| format!("cannot get mission kpi cached info: {}", e))?;

        let result = generate_mission_kpi_full(
            &mission_kpi_cached_info,
            &player_id_to_name,
            &global_kpi_state,
            &kpi_config,
        );


        Ok::<_, String>(Some(result))
    })
        .await
        .unwrap()
}