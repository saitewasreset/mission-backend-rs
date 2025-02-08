pub mod admin;
pub mod cache;
pub mod damage;
pub mod db;
pub mod general;
pub mod info;
pub mod kpi;
pub mod mission;

use std::collections::HashSet;
use actix_web::{get, post, web::{Data, Json}, HttpRequest, HttpResponse, Responder};
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Mutex;
use actix_web::cookie::Cookie;
use actix_web::web::Bytes;
use common::{APIMapping, APIResponse, Mapping};
use crate::cache::manager::CacheManager;
use uuid::Uuid;

pub type DbPool = Pool<ConnectionManager<PgConnection>>;

pub type DbConn = PooledConnection<ConnectionManager<PgConnection>>;


pub struct AppState {
    access_token: Option<String>,
    instance_path: PathBuf,
    valid_session: Mutex<HashSet<Uuid>>,
}

impl AppState {
    pub fn new(access_token: Option<String>, instance_path: PathBuf) -> Self {
        AppState {
            access_token,
            instance_path,
            valid_session: Mutex::new(HashSet::new()),
        }
    }

    pub fn get_access_token(&self) -> Option<&str> {
        self.access_token.as_deref()
    }

    pub fn check_access_token(&self, provided_token: &str) -> bool {
        if let Some(access_token) = &self.access_token {
            provided_token == access_token
        } else {
            true
        }
    }

    pub fn check_session(&self, request: &HttpRequest) -> bool {
        if let Some(provided_session_id) = request.cookie("session_id") {
            if let Ok(provided_session_uuid) = Uuid::try_from(provided_session_id.value()) {
                self.valid_session.lock().unwrap().contains(&provided_session_uuid)
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn new_session(&self) -> Uuid {
        let new_uuid = Uuid::new_v4();

        self.valid_session.lock().unwrap().insert(new_uuid);

        new_uuid
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

#[post("/login")]
pub async fn login(app_state: Data<AppState>,
                   body: Bytes) -> impl Responder {
    if let Ok(access_token) = String::from_utf8(body.to_vec()) {
        if app_state.check_access_token(&access_token) {
            let session_id = app_state.new_session();

            let cookie = Cookie::build("session_id", session_id.to_string())
                .path("/")
                .http_only(true)
                .finish();

            HttpResponse::Ok().cookie(cookie).json(APIResponse::ok(()))
        } else {
            HttpResponse::Ok().json(APIResponse::<()>::unauthorized())
        }
    } else {
        HttpResponse::Ok().json(APIResponse::<()>::unauthorized())
    }
}

#[get("/check_session")]
pub async fn check_session(app_state: Data<AppState>,
                           request: HttpRequest) -> Json<APIResponse<()>> {
    if app_state.check_session(&request) {
        Json(APIResponse::ok(()))
    } else {
        Json(APIResponse::unauthorized())
    }
}