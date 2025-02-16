use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct APISetMissionInvalid {
    pub invalid: bool,
    pub mission_id: i32,
    pub reason: String,
}

#[derive(Serialize, Deserialize)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct APIMissionInvalid {
    pub mission_id: i32,
    pub reason: String,
}