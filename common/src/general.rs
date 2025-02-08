use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct DeltaData<T: Serialize> {
    prev: T,
    recent: T,
    total: T,
}

impl<T: Serialize> DeltaData<T> {
    pub fn new(prev: T, recent: T, total: T) -> Self {
        DeltaData {
            prev,
            recent,
            total,
        }
    }

    pub fn from_slice<S, I, F>(slice: S, prev_count: usize, f: F) -> Self
    where
        S: AsRef<[I]>,
        F: Fn(std::slice::Iter<I>) -> T,
    {
        let prev_part = &slice.as_ref()[0..prev_count];
        let recent_part = &slice.as_ref()[prev_count..];

        let prev_value = f(prev_part.iter());
        let recent_value = f(recent_part.iter());
        let total_value = f(slice.as_ref().iter());

        DeltaData::new(prev_value, recent_value, total_value)
    }
}

#[derive(Serialize, Deserialize)]
pub struct GeneralInfo {
    #[serde(rename = "gameCount")]
    pub game_count: i32,
    #[serde(rename = "validRate")]
    pub valid_rate: f64,
    #[serde(rename = "totalMissionTime")]
    pub total_mission_time: i64,
    #[serde(rename = "averageMissionTime")]
    pub average_mission_time: DeltaData<i16>,
    #[serde(rename = "uniquePlayerCount")]
    pub unique_player_count: i32,
    #[serde(rename = "openRoomRate")]
    pub open_room_rate: DeltaData<f64>,
    #[serde(rename = "passRate")]
    pub pass_rate: DeltaData<f64>,
    #[serde(rename = "averageDifficulty")]
    pub average_difficulty: DeltaData<f64>,
    #[serde(rename = "averageKillNum")]
    pub average_kill_num: DeltaData<i16>,
    #[serde(rename = "averageDamage")]
    pub average_damage: DeltaData<f64>,
    #[serde(rename = "averageDeathNumPerPlayer")]
    pub average_death_num_per_player: DeltaData<f64>,
    #[serde(rename = "averageMineralsMined")]
    pub average_minerals_mined: DeltaData<f64>,
    #[serde(rename = "averageSupplyCountPerPlayer")]
    pub average_supply_count_per_player: DeltaData<f64>,
    #[serde(rename = "averageRewardCredit")]
    pub average_reward_credit: DeltaData<f64>,
}

#[derive(Serialize, Deserialize)]
pub struct MissionTypeData {
    #[serde(rename = "averageDifficulty")]
    pub average_difficulty: f64,
    #[serde(rename = "averageMissionTime")]
    pub average_mission_time: f64,
    #[serde(rename = "averageRewardCredit")]
    pub average_reward_credit: f64,
    #[serde(rename = "creditPerMinute")]
    pub credit_per_minute: f64,
    #[serde(rename = "missionCount")]
    pub mission_count: i32,
    #[serde(rename = "passRate")]
    pub pass_rate: f64,
}

#[derive(Serialize)]
pub struct MissionTypeInfo {
    #[serde(rename = "missionTypeMap")]
    pub mission_type_map: HashMap<String, String>,
    // mission_game_id -> MissionTypeData
    #[serde(rename = "missionTypeData")]
    pub mission_type_data: HashMap<String, MissionTypeData>,
}

#[derive(Serialize)]
pub struct PlayerData {
    #[serde(rename = "averageDeathNum")]
    pub average_death_num: f64,
    #[serde(rename = "averageMineralsMined")]
    pub average_minerals_mined: f64,
    #[serde(rename = "averageReviveNum")]
    pub average_revive_num: f64,
    #[serde(rename = "averageSupplyCount")]
    pub average_supply_count: f64,
    #[serde(rename = "averageSupplyEfficiency")]
    pub average_supply_efficiency: f64,
    #[serde(rename = "characterInfo")]
    pub character_info: HashMap<String, i32>,
    #[serde(rename = "validMissionCount")]
    pub valid_mission_count: i32,
}

#[derive(Serialize)]
pub struct PlayerInfo {
    #[serde(rename = "characterMap")]
    // character_game_id -> name
    pub character_map: HashMap<String, String>,
    #[serde(rename = "playerData")]
    // player_name -> PlayerData
    pub player_data: HashMap<String, PlayerData>,
    #[serde(rename = "prevPlayerData")]
    pub prev_player_data: HashMap<String, PlayerData>,
}

#[derive(Serialize)]
pub struct CharacterGeneralData {
    #[serde(rename = "playerIndex")]
    pub player_index: f64,
    #[serde(rename = "reviveNum")]
    pub revive_num: f64,
    #[serde(rename = "deathNum")]
    pub death_num: f64,
    #[serde(rename = "mineralsMined")]
    pub minerals_mined: f64,
    #[serde(rename = "supplyCount")]
    pub supply_count: f64,
    #[serde(rename = "supplyEfficiency")]
    pub supply_efficiency: f64,
}

#[derive(Serialize)]
pub struct CharacterGeneralInfo {
    #[serde(rename = "characterMapping")]
    pub character_mapping: HashMap<String, String>,
    #[serde(rename = "characterData")]
    pub character_data: HashMap<String, CharacterGeneralData>,
}

#[derive(Serialize)]
pub struct CharacterChoiceInfo {
    #[serde(rename = "characterChoiceCount")]
    pub character_choice_count: HashMap<String, i32>,
    #[serde(rename = "characterMapping")]
    pub character_mapping: HashMap<String, String>,
}

pub const MISSION_TIME_RESOLUTION_SEC: u16 = 15;
pub const GAME_TIME_RESOLUTION_SEC: u32 = 60;

#[derive(Serialize, Deserialize)]
pub struct GameTimeInfo {
    #[serde(rename = "missionTimeResolution")]
    pub mission_time_resolution: u16,
    #[serde(rename = "gameTimeResolution")]
    pub game_time_resolution: u32,
    #[serde(rename = "missionTimeDistribution")]
    pub mission_time_distribution: HashMap<i16, i32>,
    #[serde(rename = "gameTimeDistribution")]
    pub game_time_distribution: HashMap<i32, i32>,
}