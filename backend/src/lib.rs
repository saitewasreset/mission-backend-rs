pub mod admin;
pub mod cache;
pub mod damage;
pub mod db;
pub mod general;
pub mod info;
pub mod kpi;
pub mod mission;
use actix_web::{get, web::{Data, Json}, HttpRequest};
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use common::kpi::{KPIConfig};
use serde::Deserialize;
use std::path::PathBuf;
use common::{APIMapping, APIResponse, Mapping};
use crate::cache::manager::CacheManager;

pub type DbPool = Pool<ConnectionManager<PgConnection>>;

pub type DbConn = PooledConnection<ConnectionManager<PgConnection>>;


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
