use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use crate::damage::SupplyPack;

#[derive(Serialize, Deserialize)]
pub struct LoadResult {
    pub load_count: i32,
    pub decode_time: String,
    pub load_time: String,
}

#[derive(Serialize, Deserialize)]
pub struct APIMission {
    pub id: i32,
    pub begin_timestamp: i64,
    pub mission_time: i16,
    pub mission_type: String,
    pub hazard_id: i16,
    pub result: i16,
    pub reward_credit: f64,
    pub total_supply_count: i16,
}

#[derive(Serialize)]
pub struct MissionInfo {
    #[serde(rename = "missionId")]
    pub mission_id: i32,
    #[serde(rename = "beginTimestamp")]
    pub begin_timestamp: i64,
    #[serde(rename = "missionTime")]
    pub mission_time: i16,
    #[serde(rename = "missionTypeId")]
    pub mission_type_id: String,
    #[serde(rename = "hazardId")]
    pub hazard_id: i16,
    #[serde(rename = "missionResult")]
    pub mission_result: i16,
    #[serde(rename = "rewardCredit")]
    pub reward_credit: f64,
    #[serde(rename = "missionInvalid")]
    pub mission_invalid: bool,
    #[serde(rename = "missionInvalidReason")]
    pub mission_invalid_reason: String,
}

#[derive(Serialize)]
pub struct MissionList {
    #[serde(rename = "missionInfo")]
    pub mission_info: Vec<MissionInfo>,
    #[serde(rename = "missionTypeMapping")]
    pub mission_type_mapping: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct MissionGeneralInfo {
    #[serde(rename = "missionId")]
    pub mission_id: i32,
    #[serde(rename = "missionBeginTimestamp")]
    pub mission_begin_timestamp: i64,
    #[serde(rename = "missionInvalid")]
    pub mission_invalid: bool,
    #[serde(rename = "missionInvalidReason")]
    pub mission_invalid_reason: String,
}

#[derive(Serialize)]
pub struct MissionGeneralPlayerInfo {
    #[serde(rename = "characterGameId")]
    pub character_game_id: String,
    #[serde(rename = "playerRank")]
    pub player_rank: i16,
    #[serde(rename = "characterRank")]
    pub character_rank: i16,
    #[serde(rename = "characterPromotion")]
    pub character_promotion: i16,
    #[serde(rename = "presentTime")]
    pub present_time: i16,
    #[serde(rename = "reviveNum")]
    pub revive_num: i16,
    #[serde(rename = "deathNum")]
    pub death_num: i16,
    #[serde(rename = "playerEscaped")]
    pub player_escaped: bool,
}

#[derive(Serialize)]
pub struct MissionGeneralData {
    #[serde(rename = "beginTimeStamp")]
    pub begin_timestamp: i64,
    #[serde(rename = "hazardId")]
    pub hazard_id: i16,
    #[serde(rename = "missionResult")]
    pub mission_result: i16,
    #[serde(rename = "missionTime")]
    pub mission_time: i16,
    #[serde(rename = "missionTypeId")]
    pub mission_type_id: String,
    #[serde(rename = "playerInfo")]
    pub player_info: HashMap<String, MissionGeneralPlayerInfo>,
    #[serde(rename = "rewardCredit")]
    pub reward_credit: f64,
    #[serde(rename = "totalDamage")]
    pub total_damage: f64,
    #[serde(rename = "totalKill")]
    pub total_kill: i32,
    #[serde(rename = "totalMinerals")]
    pub total_minerals: f64,
    #[serde(rename = "totalNitra")]
    pub total_nitra: f64,
    #[serde(rename = "totalSupplyCount")]
    pub total_supply_count: i16,
}

#[derive(Serialize)]
pub struct PlayerFriendlyFireInfo {
    pub cause: HashMap<String, f64>,
    pub take: HashMap<String, f64>,
}

#[derive(Serialize)]
pub struct PlayerDamageInfo {
    pub damage: HashMap<String, f64>,
    pub kill: HashMap<String, i32>,
    pub ff: PlayerFriendlyFireInfo,
    #[serde(rename = "supplyCount")]
    pub supply_count: i16,
}

#[derive(Serialize)]
pub struct MissionDamageInfo {
    pub info: HashMap<String, PlayerDamageInfo>,
    #[serde(rename = "entityMapping")]
    pub entity_mapping: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct MissionWeaponDamageInfo {
    pub damage: f64,
    #[serde(rename = "friendlyFire")]
    pub friendly_fire: f64,
    #[serde(rename = "characterGameId")]
    pub character_game_id: String,
    #[serde(rename = "mappedName")]
    pub mapped_name: String,
}

#[derive(Serialize)]
pub struct PlayerResourceData {
    pub resource: HashMap<String, f64>,
    pub supply: Vec<SupplyPack>,
}

#[derive(Serialize)]
pub struct MissionResourceInfo {
    pub data: HashMap<String, PlayerResourceData>,
    #[serde(rename = "resourceMapping")]
    pub resource_mapping: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct MissionKPIComponent {
    pub name: String,
    #[serde(rename = "sourceValue")]
    pub source_value: f64,
    #[serde(rename = "weightedValue")]
    pub weighted_value: f64,
    #[serde(rename = "missionTotalWeightedValue")]
    pub mission_total_weighted_value: f64,
    #[serde(rename = "rawIndex")]
    pub raw_index: f64,
    #[serde(rename = "correctedIndex")]
    pub corrected_index: f64,
    #[serde(rename = "transformedIndex")]
    pub transformed_index: f64,
    pub weight: f64,
}

#[derive(Serialize)]
pub struct MissionKPIInfo {
    #[serde(rename = "playerName")]
    pub player_name: String,
    #[serde(rename = "kpiCharacterType")]
    pub kpi_character_type: String,
    #[serde(rename = "reviveNum")]
    pub revive_num: f64,
    #[serde(rename = "deathNum")]
    pub death_num: f64,
    #[serde(rename = "friendlyFire")]
    pub friendly_fire: f64,
    #[serde(rename = "supplyCount")]
    pub supply_count: f64,
}

impl From<MissionKPIInfoFull> for MissionKPIInfo {
    fn from(value: MissionKPIInfoFull) -> Self {
        MissionKPIInfo {
            player_name: value.player_name,
            kpi_character_type: value.kpi_character_type,
            revive_num: value.revive_num,
            death_num: value.death_num,
            friendly_fire: value.friendly_fire,
            supply_count: value.supply_count,
        }
    }
}

#[derive(Serialize)]
pub struct MissionKPIInfoFull {
    #[serde(rename = "playerName")]
    pub player_name: String,
    #[serde(rename = "kpiCharacterType")]
    pub kpi_character_type: String,
    #[serde(rename = "weightedKill")]
    pub weighted_kill: f64,
    #[serde(rename = "weightedDamage")]
    pub weighted_damage: f64,
    #[serde(rename = "priorityDamage")]
    pub priority_damage: f64,
    #[serde(rename = "reviveNum")]
    pub revive_num: f64,
    #[serde(rename = "deathNum")]
    pub death_num: f64,
    #[serde(rename = "friendlyFire")]
    pub friendly_fire: f64,
    pub nitra: f64,
    #[serde(rename = "supplyCount")]
    pub supply_count: f64,
    #[serde(rename = "weightedResource")]
    pub weighted_resource: f64,
    pub component: Vec<MissionKPIComponent>,
    #[serde(rename = "missionKPI")]
    pub mission_kpi: f64,
}
