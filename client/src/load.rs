use std::error::Error;
use std::fmt::Display;
use encoding_rs::{DecoderResult, UTF_16LE, UTF_8};
use regex::Regex;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time;
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

    for (i, current_damage_info) in damage_info.iter().enumerate() {
        if !current_damage_info.combine_eq(&damage_info[range_begin_idx]) {
            combined_damage_info.push(combine_range_damage(range_begin_idx, i, &damage_info));

            range_begin_idx = i;
        }
    }

    combined_damage_info.push(combine_range_damage(range_begin_idx, damage_info.len(), &damage_info));

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
