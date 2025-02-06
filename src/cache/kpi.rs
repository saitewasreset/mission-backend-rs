use crate::cache::mission::{cache_write_redis, CacheTimeInfo, MissionCachedInfo, MissionKPICachedInfo};
use crate::kpi::*;
use crate::{CORRECTION_ITEMS, KPI_CALCULATION_PLAYER_INDEX, NITRA_GAME_ID, TRANSFORM_KPI_COMPONENTS};
use diesel::{PgConnection, QueryDsl, RunQueryDsl, SelectableHelper};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use crate::cache::manager::{get_db_redis_conn, get_from_redis, CacheContext, CacheError, CacheGenerationError, Cacheable};
use crate::db::models::{Character, MissionInvalid, Player};
use crate::db::schema::{character, mission_invalid};

// depends on:
// - MissionCachedInfo
// - MissionKPICachedInfo
// - kpi_config
#[derive(Serialize, Deserialize)]
pub struct CachedGlobalKPIState {
    pub character_correction_factor:
        HashMap<CharacterKPIType, HashMap<KPIComponent, CorrectionFactorInfo>>,
    pub standard_correction_sum: HashMap<KPIComponent, f64>,
    pub transform_range: HashMap<CharacterKPIType, HashMap<KPIComponent, Vec<IndexTransformRange>>>,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct CorrectionFactorInfo {
    pub player_index: f64,
    pub value: f64,
    pub correction_factor: f64,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct CharacterMissionInfo {
    pub player_index: f64,
    pub damage: f64,
    pub priority: f64,
    pub kill: f64,
    pub nitra: f64,
    pub resource: f64,
}

impl CachedGlobalKPIState {
    pub fn generate(
        cached_mission_list: &[MissionCachedInfo],
        cached_mission_kpi_list: &[MissionKPICachedInfo],
        invalid_mission_id_list: &[i32],
        kpi_config: &KPIConfig,
        player_id_to_name: &HashMap<i16, String>,
        character_id_to_game_id: &HashMap<i16, String>,
        scout_special_player_set: &HashSet<String>,
    ) -> (Self, CacheTimeInfo) {
        let begin = Instant::now();

        let cached_mission_kpi_set = cached_mission_kpi_list
            .iter()
            .map(|item| (item.mission_id, item))
            .collect::<HashMap<_, _>>();

        let invalid_mission_id_set: HashSet<i32> =
            invalid_mission_id_list.iter().copied().collect();

        let cached_mission_list = cached_mission_list
            .iter()
            .filter(|x| !invalid_mission_id_set.contains(&x.mission_info.id))
            .collect::<Vec<_>>();

        if cached_mission_list.is_empty() {
            return (
                CachedGlobalKPIState {
                    character_correction_factor: HashMap::new(),
                    standard_correction_sum: HashMap::new(),
                    transform_range: HashMap::new(),
                },
                CacheTimeInfo::from_duration_generate(begin.elapsed()),
            );
        }

        let mut character_to_mission_info_list: HashMap<
            CharacterKPIType,
            Vec<CharacterMissionInfo>,
        > = HashMap::new();

        let mut character_correction_factor = HashMap::new();

        for mission in &cached_mission_list {
            for player_info in &mission.player_info {
                let player_index = *mission.player_index.get(&player_info.player_id).unwrap();

                let player_name = player_id_to_name.get(&player_info.player_id).unwrap();
                let player_character_game_id = character_id_to_game_id
                    .get(&player_info.character_id)
                    .unwrap();

                let player_character_kpi_type = CharacterKPIType::from_player(
                    player_character_game_id,
                    player_name,
                    scout_special_player_set,
                );

                let player_kill = mission
                    .kill_info
                    .get(&player_info.player_id)
                    .iter()
                    .flat_map(|player_info| player_info.values())
                    .map(|pack| pack.total_amount as f64)
                    .sum::<f64>();

                let player_damage_map = mission
                    .damage_info
                    .get(&player_info.player_id)
                    .iter()
                    .flat_map(|player_info| player_info.iter())
                    .filter(|(_, pack)| pack.taker_type != 1)
                    .map(|(taker_game_id, pack)| (taker_game_id.clone(), pack.total_amount))
                    .collect::<HashMap<_, _>>();

                let player_priority_map =
                    apply_weight_table(&player_damage_map, &kpi_config.priority_table);

                let player_priority_damage = player_priority_map.values().sum::<f64>();

                let player_damage = player_damage_map.values().sum::<f64>();

                let player_nitra = mission
                    .resource_info
                    .get(&player_info.player_id)
                    .iter()
                    .flat_map(|player_info| player_info.iter())
                    .filter(|(resource_game_id, _)| *resource_game_id == NITRA_GAME_ID)
                    .map(|(_, total_amount)| *total_amount)
                    .sum::<f64>();

                let player_resource = mission
                    .resource_info
                    .get(&player_info.player_id)
                    .iter()
                    .flat_map(|player_info| player_info.iter())
                    .map(|(_, total_amount)| *total_amount)
                    .sum::<f64>();

                character_to_mission_info_list
                    .entry(player_character_kpi_type)
                    .or_default()
                    .push(CharacterMissionInfo {
                        player_index,
                        damage: player_damage,
                        priority: player_priority_damage,
                        kill: player_kill,
                        nitra: player_nitra,
                        resource: player_resource,
                    });
            }
        }

        for (&character_kpi_type, mission_info_list) in &character_to_mission_info_list {
            let player_index = mission_info_list
                .iter()
                .map(|x| x.player_index)
                .sum::<f64>();

            let average_damage =
                mission_info_list.iter().map(|x| x.damage).sum::<f64>() / player_index;
            let average_priority_damage =
                mission_info_list.iter().map(|x| x.priority).sum::<f64>() / player_index;
            let average_kill = mission_info_list.iter().map(|x| x.kill).sum::<f64>() / player_index;
            let average_nitra =
                mission_info_list.iter().map(|x| x.nitra).sum::<f64>() / player_index;
            let average_resource =
                mission_info_list.iter().map(|x| x.resource).sum::<f64>() / player_index;

            let mut correction_info = HashMap::new();

            correction_info.insert(
                KPIComponent::Damage,
                CorrectionFactorInfo {
                    player_index,
                    value: average_damage,
                    correction_factor: 0.0,
                },
            );

            correction_info.insert(
                KPIComponent::Priority,
                CorrectionFactorInfo {
                    player_index,
                    value: average_priority_damage,
                    correction_factor: 0.0,
                },
            );

            correction_info.insert(
                KPIComponent::Kill,
                CorrectionFactorInfo {
                    player_index,
                    value: average_kill,
                    correction_factor: 0.0,
                },
            );

            correction_info.insert(
                KPIComponent::Nitra,
                CorrectionFactorInfo {
                    player_index,
                    value: average_nitra,
                    correction_factor: 0.0,
                },
            );

            correction_info.insert(
                KPIComponent::Minerals,
                CorrectionFactorInfo {
                    player_index,
                    value: average_resource,
                    correction_factor: 0.0,
                },
            );

            character_correction_factor.insert(character_kpi_type, correction_info);
        }

        let min_damage = character_correction_factor
            .values()
            .map(|x| x.get(&KPIComponent::Damage).unwrap().value)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or_default();

        let min_priority = character_correction_factor
            .values()
            .map(|x| x.get(&KPIComponent::Priority).unwrap().value)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or_default();

        let min_kill = character_correction_factor
            .values()
            .map(|x| x.get(&KPIComponent::Kill).unwrap().value)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or_default();

        let min_nitra = character_correction_factor
            .values()
            .map(|x| x.get(&KPIComponent::Nitra).unwrap().value)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or_default();

        let min_minerals = character_correction_factor
            .values()
            .map(|x| x.get(&KPIComponent::Minerals).unwrap().value)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or_default();

        for correction_info in character_correction_factor.values_mut() {
            let damage = correction_info.get_mut(&KPIComponent::Damage).unwrap();
            damage.correction_factor = damage.value / min_damage;

            let priority = correction_info.get_mut(&KPIComponent::Priority).unwrap();
            priority.correction_factor = priority.value / min_priority;

            let kill = correction_info.get_mut(&KPIComponent::Kill).unwrap();
            kill.correction_factor = kill.value / min_kill;

            let nitra = correction_info.get_mut(&KPIComponent::Nitra).unwrap();
            nitra.correction_factor = nitra.value / min_nitra;

            let minerals = correction_info.get_mut(&KPIComponent::Minerals).unwrap();
            minerals.correction_factor = minerals.value / min_minerals;
        }

        let standard_character = [
            CharacterKPIType::Driller,
            CharacterKPIType::Engineer,
            CharacterKPIType::Gunner,
            CharacterKPIType::Scout,
        ];

        let mut standard_correction_sum = HashMap::new();

        for item in CORRECTION_ITEMS {
            let item_sum = standard_character
                .iter()
                .map(|character| {
                    character_correction_factor
                        .get(character)
                        .unwrap()
                        .get(item)
                        .unwrap()
                        .correction_factor
                })
                .sum::<f64>();

            standard_correction_sum.insert(*item, item_sum);
        }

        // Vec<(f64, f64) -> (player_index, corrected_index)
        type MissionKPIIndexInfoByPlayer = HashMap<i16, HashMap<KPIComponent, Vec<(f64, f64)>>>;
        let mut character_kpi_type_to_player_id_to_mission_index_list: HashMap<
            CharacterKPIType,
            MissionKPIIndexInfoByPlayer,
        > = HashMap::new();

        for mission in &cached_mission_list {
            let mut mission_correction_sum: HashMap<KPIComponent, f64> = HashMap::new();
            for player_info in &mission.player_info {
                let player_character_id = player_info.character_id;

                let player_character_kpi_type = CharacterKPIType::from_player(
                    character_id_to_game_id.get(&player_character_id).unwrap(),
                    player_id_to_name.get(&player_info.player_id).unwrap(),
                    scout_special_player_set,
                );

                let correction_data = character_correction_factor
                    .get(&player_character_kpi_type)
                    .unwrap();

                for (&kpi_component, info) in correction_data {
                    *mission_correction_sum.entry(kpi_component).or_insert(0.0) +=
                        info.correction_factor;
                }
            }

            for player_info in &mission.player_info {
                let player_index = *mission.player_index.get(&player_info.player_id).unwrap();
                let player_character_id = player_info.character_id;

                let player_character_kpi_type = CharacterKPIType::from_player(
                    character_id_to_game_id.get(&player_character_id).unwrap(),
                    player_id_to_name.get(&player_info.player_id).unwrap(),
                    scout_special_player_set,
                );
                let player_raw_kpi_data = cached_mission_kpi_set
                    .get(&mission.mission_info.id)
                    .unwrap()
                    .raw_kpi_data
                    .get(&player_info.player_id)
                    .unwrap();

                for kpi_component in CORRECTION_ITEMS {
                    let raw_data = player_raw_kpi_data.get(kpi_component).unwrap();
                    let corrected_index = raw_data.raw_index
                        * mission_correction_sum.get(kpi_component).unwrap()
                        / standard_correction_sum.get(kpi_component).unwrap();

                    if player_index < KPI_CALCULATION_PLAYER_INDEX {
                        continue;
                    }

                    character_kpi_type_to_player_id_to_mission_index_list
                        .entry(player_character_kpi_type)
                        .or_default()
                        .entry(player_info.player_id)
                        .or_default()
                        .entry(*kpi_component)
                        .or_default()
                        .push((player_index, corrected_index));
                }
            }
        }

        let mut source_distribution: HashMap<CharacterKPIType, HashMap<KPIComponent, Vec<f64>>> =
            HashMap::new();

        for (character_kpi_type, player_map) in
            &character_kpi_type_to_player_id_to_mission_index_list
        {
            for player_data in player_map.values() {
                for (&kpi_component, index_list) in player_data {
                    let player_index_sum = index_list
                        .iter()
                        .map(|(player_index, _)| player_index)
                        .sum::<f64>();

                    let player_index_weighted_sum = index_list
                        .iter()
                        .map(|(player_index, corrected_index)| player_index * corrected_index)
                        .sum::<f64>();

                    source_distribution
                        .entry(*character_kpi_type)
                        .or_default()
                        .entry(kpi_component)
                        .or_default()
                        .push(player_index_weighted_sum / player_index_sum);
                }
            }
        }

        source_distribution.iter_mut().for_each(|(_, data)| {
            data.iter_mut().for_each(|(_, index_list)| {
                index_list.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
            });
        });

        let config_transform_range = &kpi_config.transform_range;

        let mut transform_range: HashMap<
            CharacterKPIType,
            HashMap<KPIComponent, Vec<IndexTransformRange>>,
        > = HashMap::new();

        for (&character_kpi_type, data) in &source_distribution {
            for &kpi_component in TRANSFORM_KPI_COMPONENTS {
                let index_list = data.get(&kpi_component).unwrap();
                for range_config in config_transform_range {
                    let source_list_begin_index =
                        (index_list.len() as f64 * range_config.rank_range.0) as usize;

                    let source_list_end_index =
                        (index_list.len() as f64 * range_config.rank_range.1) as usize;

                    let source_min = match source_list_begin_index {
                        0 => 0.0,
                        _ => index_list[source_list_begin_index],
                    };
                    let source_max = match source_list_end_index {
                        x if x < index_list.len() => index_list[x],
                        _ => 1.00,
                    };

                    let transform_min = range_config.transform_range.0;
                    let transform_max = range_config.transform_range.1;

                    let (a, b) = match source_max - source_min {
                        0.0 => ((transform_max + transform_min) / (2.0 * source_min), 0.0),
                        _ => {
                            let a = (transform_max - transform_min) / (source_max - source_min);
                            let b = transform_min - a * source_min;
                            (a, b)
                        }
                    };

                    let result = IndexTransformRange {
                        rank_range: range_config.rank_range,
                        source_range: (source_min, source_max),
                        transform_range: range_config.transform_range,
                        transform_coefficient: (a, b),
                        player_count: (source_list_end_index - source_list_begin_index) as i32,
                    };

                    transform_range
                        .entry(character_kpi_type)
                        .or_default()
                        .entry(kpi_component)
                        .or_default()
                        .push(result);
                }
            }
        }

        let result = CachedGlobalKPIState {
            character_correction_factor,
            standard_correction_sum,
            transform_range,
        };

        let elapsed = begin.elapsed();

        (result, CacheTimeInfo::from_duration_generate(elapsed))
    }

    pub fn from_redis_all(
        db_conn: &mut PgConnection,
        redis_conn: &mut redis::Connection,
        invalid_mission_id_list: &[i32],
        kpi_config: &KPIConfig,
        player_id_to_name: &HashMap<i16, String>,
        character_id_to_game_id: &HashMap<i16, String>,
        scout_special_player_set: &HashSet<String>,
    ) -> Result<(Self, CacheTimeInfo), CacheError> {
        let begin = Instant::now();
        let cached_mission_list = MissionCachedInfo::try_get_cached_all(
            db_conn,
            redis_conn,
        )?;

        let cached_mission_kpi_list = MissionKPICachedInfo::try_get_cached_all(
            db_conn,
            redis_conn,
        )?;

        let load_from_redis_elapsed = begin.elapsed();
        let begin = Instant::now();

        let generated = Self::generate(
            &cached_mission_list,
            &cached_mission_kpi_list,
            invalid_mission_id_list,
            kpi_config,
            player_id_to_name,
            character_id_to_game_id,
            scout_special_player_set,
        )
            .0;

        let generate_elapsed = begin.elapsed();

        Ok((generated, CacheTimeInfo {
            count: 1,
            load_from_db: Some(load_from_redis_elapsed),
            generate: generate_elapsed,
        }))
    }

    pub fn try_get_cached(redis_conn: &mut redis::Connection) -> Result<Self, CacheError> {
        let redis_key = "global_kpi_state";

        get_from_redis(redis_conn, redis_key)
    }
}

impl Cacheable for CachedGlobalKPIState {
    fn name(&self) -> &str {
        "global_kpi_state"
    }

    fn generate_and_write(context: &CacheContext) -> Result<CacheTimeInfo, CacheGenerationError> {
        let begin = Instant::now();

        let scout_special_player_set = &context.mapping.scout_special_player_set;

        let kpi_config = context.kpi_config.as_ref().ok_or_else(|| {
            CacheGenerationError::ConfigError("kpi config is not set".to_string())
        })?;

        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&context.db_pool, &context.redis_client)?;

        let character_list = character::table
            .select(Character::as_select())
            .load(&mut db_conn).map_err(|e| {
            CacheGenerationError::InternalError(format!("cannot get character list from db: {}", e))
        })?;

        let character_id_to_game_id = character_list
            .into_iter()
            .map(|character| (character.id, character.character_game_id))
            .collect::<HashMap<_, _>>();

        let player_list = crate::db::schema::player::table
            .select(Player::as_select())
            .load(&mut db_conn).map_err(|e| {
            CacheGenerationError::InternalError(format!("cannot get player list from db: {}", e))
        })?;

        let player_id_to_name = player_list
            .into_iter()
            .map(|player| (player.id, player.player_name))
            .collect::<HashMap<_, _>>();

        let invalid_mission_list = mission_invalid::table
            .select(MissionInvalid::as_select())
            .load(&mut db_conn).map_err(|e| {
            CacheGenerationError::InternalError(format!("cannot get invalid mission list from db: {}", e))
        })?;


        let invalid_mission_id_list = invalid_mission_list
            .into_iter()
            .map(|x| x.mission_id)
            .collect::<Vec<_>>();

        let load_from_db_elapsed = begin.elapsed();

        let (cache_result, mut time_info) = CachedGlobalKPIState::from_redis_all(
            &mut db_conn,
            &mut redis_conn,
            &invalid_mission_id_list,
            kpi_config,
            &player_id_to_name,
            &character_id_to_game_id,
            scout_special_player_set,
        ).map_err(|e| {
            CacheGenerationError::InternalError(format!("cannot generate global kpi state: {}", e))
        })?;

        cache_write_redis(&cache_result, "global_kpi_state", &mut redis_conn).map_err(CacheGenerationError::InternalError)?;

        let _ = redis::cmd("SAVE").exec(&mut redis_conn);

        time_info.add_load_from_db(load_from_db_elapsed);

        Ok(time_info)
    }
}