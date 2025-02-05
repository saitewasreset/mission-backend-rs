pub mod kpi;
pub mod mission;
pub mod manager;

use crate::{APIResponse, AppState};
use actix_web::{get, web::{self, Data, Json}, HttpRequest};
use log::error;
use crate::cache::manager::{CacheManager, CacheType};

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

pub fn scoped_config(cfg: &mut web::ServiceConfig) {
    cfg.service(update_mission_raw_cache);
    cfg.service(update_mission_kpi_cache);
    cfg.service(update_global_kpi_state);
}
