pub mod kpi;
pub mod mission;
pub mod damage;
pub mod info;
pub mod general;
pub mod cache;
pub mod mission_log;

use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use log::error;
use phf::{phf_map, Map};
use serde::{Deserialize, Serialize};
use crate::kpi::KPIComponent;

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