use crate::APIResponse;
use actix_web::web::Buf;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use std::fmt::Display;

#[derive(Clone, Copy)]
pub enum CacheType {
    MissionRawCache,
    MissionKPIRawCache,
    GlobalKPIState,
}

impl CacheType {
    pub fn url_path(&self) -> &'static str {
        match self {
            CacheType::MissionRawCache => "/cache/update_mission_raw",
            CacheType::MissionKPIRawCache => "/cache/update_mission_kpi_raw",
            CacheType::GlobalKPIState => "/cache/update_global_kpi_state",
        }
    }
}

impl Display for CacheType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheType::MissionRawCache => write!(f, "MissionRawCache"),
            CacheType::MissionKPIRawCache => write!(f, "MissionKPIRawCache"),
            CacheType::GlobalKPIState => write!(f, "GlobalKPIState"),
        }
    }
}

fn update_specific_cache(
    cache_type: CacheType,
    endpoint_url: &str,
    http_client: &Client,
) -> Result<(), String> {
    let update_url = format!("{}{}", endpoint_url, cache_type.url_path());

    match http_client.get(&update_url).send() {
        Ok(response) => match response.status() {
            StatusCode::OK => {
                let body = response.bytes().expect("failed fetching response body");
                match serde_json::from_reader::<_, APIResponse<()>>(body.reader()) {
                    Ok(x) => {
                        if x.code != 200 {
                            Err(format!(
                                "failed updating cache {}: {} {}",
                                cache_type, x.code, x.message
                            ))
                        } else {
                            Ok(())
                        }
                    }
                    Err(e) => Err(format!("failed parsing response body {}", e)),
                }
            }
            _ => Err(format!(
                "failed fetching cache update response with status code {}",
                response.status()
            )),
        },
        Err(e) => Err(format!("failed sending request: {}", e)),
    }
}

pub fn update_cache(
    cache_type_list: &[CacheType],
    endpoint_url: &str,
    http_client: &Client,
) -> Result<(), String> {
    for &cache_type in cache_type_list {
        if let Err(e) = update_specific_cache(cache_type, endpoint_url, http_client) {
            return Err(format!("failed updating cache {}: {}", cache_type, e));
        }
    }

    Ok(())
}

pub fn author_info() {
    println!("Mission Monitor backend toolset");
    println!("made by saitewasreset with love");
    println!("Source: https://github.com/saitewasreset/mission-backend-rs");

    println!();
    println!("Afraid of the dark? No need, you got me!");
    println!();
}
