pub mod admin;
pub mod cache;
pub mod client;
pub mod damage;
pub mod db;
pub mod general;
pub mod info;
pub mod kpi;
pub mod mission;
use actix_web::{get, web::{Data, Json}, HttpRequest};
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use kpi::{KPIComponent, KPIConfig};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::path::PathBuf;
use log::error;
use phf::{Map, phf_map};
use crate::cache::manager::CacheManager;

pub type DbPool = Pool<ConnectionManager<PgConnection>>;

pub type DbConn = PooledConnection<ConnectionManager<PgConnection>>;

pub const NITRA_GAME_ID: &str = "RES_VEIN_Nitra";
pub const FLOAT_EPSILON: f64 = 1e-3;
pub const KPI_CALCULATION_PLAYER_INDEX: f64 = 0.5;

pub const KPI_VERSION: &str = "0.3.0";

pub const RE_SPOT_TIME_THRESHOLD: i64 = 60 * 60 * 24;

pub const INVALID_MISSION_TIME_THRESHOLD: i16 = 60 * 5;

pub const CORRECTION_ITEMS: &[KPIComponent] = &[
    KPIComponent::Damage,
    KPIComponent::Priority,
    KPIComponent::Kill,
    KPIComponent::Nitra,
    KPIComponent::Minerals,
];

pub const TRANSFORM_KPI_COMPONENTS: &[KPIComponent] = &[
    KPIComponent::Damage,
    KPIComponent::Priority,
    KPIComponent::Kill,
    KPIComponent::Nitra,
    KPIComponent::Minerals,
];

pub static WEAPON_TYPE: Map<&'static str, i16> = phf_map! {
    "WPN_FlameThrower" => 0,
    "WPN_Cryospray" => 0,
    "WPN_GooCannon" => 0,
    "WPN_Pistol_A" => 1,
    "WPN_ChargeBlaster" => 1,
    "WPN_MicrowaveGun" => 1,
    "WPN_CombatShotgun" => 0,
    "WPN_SMG_OneHand" => 0,
    "WPN_LockOnRifle" => 0,
    "WPN_GrenadeLauncher" => 1,
    "WPN_LineCutter" => 1,
    "WPN_HeavyParticleCannon" => 1,
    "WPN_Gatling" => 0,
    "WPN_Autocannon" => 0,
    "WPN_MicroMissileLauncher" => 0,
    "WPN_Revolver" => 1,
    "WPN_BurstPistol" => 1,
    "WPN_CoilGun" => 1,
    "WPN_AssaultRifle" => 0,
    "WPN_M1000" => 0,
    "WPN_PlasmaCarbine" => 0,
    "WPN_SawedOffShotgun" => 1,
    "WPN_DualMPs" => 1,
    "WPN_Crossbow" => 1,
};

pub static WEAPON_ORDER: Map<&'static str, i16> = phf_map! {
    "WPN_FlameThrower" => 0,
    "WPN_Cryospray" => 1,
    "WPN_GooCannon" => 2,
    "WPN_Pistol_A" => 3,
    "WPN_ChargeBlaster" => 4,
    "WPN_MicrowaveGun" => 5,
    "WPN_CombatShotgun" => 6,
    "WPN_SMG_OneHand" => 7,
    "WPN_LockOnRifle" => 8,
    "WPN_GrenadeLauncher" => 9,
    "WPN_LineCutter" => 10,
    "WPN_HeavyParticleCannon" => 11,
    "WPN_Gatling" => 12,
    "WPN_Autocannon" => 13,
    "WPN_MicroMissileLauncher" => 14,
    "WPN_Revolver" => 15,
    "WPN_BurstPistol" => 16,
    "WPN_CoilGun" => 17,
    "WPN_AssaultRifle" => 18,
    "WPN_M1000" => 19,
    "WPN_PlasmaCarbine" => 20,
    "WPN_SawedOffShotgun" => 21,
    "WPN_DualMPs" => 22,
    "WPN_Crossbow" => 23,
};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Mapping {
    #[serde(default)]
    pub character_mapping: HashMap<String, String>,
    #[serde(default)]
    pub entity_mapping: HashMap<String, String>,
    #[serde(default)]
    pub entity_blacklist_set: HashSet<String>,
    #[serde(default)]
    pub entity_combine: HashMap<String, String>,
    #[serde(default)]
    pub mission_type_mapping: HashMap<String, String>,
    #[serde(default)]
    pub resource_mapping: HashMap<String, String>,
    #[serde(default)]
    pub weapon_mapping: HashMap<String, String>,
    #[serde(default)]
    pub weapon_combine: HashMap<String, String>,
    #[serde(default)]
    pub weapon_character: HashMap<String, String>,
    #[serde(default)]
    pub scout_special_player_set: HashSet<String>,
}

pub struct AppState {
    access_token: Option<String>,
    instance_path: PathBuf,
}

impl AppState {
    pub fn new(access_token: Option<String>, instance_path: PathBuf) -> Self {
        AppState {
            access_token,
            instance_path,
        }
    }

    pub fn get_access_token(&self) -> Option<&str> {
        self.access_token.as_deref()
    }

    pub fn check_access_token(&self, request: &HttpRequest) -> bool {
        if let Some(access_token) = &self.access_token {
            if let Some(provided_access_token) = request.cookie("access_token") {
                provided_access_token.value() == access_token
            } else {
                false
            }
        } else {
            true
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct APIResponse<T: Serialize> {
    pub code: i32,
    pub message: String,
    pub data: Option<T>,
}

impl<T: Serialize> APIResponse<T> {
    pub fn new(code: i32, message: String, data: Option<T>) -> Self {
        APIResponse {
            code,
            message,
            data,
        }
    }

    pub fn ok(data: T) -> Self {
        APIResponse {
            code: 200,
            message: "Rock and stone!".to_string(),
            data: Some(data),
        }
    }

    pub fn from_result<E: Display>(data: Result<T, E>, error_log_info: impl Display) -> Self {
        match data {
            Ok(x) => APIResponse::ok(x),
            Err(e) => {
                error!("{}: {}", error_log_info, e);
                APIResponse::internal_error()
            }
        }
    }

    pub fn from_result_option<E: Display>(data: Result<Option<T>, E>, error_log_info: impl Display) -> Self {
        match data {
            Ok(Some(x)) => APIResponse::ok(x),
            Ok(None) => APIResponse::not_found(),
            Err(e) => {
                error!("{}: {}", error_log_info, e);
                APIResponse::internal_error()
            }
        }
    }

    pub fn unauthorized() -> Self {
        APIResponse {
            code: 403,
            message: "Sorry, but this was meant to be a private game: invalid access token"
                .to_string(),
            data: None,
        }
    }

    pub fn bad_request(message: &str) -> Self {
        APIResponse {
            code: 400,
            message: message.into(),
            data: None,
        }
    }

    pub fn not_found() -> Self {
        APIResponse {
            code: 404,
            message: "Sorry, but this was meant to be a private game: the requested resource was not found".to_string(),
            data: None,
        }
    }

    pub fn internal_error() -> Self {
        APIResponse {
            code: 500,
            message: "Multiplayer Session Ended: an internal server error has occurred".to_string(),
            data: None,
        }
    }

    pub fn config_required(for_what: &str) -> Self {
        APIResponse {
            code: 1001,
            message: format!(
                "Multiplayer Session Ended: the server requires configuration for {}",
                for_what
            ),
            data: None,
        }
    }

    pub fn busy(for_what: &str) -> Self {
        APIResponse {
            code: 1002,
            message: format!(
                "Multiplayer Session Ended: the server is busy processing {}",
                for_what
            ),
            data: None,
        }
    }
}

#[derive(Deserialize)]
pub struct ClientConfig {
    #[serde(default)]
    pub access_token: Option<String>,
    pub endpoint_url: String,
    #[serde(default)]
    pub mapping_path: Option<String>,
    #[serde(default)]
    pub watchlist_path: Option<String>,
    #[serde(default)]
    pub kpi_config_path: Option<String>,
}

#[derive(Serialize)]
pub struct APIMapping {
    pub character: HashMap<String, String>,
    pub entity: HashMap<String, String>,
    #[serde(rename = "entityBlacklist")]
    pub entity_blacklist: Vec<String>,
    #[serde(rename = "entityCombine")]
    pub entity_combine: HashMap<String, String>,
    #[serde(rename = "missionType")]
    pub mission_type: HashMap<String, String>,
    pub resource: HashMap<String, String>,
    pub weapon: HashMap<String, String>,
    #[serde(rename = "weaponCombine")]
    pub weapon_combine: HashMap<String, String>,
    #[serde(rename = "weaponHero")]
    pub weapon_character: HashMap<String, String>,
}

pub fn hazard_id_to_real(hazard_id: i16) -> f64 {
    match hazard_id {
        1..6 => hazard_id as f64,
        100 => 3.0,
        101 => 3.5,
        102 => 3.5,
        103 => 4.5,
        104 => 5.0,
        105 => 5.5,
        _ => unreachable!("invalid hazard id"),
    }
}

pub fn generate_mapping(mapping: Mapping) -> APIMapping {
    APIMapping {
        character: mapping.character_mapping,
        entity: mapping.entity_mapping,
        entity_blacklist: mapping.entity_blacklist_set.into_iter().collect(),
        entity_combine: mapping.entity_combine,
        mission_type: mapping.mission_type_mapping,
        resource: mapping.resource_mapping,
        weapon: mapping.weapon_mapping,
        weapon_combine: mapping.weapon_combine,
        weapon_character: mapping.weapon_character,
    }
}

#[get("/mapping")]
pub async fn get_mapping(cache_manager: Data<CacheManager>) -> Json<APIResponse<APIMapping>> {
    let mapping = cache_manager.get_mapping();
    Json(APIResponse::ok(generate_mapping(mapping)))
}

#[get("/heartbeat")]
pub async fn echo_heartbeat() -> Json<APIResponse<()>> {
    Json(APIResponse::ok(()))
}
