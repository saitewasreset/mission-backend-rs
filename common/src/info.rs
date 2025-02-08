use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct OverallInfo {
    // 平均游戏局数
    #[serde(rename = "playerAverageSpot")]
    pub player_average_spot: f64,
    // 路人玩家数
    #[serde(rename = "unfamiliarPlayerCount")]
    pub unfamiliar_player_count: i32,
    // 再相遇概率
    #[serde(rename = "playerGeTwoPercent")]
    pub player_ge_two_percent: f64,
    //多于一局概率
    #[serde(rename = "playerSpotPercent")]
    pub player_spot_percent: f64,
}

#[derive(Serialize, Deserialize)]
pub struct PlayerInfo {
    #[serde(rename = "gameCount")]
    pub game_count: i32,
    #[serde(rename = "lastSpot")]
    pub last_spot: i64,
    #[serde(rename = "presenceTime")]
    pub presence_time: i32,
    // 再相遇次数
    #[serde(rename = "spotCount")]
    pub spot_count: i32,
    #[serde(rename = "timestampList")]
    pub timestamp_list: Vec<i64>,
}

#[derive(Serialize, Deserialize)]
pub struct APIBrothers {
    pub overall: OverallInfo,
    pub player: HashMap<String, PlayerInfo>,
}