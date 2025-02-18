use std::error::Error;
use std::path::PathBuf;
use clap::{Parser, Subcommand};
use clio::Input;
use mission_monitor_tools::{APP_NAME, APP_VERSION, APP_DESCRIPTION, cli_print_config, ClientConfig, cli_update_config, CliCacheType, cli_server_init, cli_login, cli_load_mission, cli_load_mapping, cli_load_kpi_config, cli_load_kpi_watchlist, cli_update_cache, cli_get_cache_status, cli_get_mission_list, cli_add_mission_invalid, cli_delete_mission_invalid, cli_get_mission_invalid, cli_set_assigned_kpi, cli_delete_assigned_kpi, cli_get_assigned_kpi};

#[derive(Parser)]
#[command(name = APP_NAME)]
#[command(version = APP_VERSION)]
#[command(about = APP_DESCRIPTION)]
struct Cli {
    /// Path to the configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Get or set the configuration
    Config {
        /// API endpoint, e.g. "http://127.0.0.1:8080/api"
        #[arg(short, long)]
        api_endpoint: Option<String>,

        /// Path to store the cookie file
        #[arg(short, long)]
        cookie_path: Option<PathBuf>,

        /// Path to the directory of mission raw log file
        #[arg(short, long)]
        mission_raw_log_path: Option<PathBuf>,
    },
    /// Initialize the server
    ServerInit,
    /// Login to the server
    Login {
        /// File to read access token from
        token_file: Option<Input>
    },
    /// Load mission data
    LoadMission,
    /// Load mapping data
    LoadMapping {
        /// Path to the directory of mapping files, default: "./mapping"
        mapping_directory: Option<PathBuf>
    },
    /// Load KPI config data
    LoadKPIConfig {
        /// Path to the directory of KPI config files, default: "./kpi_config"
        kpi_config_directory: Option<PathBuf>
    },
    /// Load watchlist data
    LoadWatchlist {
        /// Path to the watchlist file, default: "./watchlist.txt"
        watchlist_path: Option<PathBuf>
    },
    /// Update server cache
    UpdateCache {
        /// Cache type to update
        #[arg(value_enum)]
        cache_type: CliCacheType
    },
    /// Get server cache status
    CacheStatus,
    /// Get mission list
    MissionList {
        /// Only show the most recent n entries
        #[arg(short, long)]
        entry_limit: Option<usize>
    },
    /// Add invalid mark to selected mission
    AddMissionInvalid {
        /// Mission ID
        mission_id: i32,
        /// Reason for invalid
        reason: String,
    },
    /// Remove invalid mark from selected mission
    DeleteMissionInvalid {
        /// Mission ID
        mission_id: i32
    },
    /// Get invalid mark list
    GetMissionInvalid,
    /// Add assigned KPI to selected player in selected mission
    AddAssignedKPI,
    /// Remove assigned KPI from selected player in selected mission
    DeleteAssignedKPI {
        /// Mission ID
        mission_id: i32,
        /// Player name
        player_name: String,
    },
    /// Get assigned KPI list
    GetAssignedKPI {
        /// Mission ID
        #[arg(long, short)]
        mission_id: Option<i32>,

        /// Player name
        #[arg(long, short)]
        player_name: Option<String>,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let client_config: ClientConfig = if let Some(config_path) = &cli.config {
        confy::load_path(config_path)?
    } else {
        confy::load(APP_NAME, None)?
    };

    match cli.command {
        Commands::Config { api_endpoint, cookie_path, mission_raw_log_path } => {
            if api_endpoint.is_none() && cookie_path.is_none() && mission_raw_log_path.is_none() {
                cli_print_config(client_config)?
            } else {
                cli_update_config(client_config, cli.config, api_endpoint, cookie_path, mission_raw_log_path)?
            }
        }
        Commands::ServerInit => {
            cli_server_init(client_config)?
        }
        Commands::Login { token_file } => {
            cli_login(client_config, token_file)?
        }
        Commands::LoadMission => {
            cli_load_mission(client_config)?
        }
        Commands::LoadMapping { mapping_directory } => {
            cli_load_mapping(client_config, mapping_directory)?
        }
        Commands::LoadKPIConfig { kpi_config_directory } => {
            cli_load_kpi_config(client_config, kpi_config_directory)?
        }
        Commands::LoadWatchlist { watchlist_path } => {
            cli_load_kpi_watchlist(client_config, watchlist_path)?
        }
        Commands::UpdateCache { cache_type } => {
            cli_update_cache(client_config, cache_type.into())?
        }
        Commands::CacheStatus => {
            cli_get_cache_status(client_config)?
        }
        Commands::MissionList { entry_limit } => {
            cli_get_mission_list(client_config, entry_limit)?
        }
        Commands::AddMissionInvalid { mission_id, reason } => {
            cli_add_mission_invalid(client_config, mission_id, reason)?
        }
        Commands::DeleteMissionInvalid { mission_id } => {
            cli_delete_mission_invalid(client_config, mission_id)?
        }
        Commands::GetMissionInvalid => {
            cli_get_mission_invalid(client_config)?
        }
        Commands::AddAssignedKPI => {
            cli_set_assigned_kpi(client_config)?
        }
        Commands::DeleteAssignedKPI { mission_id, player_name } => {
            cli_delete_assigned_kpi(client_config, mission_id, player_name)?
        }
        Commands::GetAssignedKPI { mission_id, player_name } => {
            cli_get_assigned_kpi(client_config, mission_id, player_name)?
        }
    };

    Ok(())
}
