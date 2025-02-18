use std::collections::HashSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::hash::RandomState;
use std::io::Read;
use std::path::PathBuf;
use clap::ValueEnum;
use clio::Input;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use common::admin::APISetMissionInvalid;
use common::cache::APICacheType;
use common::kpi::APIDeleteAssignedKPI;
use crate::api::{APIResult, Authenticated, MissionMonitorClient, NotAuthenticated};
use crate::assigned_kpi::{print_assigned_kpi, read_assigned_kpi};
use crate::cache_status::print_cache_status;
use crate::load::{compress, load_kpi_config_from_file, load_mapping_from_file, parse_config_file_list, parse_mission_log, LoadError};
use crate::mission_list::print_mission_list;

pub mod load;
pub mod api;

pub mod formatter;
pub mod cache_status;
pub mod mission_list;

pub mod mission_invalid;
pub mod kpi;
pub mod assigned_kpi;

pub const APP_NAME: &str = env!("CARGO_PKG_NAME");
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClientConfig {
    pub api_endpoint: String,
    pub cookie_path: PathBuf,
    pub mission_raw_log_path: PathBuf,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, ValueEnum)]
pub enum CliCacheType {
    MissionRaw,
    MissionKPIRaw,
    GlobalKPIState,
    All,
}

impl From<CliCacheType> for APICacheType {
    fn from(value: CliCacheType) -> Self {
        match value {
            CliCacheType::MissionRaw => APICacheType::MissionRaw,
            CliCacheType::MissionKPIRaw => APICacheType::MissionKPIRaw,
            CliCacheType::GlobalKPIState => APICacheType::GlobalKPIState,
            CliCacheType::All => APICacheType::All,
        }
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            api_endpoint: "http://localhost:8080/api".to_string(),
            cookie_path: PathBuf::from("./cookie.json"),
            mission_raw_log_path: PathBuf::from("./raw_log"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClientError {
    InputError(String),
    ParseError(String),
    NetworkError(String),
    APIError(String),
}

impl Display for ClientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::InputError(msg) => write!(f, "Input error: {}", msg),
            ClientError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            ClientError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            ClientError::APIError(msg) => write!(f, "API error: {}", msg),
        }
    }
}

impl<T: DeserializeOwned> From<APIResult<T>> for Result<T, ClientError> {
    fn from(value: APIResult<T>) -> Self {
        match value {
            APIResult::Success(x) => Ok(x),
            APIResult::APIError(code, message) => Err(ClientError::APIError(format!("API error {}: {}", code, message))),
            APIResult::NetworkError(e) => Err(ClientError::NetworkError(format!("Network error: {}", e))),
        }
    }
}

impl From<LoadError> for ClientError {
    fn from(e: LoadError) -> Self {
        match e {
            LoadError::IOError(e) => ClientError::InputError(e.to_string()),
            LoadError::ParseError(msg) => ClientError::ParseError(msg),
        }
    }
}

impl Error for ClientError {}

pub fn format_size(size: usize) -> String {
    match size {
        0..1024 => format!("{}B", size),
        1024..1048576 => format!("{:.2}KiB", size as f64 / 1024.0),
        1048576.. => format!("{:.2}MiB", size as f64 / (1024.0 * 1024.0)),
    }
}

pub fn client_from_local_cookie_unchecked(client_config: ClientConfig) -> Result<MissionMonitorClient<Authenticated>, ClientError> {
    let cookie_storage_content = match std::fs::read(&client_config.cookie_path) {
        Ok(content) => content,
        Err(e) => {
            return if matches!(e.kind(), std::io::ErrorKind::NotFound) {
                Err(ClientError::InputError("Please login first".into()))
            } else {
                Err(ClientError::InputError(format!("cannot read cookie file: {}", e)))
            }
        }
    };

    let client = MissionMonitorClient::<NotAuthenticated>::new(client_config.api_endpoint).load_cookie(&cookie_storage_content).map_err(|(msg, _)| msg)?;

    Ok(client)
}

fn client_login(client_config: ClientConfig, token_file: Option<Input>) -> Result<(), ClientError> {
    let token = match token_file {
        Some(mut token_file) => {
            let mut result = String::new();
            token_file.read_to_string(&mut result).map_err(|e| ClientError::InputError(format!("cannot read token from file: {}", e)))?;

            result
        }
        None => {
            rpassword::prompt_password("Access token: ").map_err(|e| ClientError::InputError(format!("cannot read token from stdin: {}", e)))?
        }
    };

    let client: MissionMonitorClient<Authenticated> = match MissionMonitorClient::<NotAuthenticated>::new(client_config.api_endpoint).login(token) {
        Ok(client) => client,
        Err((msg, _)) => match msg {
            APIResult::Success(_) => unreachable!(),
            APIResult::APIError(_, message) => return Err(ClientError::APIError(message)),
            APIResult::NetworkError(e) => return Err(ClientError::NetworkError(e.to_string())),
        },
    };

    client.save_cookie(&client_config.cookie_path)?;

    Ok(())
}

pub fn cli_login(client_config: ClientConfig, token_file: Option<Input>) -> Result<(), ClientError> {
    match client_from_local_cookie_unchecked(client_config.clone()) {
        Ok(mut client) => {
            match client.check_session() {
                APIResult::Success(()) => Ok(()),
                APIResult::APIError(_, msg) => {
                    println!("Authentication using saved token failed: {}", msg);
                    client_login(client_config, token_file)
                }
                APIResult::NetworkError(e) => Err(ClientError::NetworkError(e.to_string())),
            }
        }
        Err(e) => {
            println!("Authentication using saved token failed: {}", e);
            client_login(client_config, token_file)
        }
    }
}

pub fn cli_load_mission(client_config: ClientConfig) -> Result<(), ClientError> {
    println!("Parsing mission log...");
    let mission_list = parse_mission_log(&client_config.mission_raw_log_path)?;

    let mut client = client_from_local_cookie_unchecked(client_config)?;

    println!("Getting uploaded mission list...");

    let uploaded_mission_list = Result::from(client.get_api_mission_list())?;

    println!("uploaded mission count: {}", uploaded_mission_list.len());

    let uploaded_mission_timestamp_set: HashSet<_, RandomState> = HashSet::from_iter(uploaded_mission_list.iter().map(|m| m.begin_timestamp));

    let to_upload_mission_list = mission_list
        .into_iter()
        .filter(|mission|
            !uploaded_mission_timestamp_set.contains(&mission.mission_info.begin_timestamp))
        .collect::<Vec<_>>();

    println!("to upload mission count: {}", to_upload_mission_list.len());

    let serialized = rmp_serde::to_vec(&to_upload_mission_list).unwrap();

    let compressed = compress(&serialized);

    Result::from(client.load_mission(compressed))?;

    Ok(())
}

pub fn cli_update_cache(client_config: ClientConfig, cache_type: APICacheType) -> Result<(), ClientError> {
    let mut client = client_from_local_cookie_unchecked(client_config)?;

    client.update_cache(cache_type).into()
}


pub fn cli_get_cache_status(client_config: ClientConfig) -> Result<(), ClientError> {
    let mut client = client_from_local_cookie_unchecked(client_config)?;

    let cache_status = Result::from(client.get_cache_status())?;

    print_cache_status(cache_status);

    Ok(())
}

pub fn cli_get_mission_list(client_config: ClientConfig, entry_limit: Option<usize>) -> Result<(), ClientError> {
    let mut client = MissionMonitorClient::new(client_config.api_endpoint);

    let api_mission_list = Result::from(client.get_api_mission_list())?;

    print_mission_list(api_mission_list, entry_limit);

    Ok(())
}

pub fn cli_load_mapping(client_config: ClientConfig, mapping_directory: Option<PathBuf>) -> Result<(), ClientError> {
    let mut client = client_from_local_cookie_unchecked(client_config)?;

    let mapping_directory = mapping_directory.unwrap_or_else(|| PathBuf::from("mapping"));

    let mapping = load_mapping_from_file(&mapping_directory)?;

    Result::from(client.load_mapping(mapping))?;

    Ok(())
}

pub fn cli_load_kpi_config(client_config: ClientConfig, kpi_config_directory: Option<PathBuf>) -> Result<(), ClientError> {
    let mut client = client_from_local_cookie_unchecked(client_config)?;

    let kpi_config_directory = kpi_config_directory.unwrap_or_else(|| PathBuf::from("kpi"));

    let kpi_config = load_kpi_config_from_file(&kpi_config_directory)?;

    Result::from(client.load_kpi(kpi_config))?;

    Ok(())
}

pub fn cli_load_kpi_watchlist(client_config: ClientConfig, watchlist_path: Option<PathBuf>) -> Result<(), ClientError> {
    let mut client = client_from_local_cookie_unchecked(client_config)?;

    let kpi_config_directory = watchlist_path.unwrap_or_else(|| PathBuf::from("watchlist.txt"));

    let watchlist = parse_config_file_list(&kpi_config_directory)?;

    Result::from(client.load_watchlist(watchlist))?;

    Ok(())
}

pub fn cli_delete_mission_invalid(client_config: ClientConfig, mission_id: i32) -> Result<(), ClientError> {
    let mut client = client_from_local_cookie_unchecked(client_config)?;

    Result::from(client.set_mission_invalid(APISetMissionInvalid {
        invalid: false,
        mission_id,
        reason: String::new(),
    }))?;

    Ok(())
}

pub fn cli_add_mission_invalid(client_config: ClientConfig, mission_id: i32, reason: String) -> Result<(), ClientError> {
    let mut client = client_from_local_cookie_unchecked(client_config)?;

    Result::from(client.set_mission_invalid(APISetMissionInvalid {
        invalid: true,
        mission_id,
        reason,
    }))?;

    Ok(())
}

pub fn cli_get_mission_invalid(client_config: ClientConfig) -> Result<(), ClientError> {
    let mut client = client_from_local_cookie_unchecked(client_config)?;

    let mission_invalid_list = Result::from(client.get_mission_invalid())?;
    let mission_list = Result::from(client.get_api_mission_list())?;

    mission_invalid::print_mission_invalid_list(mission_invalid_list, mission_list);

    Ok(())
}

pub fn cli_server_init(client_config: ClientConfig) -> Result<(), ClientError> {
    println!("Loading watchlist...");
    cli_load_kpi_watchlist(client_config.clone(), None)?;

    println!("Loading kpi config...");
    cli_load_kpi_config(client_config.clone(), None)?;

    println!("Loading mapping...");
    cli_load_mapping(client_config.clone(), None)?;

    println!("Loading mission...");
    cli_load_mission(client_config.clone())?;

    Ok(())
}

pub fn cli_get_assigned_kpi(client_config: ClientConfig, mission_id: Option<i32>, player_name: Option<String>) -> Result<(), ClientError> {
    let mut client = MissionMonitorClient::new(client_config.api_endpoint);

    let assigned_kpi_list = Result::from(client.get_assigned_kpi())?;

    let assigned_kpi_list = assigned_kpi_list
        .into_iter()
        .filter(|x| {
            if let Some(mission_id) = mission_id {
                x.mission_id == mission_id
            } else {
                true
            }
        })
        .filter(|x| {
            if let Some(player_name) = &player_name {
                x.player_name.as_str() == player_name.as_str()
            } else {
                true
            }
        }).collect::<Vec<_>>();

    for assigned_kpi in &assigned_kpi_list {
        print_assigned_kpi(assigned_kpi);
    }

    Ok(())
}

pub fn cli_set_assigned_kpi(client_config: ClientConfig) -> Result<(), ClientError> {
    let mut client = client_from_local_cookie_unchecked(client_config)?;

    let assigned_kpi = read_assigned_kpi(&mut client).map_err(ClientError::InputError)?;

    Result::from(client.set_assigned_kpi(assigned_kpi))
}

pub fn cli_delete_assigned_kpi(client_config: ClientConfig, mission_id: i32, player_name: String) -> Result<(), ClientError> {
    let mut client = client_from_local_cookie_unchecked(client_config)?;

    Result::from(client.delete_assigned_kpi(APIDeleteAssignedKPI {
        mission_id,
        player_name,
    }))
}

pub fn cli_update_config(mut client_config: ClientConfig, config_path: Option<PathBuf>, api_endpoint: Option<String>, cookie_path: Option<PathBuf>, mission_raw_log_path: Option<PathBuf>) -> Result<(), ClientError> {
    if let Some(api_endpoint) = api_endpoint {
        client_config.api_endpoint = api_endpoint.clone();
    }

    if let Some(cookie_path) = cookie_path {
        client_config.cookie_path = cookie_path.clone();
    }

    if let Some(mission_raw_log_path) = mission_raw_log_path {
        client_config.mission_raw_log_path = mission_raw_log_path.clone();
    }

    if let Some(config_path) = config_path {
        confy::store_path(config_path, client_config).map_err(|e| ClientError::InputError(e.to_string()))?;
    } else {
        confy::store(APP_NAME, None, client_config).map_err(|e| ClientError::InputError(e.to_string()))?;
    }

    Ok(())
}

pub fn cli_print_config(client_config: ClientConfig) -> Result<(), ClientError> {
    println!("API endpoint: {}", client_config.api_endpoint);
    println!("Cookie path: {:?}", client_config.cookie_path);
    println!("Mission raw log path: {:?}", client_config.mission_raw_log_path);

    Ok(())
}