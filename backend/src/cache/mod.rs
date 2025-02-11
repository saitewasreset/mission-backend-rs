pub mod kpi;
pub mod mission;
pub mod manager;

use crate::{api_parse_json_body, APIResponse, AppState};
use actix_web::{get, web::{self, Data, Json}, HttpRequest};
use actix_web::web::Bytes;
use log::error;
use common::cache::{APICacheStatus, APICacheType};
use crate::cache::manager::{CacheManager, CacheType};


pub fn api_try_schedule_cache(cache_manager: &CacheManager, cache_type: CacheType) -> APIResponse<()> {
    match cache_manager.try_schedule(cache_type) {
        Ok(true) => APIResponse::ok(()),
        Ok(false) => APIResponse::busy("cache queue is full"),
        Err(e) => {
            error!("{}", e);
            APIResponse::internal_error()
        }
    }
}

pub fn api_try_schedule_cache_all(cache_manager: &CacheManager) -> APIResponse<()> {
    match cache_manager.try_schedule_all() {
        Ok(true) => APIResponse::ok(()),
        Ok(false) => APIResponse::busy("cache queue is full"),
        Err(e) => {
            error!("{}", e);
            APIResponse::internal_error()
        }
    }
}

#[get("/update_cache")]
async fn update_cache(
    app_state: Data<AppState>,
    cache_manager: Data<CacheManager>,
    request: HttpRequest,
    body: Bytes,
) -> Json<APIResponse<()>> {
    if !app_state.check_session(&request) {
        return Json(APIResponse::unauthorized());
    }

    if let Ok(api_cache_type) = api_parse_json_body(body) {
        match api_cache_type {
            APICacheType::MissionRaw => {
                Json(api_try_schedule_cache(&cache_manager, CacheType::MissionRaw))
            }
            APICacheType::MissionKPIRaw => {
                Json(api_try_schedule_cache(&cache_manager, CacheType::MissionKPIRaw))
            }
            APICacheType::GlobalKPIState => {
                Json(api_try_schedule_cache(&cache_manager, CacheType::GlobalKPIState))
            }
            APICacheType::All => {
                Json(api_try_schedule_cache_all(&cache_manager))
            }
        }
    } else {
        Json(APIResponse::bad_request("cannot parse payload body as json"))
    }
}

#[get("/cache_status")]
async fn get_cache_status(
    app_state: Data<AppState>,
    cache_manager: Data<CacheManager>,
    request: HttpRequest,
) -> Json<APIResponse<APICacheStatus>> {
    if !app_state.check_session(&request) {
        return Json(APIResponse::unauthorized());
    }

    let result = cache_manager.get_api_cache_status();

    Json(APIResponse::ok(result))
}

pub fn scoped_config(cfg: &mut web::ServiceConfig) {
    cfg.service(update_cache);
    cfg.service(get_cache_status);
}
