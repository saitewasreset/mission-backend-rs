use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct APICacheStatusItem {
    #[serde(rename = "cacheType")]
    pub cache_type: String,
    #[serde(rename = "lastUpdate")]
    pub last_update: i64,
    #[serde(rename = "lastSuccess")]
    pub last_success: bool,
    // count, load_from_db(ms), generate(ms)
    #[serde(rename = "lastSuccessData")]
    pub last_success_data: (i64, f64, f64),
    #[serde(rename = "lastErrorMessage")]
    pub last_error_message: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct APICacheStatus {
    pub working: bool,
    pub items: Vec<APICacheStatusItem>,
}

#[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash)]
pub enum APICacheType {
    MissionRaw,
    MissionKPIRaw,
    GlobalKPIState,
    All,
}
