pub mod kpi;
pub mod mission;
pub mod manager;

use crate::{APIResponse, AppState};
use actix_web::{get, web::{self, Data, Json}, HttpRequest};
use log::error;
use crate::cache::manager::{CacheManager, CacheType};
use serde::Serialize;

#[derive(Serialize, Clone, PartialEq)]
pub struct APICacheStatusItem {
    #[serde(rename = "cacheType")]
    cache_type: String,
    #[serde(rename = "lastUpdate")]
    last_update: i64,
    #[serde(rename = "lastSuccess")]
    last_success: bool,
    // count, load_from_db(ms), generate(ms)
    #[serde(rename = "lastSuccessData")]
    last_success_data: (i64, f64, f64),
    #[serde(rename = "lastErrorMessage")]
    last_error_message: String,
}

#[derive(Serialize, Clone, PartialEq)]
pub struct APICacheStatus {
    working: bool,
    items: Vec<APICacheStatusItem>,
}

impl From<&CacheManager> for APICacheStatus {
    fn from(manager: &CacheManager) -> Self {
        APICacheStatus {
            working: manager.is_working(),
            items: manager.get_cache_status_all().iter().map(|(k, v)| {
                APICacheStatusItem {
                    cache_type: k.to_string(),
                    last_update: v.0,
                    last_success: v.1.is_ok(),
                    last_success_data: match &v.1 {
                        Ok(x) => (x.count as i64, x.load_from_db.unwrap_or_default().as_millis() as f64, x.generate.as_millis() as f64),
                        Err(_) => (0, 0.0, 0.0),
                    },
                    last_error_message: match &v.1 {
                        Ok(_) => "".to_string(),
                        Err(e) => e.to_string(),
                    },
                }
            }).collect(),
        }
    }
}

pub fn api_try_schedule_cache(cache_manager: &CacheManager, cache_type: CacheType) -> APIResponse<()> {
    match cache_manager.try_schedule(cache_type) {
        Ok(true) => APIResponse::ok(()),
        Ok(false) => APIResponse::busy("cache queue is full"),
        Err(()) => {
            error!("cache manager thread is dead");
            APIResponse::internal_error()
        }
    }
}

#[get("/update_mission_raw")]
async fn update_mission_raw_cache(
    app_state: Data<AppState>,
    cache_manager: Data<CacheManager>,
    request: HttpRequest,
) -> Json<APIResponse<()>> {
    if !app_state.check_access_token(&request) {
        return Json(APIResponse::unauthorized());
    }

    Json(api_try_schedule_cache(&cache_manager, CacheType::MissionRaw))
}

#[get("/update_mission_kpi_raw")]
async fn update_mission_kpi_cache(
    app_state: Data<AppState>,
    cache_manager: Data<CacheManager>,
    request: HttpRequest,
) -> Json<APIResponse<()>> {
    if !app_state.check_access_token(&request) {
        return Json(APIResponse::unauthorized());
    }

    Json(api_try_schedule_cache(&cache_manager, CacheType::MissionKPIRaw))
}

#[get("/update_global_kpi_state")]
async fn update_global_kpi_state(
    app_state: Data<AppState>,
    cache_manager: Data<CacheManager>,
    request: HttpRequest,
) -> Json<APIResponse<()>> {
    if !app_state.check_access_token(&request) {
        return Json(APIResponse::unauthorized());
    }

    Json(api_try_schedule_cache(&cache_manager, CacheType::GlobalKPIState))
}

#[get("/cache_status")]
async fn get_cache_status(
    app_state: Data<AppState>,
    cache_manager: Data<CacheManager>,
    request: HttpRequest,
) -> Json<APIResponse<APICacheStatus>> {
    if !app_state.check_access_token(&request) {
        return Json(APIResponse::unauthorized());
    }

    let result = cache_manager.as_ref().into();

    Json(APIResponse::ok(result))
}

pub fn scoped_config(cfg: &mut web::ServiceConfig) {
    cfg.service(update_mission_raw_cache);
    cfg.service(update_mission_kpi_cache);
    cfg.service(update_global_kpi_state);
    cfg.service(get_cache_status);
}
