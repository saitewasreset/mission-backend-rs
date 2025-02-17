use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct KPIVersionInfo {
    pub version: String,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum CharacterKPIType {
    Driller,
    Engineer,
    Gunner,
    Scout,
    ScoutSpecial,
}

impl TryFrom<i16> for CharacterKPIType {
    type Error = String;
    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(CharacterKPIType::Driller),
            1 => Ok(CharacterKPIType::Gunner),
            2 => Ok(CharacterKPIType::Engineer),
            3 => Ok(CharacterKPIType::Scout),
            4 => Ok(CharacterKPIType::ScoutSpecial),
            _ => Err(format!("Invalid character type: {}", value)),
        }
    }
}

impl Display for CharacterKPIType {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            CharacterKPIType::Driller => write!(f, "driller"),
            CharacterKPIType::Engineer => write!(f, "engineer"),
            CharacterKPIType::Gunner => write!(f, "gunner"),
            CharacterKPIType::Scout => write!(f, "scout"),
            CharacterKPIType::ScoutSpecial => write!(f, "scout_special"),
        }
    }
}

impl CharacterKPIType {
    pub fn from_player(
        character_game_id: &str,
        player_name: &str,
        scout_special_player_set: &HashSet<String>,
    ) -> CharacterKPIType {
        match character_game_id {
            "DRILLER" => CharacterKPIType::Driller,
            "ENGINEER" => CharacterKPIType::Engineer,
            "GUNNER" => CharacterKPIType::Gunner,
            "SCOUT" => {
                if scout_special_player_set.contains(player_name) {
                    CharacterKPIType::ScoutSpecial
                } else {
                    CharacterKPIType::Scout
                }
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Debug)]
pub struct KPIComponentData {
    pub index_weight: f64,
    pub source_value: f64,
    pub source_total: f64,
    pub weighted_value: f64,
    pub weighted_total: f64,
    pub source_index: f64,
    pub weighted_index: f64,
    pub weighted_rank: f64,
    pub transform_coefficient: (f64, f64),
    pub transformed_value: f64,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum KPIComponent {
    Kill,
    Damage,
    Priority,
    Revive,
    Death,
    FriendlyFire,
    Nitra,
    Supply,
    Minerals,
}

impl Display for KPIComponent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            KPIComponent::Kill => write!(f, "kill"),
            KPIComponent::Damage => write!(f, "damage"),
            KPIComponent::Priority => write!(f, "priority"),
            KPIComponent::Revive => write!(f, "revive"),
            KPIComponent::Death => write!(f, "death"),
            KPIComponent::FriendlyFire => write!(f, "friendly_fire"),
            KPIComponent::Nitra => write!(f, "nitra"),
            KPIComponent::Supply => write!(f, "supply"),
            KPIComponent::Minerals => write!(f, "minerals"),
        }
    }
}

impl TryFrom<&str> for KPIComponent {
    type Error = String;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "kill" => Ok(KPIComponent::Kill),
            "damage" => Ok(KPIComponent::Damage),
            "priority" => Ok(KPIComponent::Priority),
            "revive" => Ok(KPIComponent::Revive),
            "death" => Ok(KPIComponent::Death),
            "friendly_fire" => Ok(KPIComponent::FriendlyFire),
            "nitra" => Ok(KPIComponent::Nitra),
            "supply" => Ok(KPIComponent::Supply),
            "minerals" => Ok(KPIComponent::Minerals),
            _ => Err(format!("Invalid KPI component: {}", value)),
        }
    }
}

impl FromStr for KPIComponent {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}

impl From<KPIComponent> for i16 {
    fn from(value: KPIComponent) -> Self {
        match value {
            KPIComponent::Kill => 0,
            KPIComponent::Damage => 1,
            KPIComponent::Priority => 2,
            KPIComponent::Revive => 3,
            KPIComponent::Death => 4,
            KPIComponent::FriendlyFire => 5,
            KPIComponent::Nitra => 6,
            KPIComponent::Supply => 7,
            KPIComponent::Minerals => 8,
        }
    }
}

impl KPIComponent {
    pub fn to_string_zh(&self) -> String {
        match self {
            KPIComponent::Kill => "击杀数指数".to_string(),
            KPIComponent::Damage => "输出指数".to_string(),
            KPIComponent::Priority => "高威胁目标".to_string(),
            KPIComponent::Revive => "救人指数".to_string(),
            KPIComponent::Death => "倒地指数".to_string(),
            KPIComponent::FriendlyFire => "友伤指数".to_string(),
            KPIComponent::Nitra => "硝石指数".to_string(),
            KPIComponent::Supply => "补给指数".to_string(),
            KPIComponent::Minerals => "采集指数".to_string(),
        }
    }

    pub fn max_value(&self) -> f64 {
        match self {
            KPIComponent::Kill => 1.0,
            KPIComponent::Damage => 1.0,
            KPIComponent::Priority => 1.0,
            KPIComponent::Revive => 1.0,
            KPIComponent::Death => 0.0,
            KPIComponent::FriendlyFire => 1.0,
            KPIComponent::Nitra => 1.0,
            KPIComponent::Supply => 0.0,
            KPIComponent::Minerals => 1.0,
        }
    }
}

impl TryFrom<usize> for KPIComponent {
    type Error = String;
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(KPIComponent::Kill),
            1 => Ok(KPIComponent::Damage),
            2 => Ok(KPIComponent::Priority),
            3 => Ok(KPIComponent::Revive),
            4 => Ok(KPIComponent::Death),
            5 => Ok(KPIComponent::FriendlyFire),
            6 => Ok(KPIComponent::Nitra),
            7 => Ok(KPIComponent::Supply),
            8 => Ok(KPIComponent::Minerals),
            _ => Err(format!("Invalid KPI component id: {}", value)),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct KPIConfig {
    pub character_weight_table: HashMap<CharacterKPIType, HashMap<String, f64>>,
    pub priority_table: HashMap<String, f64>,
    pub resource_weight_table: HashMap<String, f64>,
    pub character_component_weight: HashMap<CharacterKPIType, HashMap<KPIComponent, f64>>,
    pub transform_range: Vec<IndexTransformRangeConfig>,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct IndexTransformRangeConfig {
    pub rank_range: (f64, f64),
    pub transform_range: (f64, f64),
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct IndexTransformRange {
    #[serde(rename = "rankRange")]
    pub rank_range: (f64, f64),
    #[serde(rename = "sourceRange")]
    pub source_range: (f64, f64),
    #[serde(rename = "transformRange")]
    pub transform_range: (f64, f64),
    // y = ax + b
    #[serde(rename = "transformCoefficient")]
    pub transform_coefficient: (f64, f64),
    #[serde(rename = "playerCount")]
    pub player_count: i32,
}

#[derive(Serialize, Deserialize)]
pub struct APIWeightTableData {
    #[serde(rename = "entityGameId")]
    pub entity_game_id: String,
    pub priority: f64,
    pub driller: f64,
    pub gunner: f64,
    pub engineer: f64,
    pub scout: f64,
    #[serde(rename = "scoutSpecial")]
    pub scout_special: f64,
}

#[derive(Serialize, Deserialize)]
pub struct GammaInnerInfo {
    #[serde(rename = "playerIndex")]
    pub player_index: f64,
    pub value: f64,
    pub ratio: f64,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct PlayerMissionKPIInfo {
    #[serde(rename = "missionId")]
    pub mission_id: i32,
    #[serde(rename = "beginTimestamp")]
    pub begin_timestamp: i64,
    #[serde(rename = "playerIndex")]
    pub player_index: f64,
    #[serde(rename = "missionKPI")]
    pub mission_kpi: f64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PlayerCharacterKPIInfo {
    #[serde(rename = "playerIndex")]
    pub player_index: f64,
    #[serde(rename = "characterKPI")]
    pub character_kpi: f64,
    #[serde(rename = "characterKPIType")]
    pub character_kpi_type: String,
    #[serde(rename = "missionList")]
    pub mission_list: Vec<PlayerMissionKPIInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct PlayerKPIInfo {
    #[serde(rename = "playerIndex")]
    pub player_index: f64,
    #[serde(rename = "playerKPI")]
    pub player_kpi: f64,
    #[serde(rename = "byCharacter")]
    pub by_character: HashMap<String, PlayerCharacterKPIInfo>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct PlayerAssignedKPIInfo {
    pub by_component: HashMap<KPIComponent, f64>,
    pub overall: Option<f64>,
    pub note: String,
}

#[derive(Serialize, Deserialize)]
pub struct APIAssignedKPI {
    pub mission_id: i32,
    pub player_name: String,
    pub player_assigned_kpi_info: PlayerAssignedKPIInfo,
}

#[derive(Serialize, Deserialize)]
pub struct APIDeleteAssignedKPI {
    pub mission_id: i32,
    pub player_name: String,
}