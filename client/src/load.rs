use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::Display;
use encoding_rs::{DecoderResult, UTF_16LE, UTF_8};
use regex::Regex;
use std::io::Write;
use std::num::ParseFloatError;
use std::path::Path;
use std::path::PathBuf;
use std::time;
use serde::Deserialize;
use common::kpi::{CharacterKPIType, IndexTransformRangeConfig, KPIComponent, KPIConfig};
use common::Mapping;
use common::mission_log::{LogContent, LogDamageInfo, LogKillInfo, LogMissionInfo, LogPlayerInfo, LogResourceInfo, LogSupplyInfo};
use crate::format_size;

#[derive(Debug)]
pub enum LoadError {
    IOError(std::io::Error),
    ParseError(String),
}

impl Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::IOError(e) => write!(f, "IOError: {}", e),
            LoadError::ParseError(e) => write!(f, "ParseError: {}", e),
        }
    }
}

impl Error for LoadError {}

const MAX_LOG_LENGTH: usize = 64 * 1024 * 1024;

pub fn compress(data: &[u8]) -> Vec<u8> {
    println!("Serialized len = {}", format_size(data.len()));

    let compressed = Vec::with_capacity(data.len());

    let start = time::Instant::now();

    let mut encoder = zstd::Encoder::new(compressed, 15).unwrap();

    encoder.write_all(data).unwrap();
    let mut compressed = encoder.finish().unwrap();

    let finish = time::Instant::now();

    println!(
        "Compressed using zstd, compressed len = {} with level 15, time: {:?}",
        format_size(compressed.len()),
        finish.duration_since(start)
    );

    compressed.shrink_to_fit();
    compressed
}

#[derive(Debug, Deserialize)]
struct EntityListEntry {
    pub entity_game_id: String,
    #[serde(rename = "mapped_name")]
    pub _mapped_name: String,
    pub priority: f64,
    pub driller: f64,
    pub gunner: f64,
    pub engineer: f64,
    pub scout: f64,
    pub scout_special: f64,
}

fn get_log_file_list(base_path: impl AsRef<Path>) -> Result<Vec<PathBuf>, std::io::Error> {
    let re = Regex::new("MissionMonitor_([0-9]+).txt").unwrap();
    let r = std::fs::read_dir(base_path)?
        .filter(|r| {
            re.is_match(
                r.as_ref()
                    .unwrap()
                    .file_name()
                    .as_os_str()
                    .to_str()
                    .unwrap(),
            )
        })
        .map(|r| r.unwrap().path())
        .collect();

    Ok(r)
}

fn process_log_segment<'a, E, T>(log_segment_str: &'a str, segment_name: &str) -> Result<Vec<T>, String>
where
    E: Display,
    T: TryFrom<&'a str, Error=E>,
{
    let mut result: Vec<T> = Vec::new();

    for player_info_line in log_segment_str.lines() {
        if player_info_line.trim().is_empty() {
            continue;
        }
        result.push(
            player_info_line
                .try_into()
                .map_err(|e| format!("load {}: {}", segment_name, e))?,
        );
    }

    Ok(result)
}

fn combine_range_damage(range_begin_idx: usize, range_end_idx: usize, damage_info: &[LogDamageInfo]) -> LogDamageInfo {
    let range_begin_item = &damage_info[range_begin_idx];
    let damage_sum = damage_info[range_begin_idx..range_end_idx]
        .iter()
        .map(|item| item.damage)
        .sum::<f64>();

    LogDamageInfo {
        mission_time: range_begin_item.mission_time,
        damage: damage_sum,
        taker: range_begin_item.taker.clone(),
        causer: range_begin_item.causer.clone(),
        weapon: range_begin_item.weapon.clone(),
        causer_type: range_begin_item.causer_type,
        taker_type: range_begin_item.taker_type,
    }
}

fn get_file_content_parted(file_path: impl AsRef<Path>) -> Result<LogContent, Box<dyn Error>> {
    let raw_file_content = std::fs::read(file_path.as_ref())?;

    let mut file_content = String::with_capacity(MAX_LOG_LENGTH);

    if raw_file_content[0] == 0xFF && raw_file_content[1] == 0xFE {
        // UTF-16-LE
        let mut decoder = UTF_16LE.new_decoder();

        let (result, _) = decoder.decode_to_string_without_replacement(
            &raw_file_content,
            &mut file_content,
            false,
        );
        if let DecoderResult::Malformed(_, _) = result {
            panic!(
                "Cannot decode input: {} with UTF-16-LE",
                file_path.as_ref().file_name().unwrap().to_str().unwrap()
            );
        }
    } else {
        let mut decoder = UTF_8.new_decoder();
        let (result, _) = decoder.decode_to_string_without_replacement(
            &raw_file_content,
            &mut file_content,
            true,
        );
        if let DecoderResult::Malformed(_, _) = result {
            panic!(
                "Cannot decode input: {} with UTF-8",
                file_path.as_ref().file_name().unwrap().to_str().unwrap()
            );
        }
    }

    file_content.shrink_to_fit();

    let file_part_list = file_content.split("______").collect::<Vec<&str>>();

    let mission_info = LogMissionInfo::try_from(file_content.as_str())
        .map_err(|e| format!("load mission info: {}", e))?;

    let player_info_part = file_part_list[1];

    let mut player_info: Vec<LogPlayerInfo> = process_log_segment(player_info_part, "player info")?;

    let damage_info_part = file_part_list[2];

    let mut damage_info: Vec<LogDamageInfo> = process_log_segment(damage_info_part, "damage info")?;

    let mut range_begin_idx: usize = 0;

    let mut combined_damage_info: Vec<LogDamageInfo> = Vec::with_capacity(damage_info.len());

    if damage_info.len() > 0 {
        for (i, current_damage_info) in damage_info.iter().enumerate() {
            if !current_damage_info.combine_eq(&damage_info[range_begin_idx]) {
                combined_damage_info.push(combine_range_damage(range_begin_idx, i, &damage_info));

                range_begin_idx = i;
            }
        }

        combined_damage_info.push(combine_range_damage(range_begin_idx, damage_info.len(), &damage_info));
    }



    let kill_info_part = file_part_list[3];

    let mut kill_info: Vec<LogKillInfo> = Vec::new();

    for kill_info_line in kill_info_part.lines() {
        if kill_info_line.trim().is_empty() {
            continue;
        }
        kill_info.push(
            kill_info_line
                .try_into()
                .map_err(|e| format!("load kill info: {}", e))?,
        );
    }

    let resource_info_part = file_part_list[4];

    let mut resource_info: Vec<LogResourceInfo> = Vec::new();

    for resource_info_line in resource_info_part.lines() {
        if resource_info_line.trim().is_empty() {
            continue;
        }
        resource_info.push(
            resource_info_line
                .try_into()
                .map_err(|e| format!("load resource info: {}", e))?,
        );
    }

    let supply_info_part = file_part_list[5];
    let mut supply_info: Vec<LogSupplyInfo> = Vec::new();

    for supply_info_line in supply_info_part.lines() {
        if supply_info_line.trim().is_empty() {
            continue;
        }
        supply_info.push(
            supply_info_line
                .try_into()
                .map_err(|e| format!("load supply info: {}", e))?,
        );
    }

    let mission_time = mission_info.mission_time;

    // Fix total present time

    for current_player_info in &mut player_info {
        if current_player_info.total_present_time == 0 {
            current_player_info.total_present_time = mission_time;
        }
    }

    let first_player_join_time = player_info
        .iter()
        .map(|player| player.join_mission_time)
        .min()
        .ok_or(String::from("player count is 0"))?;

    // Fix time for damage info, killed info, resource info, supply info

    for current_damage_info in &mut damage_info {
        current_damage_info.mission_time -= first_player_join_time;
    }

    for current_kill_info in &mut kill_info {
        current_kill_info.mission_time -= first_player_join_time;
    }

    for current_resource_info in &mut resource_info {
        current_resource_info.mission_time -= first_player_join_time;
    }

    for current_supply_info in &mut supply_info {
        current_supply_info.mission_time -= first_player_join_time;
    }

    Ok(LogContent {
        mission_info,
        player_info,
        damage_info: combined_damage_info,
        kill_info,
        resource_info,
        supply_info,
    })

    // Identify Deep Dive in get_mission_list
}

pub fn parse_mission_log(base_path: impl AsRef<Path>) -> Result<Vec<LogContent>, LoadError> {
    let file_path_list = get_log_file_list(base_path).map_err(LoadError::IOError)?;

    let mut parsed_mission_list = Vec::new();
    for file_path in file_path_list {
        parsed_mission_list.push(get_file_content_parted(&file_path).map_err(|e| {
            format!(
                "cannot parse log: {}: {}",
                &file_path.as_os_str().to_str().unwrap(),
                e
            )
        }).map_err(LoadError::ParseError)?);
    }

    parsed_mission_list.sort_unstable_by(|a, b| {
        a.mission_info
            .begin_timestamp
            .cmp(&b.mission_info.begin_timestamp)
    });

    let mut deep_dive_mission_list = Vec::new();

    for mission in &parsed_mission_list {
        let first_player_join_time = mission
            .player_info
            .iter()
            .map(|p| p.join_mission_time)
            .min()
            .unwrap();

        if first_player_join_time > 0 {
            deep_dive_mission_list.push(mission.mission_info.begin_timestamp);
        }
    }

    for i in 0..parsed_mission_list.len() {
        let current_mission = &parsed_mission_list[i];

        let prev_mission = match i {
            0 => None,
            x => Some(&parsed_mission_list[x - 1]),
        };

        // 对于深潜，第一层对应的first_player_join_time为0，而二、三层不为0
        // 对于普通深潜，每一层的难度都显示为0.75（3）
        if deep_dive_mission_list
            .binary_search(&current_mission.mission_info.begin_timestamp)
            .is_ok()
        {
            // 若当前任务first_player_join_time不为0，但前一任务为0，说明当前是第二层，前一任务是第一层
            // 若当前任务first_player_join_time不为0，前一任务也不为0，说明当前是第三层，前一任务是第二层
            // 注：除非在第一层手动放弃任务，否则不论第二层是否胜利，都会有第二层的数据
            // 若在第一层手动放弃任务，则第一层无法识别为深潜
            if let Some(prev_mission) = prev_mission {
                match deep_dive_mission_list
                    .binary_search(&prev_mission.mission_info.begin_timestamp)
                {
                    Ok(_) => {
                        // 前一层是第二层，当前是第三层
                        if prev_mission.mission_info.hazard_id.get() == 3
                            || prev_mission.mission_info.hazard_id.get() == 101
                        {
                            // 普通深潜
                            prev_mission.mission_info.hazard_id.set(101);
                            current_mission.mission_info.hazard_id.set(102);
                        } else {
                            // 精英深潜
                            prev_mission.mission_info.hazard_id.set(104);
                            current_mission.mission_info.hazard_id.set(105);
                        }
                    }
                    Err(_) => {
                        // 前一层是第一层，当前是第二层
                        if prev_mission.mission_info.hazard_id.get() == 3
                            || prev_mission.mission_info.hazard_id.get() == 100
                        {
                            // 普通深潜
                            prev_mission.mission_info.hazard_id.set(100);
                            current_mission.mission_info.hazard_id.set(101);
                        } else {
                            // 精英深潜
                            prev_mission.mission_info.hazard_id.set(103);
                            current_mission.mission_info.hazard_id.set(104);
                        }
                    }
                }
            }
        }
    }

    Ok(parsed_mission_list)
}

pub fn parse_config_file_list(file_path: impl AsRef<Path>) -> Result<Vec<String>, LoadError> {
    let raw_file_content = std::fs::read(file_path.as_ref()).map_err(LoadError::IOError)?;

    let file_content = String::from_utf8(raw_file_content)
        .map_err(|e| LoadError::ParseError(format!("{}: {}", file_path.as_ref().to_string_lossy(), e)))?;

    let valid_lines = file_content.lines()
        .map(|raw_line| raw_line.trim())
        .filter(|line| !line.starts_with('#'))
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect::<Vec<_>>();

    Ok(valid_lines)
}

pub fn parse_config_file_map(file_path: impl AsRef<Path>) -> Result<HashMap<String, String>, LoadError> {
    let file_path_str = file_path.as_ref().to_string_lossy().to_string();

    let valid_lines = parse_config_file_list(file_path)?;

    let mut config_map = HashMap::new();

    for line in valid_lines {
        let split = line
            .splitn(2, '|')
            .map(|item| item.trim())
            .collect::<Vec<_>>();

        if split.len() != 2 {
            return Err(LoadError::ParseError(format!("{}: invalid line: {}", file_path_str, line)));
        }

        config_map.insert(split[0].to_string(), split[1].to_string());
    }

    Ok(config_map)
}

pub fn load_mapping_from_file(mapping_directory: impl AsRef<Path>) -> Result<Mapping, LoadError> {
    let character_mapping =
        parse_config_file_map(mapping_directory.as_ref().join("character.txt"))?;

    let entity_mapping =
        parse_config_file_map(mapping_directory.as_ref().join("entity.txt"))?;

    let entity_blacklist =
        parse_config_file_list(mapping_directory.as_ref().join("entity_blacklist.txt"))?;

    let entity_combine =
        parse_config_file_map(mapping_directory.as_ref().join("entity_combine.txt"))?;

    let mission_type_mapping =
        parse_config_file_map(mapping_directory.as_ref().join("mission_type.txt"))?;

    let resource_mapping =
        parse_config_file_map(mapping_directory.as_ref().join("resource.txt"))?;

    let weapon_mapping =
        parse_config_file_map(mapping_directory.as_ref().join("weapon.txt"))?;

    let weapon_combine =
        parse_config_file_map(mapping_directory.as_ref().join("weapon_combine.txt"))?;

    let weapon_character =
        parse_config_file_map(mapping_directory.as_ref().join("weapon_hero.txt"))?;

    let scout_special_player =
        parse_config_file_list(mapping_directory.as_ref().join("scout_special.txt"))?;

    Ok(Mapping {
        character_mapping,
        entity_mapping,
        entity_blacklist_set: HashSet::from_iter(entity_blacklist),
        entity_combine,
        mission_type_mapping,
        resource_mapping,
        weapon_mapping,
        weapon_combine,
        weapon_character,
        scout_special_player_set: HashSet::from_iter(scout_special_player),
    })
}

fn kpi_transform_range_parse_line(line: &str) -> Result<Vec<f64>, ParseFloatError> {
    let mut result = Vec::new();

    for item_str in line.split(' ') {
        let item = item_str.parse::<f64>()?;
        result.push(item);
    }

    Ok(result)
}

fn kpi_load_transform_range(file_path: impl AsRef<Path>) -> Result<Vec<IndexTransformRangeConfig>, LoadError> {
    let mut result = Vec::new();

    let file_path_str = file_path.as_ref().to_string_lossy().to_string();

    let lines = parse_config_file_list(file_path)?;

    if lines.len() != 2 {
        return Err(LoadError::ParseError(format!("{}: invalid line count", file_path_str)));
    }

    let source_list = kpi_transform_range_parse_line(&lines[0])
        .map_err(|e| LoadError::ParseError(format!("{}: {}", file_path_str, e)))?;

    let transformed_list = kpi_transform_range_parse_line(&lines[1])
        .map_err(|e| LoadError::ParseError(format!("{}: {}", file_path_str, e)))?;

    if source_list.len() != transformed_list.len() {
        return Err(LoadError::ParseError(format!("count mismatch: source: {}, transformed: {}",
                                                 source_list.len(),
                                                 transformed_list.len())));
    }

    if source_list.is_empty() {
        return Err(LoadError::ParseError(format!("{}: empty source list", file_path_str)));
    }

    let count = source_list.len();

    for i in 0..count - 1 {
        result.push(IndexTransformRangeConfig {
            rank_range: (source_list[i], source_list[i + 1]),
            transform_range: (transformed_list[i], transformed_list[i + 1]),
        })
    }

    Ok(result)
}

fn kpi_load_character_component_weight(file_path: impl AsRef<Path>) -> Result<HashMap<CharacterKPIType, HashMap<KPIComponent, f64>>, LoadError> {
    let file_path_str = file_path.as_ref().to_string_lossy().to_string();

    let lines = parse_config_file_list(file_path)?;

    let mut result = HashMap::new();

    for line in lines {
        let split_line = line.split(' ').collect::<Vec<_>>();

        let character_type_id: i16 = split_line[0]
            .parse()
            .map_err(|e| LoadError::ParseError(format!("{}: {}", file_path_str, e)))?;

        let mut character_weight_list = Vec::new();

        let mut character_weight_map = HashMap::new();

        for weight_str in split_line.iter().skip(1) {
            let weight = weight_str
                .parse::<f64>()
                .map_err(|e| LoadError::ParseError(format!("{}: {}", file_path_str, e)))?;
            character_weight_list.push(weight);
        }

        if character_weight_list.len() != 9 {
            return Err(LoadError::ParseError(format!("{}: invalid weight count: {}", file_path_str, line)));
        }

        for (i, weight) in character_weight_list.iter().enumerate() {
            if *weight < 0.0 || *weight > 1.0 {
                return Err(LoadError::ParseError(format!("{}: invalid weight: {}", file_path_str, *weight)));
            }

            character_weight_map.insert(KPIComponent::try_from(i).unwrap(), *weight);
        }

        result.insert(CharacterKPIType::try_from(character_type_id).map_err(LoadError::ParseError)?, character_weight_map);
    }

    Ok(result)
}

fn kpi_load_resource_weight_table(file_path: impl AsRef<Path>) -> Result<HashMap<String, f64>, LoadError> {
    let file_path_str = file_path.as_ref().to_string_lossy().to_string();

    let mut result = HashMap::new();

    let mut reader = csv::Reader::from_path(file_path).map_err(|e| LoadError::ParseError(e.to_string()))?;

    for row in reader.records().skip(1) {
        let record_list = row.map_err(|e| LoadError::ParseError(format!("{}: {}", file_path_str, e)))?;

        if record_list.len() != 3 {
            return Err(LoadError::ParseError(format!("{}: invalid column count in row: {:?}", file_path_str, record_list)));
        }

        let resource_game_id = record_list[0].to_string();
        let resource_weight = record_list[2]
            .parse::<f64>()
            .map_err(|e| LoadError::ParseError(format!("{}: {}", file_path_str, e)))?;

        result.insert(resource_game_id, resource_weight);
    }

    Ok(result)
}

type CharacterWeightTable = HashMap<CharacterKPIType, HashMap<String, f64>>;
type PriorityTable = HashMap<String, f64>;

fn kpi_load_entity_weight_table(file_path: impl AsRef<Path>) -> Result<(CharacterWeightTable, PriorityTable), LoadError> {
    let file_path_str = file_path.as_ref().to_string_lossy().to_string();

    let mut character_weight_table = HashMap::new();
    let mut priority_table = HashMap::new();

    let mut reader = csv::Reader::from_path(file_path).map_err(|e| LoadError::ParseError(e.to_string()))?;

    for result in reader.deserialize() {
        let record: EntityListEntry = result.map_err(|e| LoadError::ParseError(format!("{}: {}", file_path_str, e)))?;

        character_weight_table
            .entry(CharacterKPIType::Driller)
            .or_insert_with(HashMap::new)
            .insert(record.entity_game_id.clone(), record.driller);

        character_weight_table
            .entry(CharacterKPIType::Gunner)
            .or_insert_with(HashMap::new)
            .insert(record.entity_game_id.clone(), record.gunner);

        character_weight_table
            .entry(CharacterKPIType::Engineer)
            .or_insert_with(HashMap::new)
            .insert(record.entity_game_id.clone(), record.engineer);

        character_weight_table
            .entry(CharacterKPIType::Scout)
            .or_insert_with(HashMap::new)
            .insert(record.entity_game_id.clone(), record.scout);

        character_weight_table
            .entry(CharacterKPIType::ScoutSpecial)
            .or_insert_with(HashMap::new)
            .insert(record.entity_game_id.clone(), record.scout_special);

        priority_table.insert(record.entity_game_id, record.priority);
    }

    Ok((character_weight_table, priority_table))
}

pub fn load_kpi_config_from_file(kpi_config_directory: impl AsRef<Path>) -> Result<KPIConfig, LoadError> {
    let character_component_weight =
        kpi_load_character_component_weight(kpi_config_directory.as_ref().join("character_component_weight.txt"))?;

    let resource_weight_table =
        kpi_load_resource_weight_table(kpi_config_directory.as_ref().join("resource_table.csv"))?;

    let (character_weight_table, priority_table) =
        kpi_load_entity_weight_table(kpi_config_directory.as_ref().join("entity_list_combined.csv"))?;

    let transform_range =
        kpi_load_transform_range(kpi_config_directory.as_ref().join("transform_range.txt"))?;

    Ok(KPIConfig {
        character_weight_table,
        priority_table,
        resource_weight_table,
        character_component_weight,
        transform_range,
    })
}