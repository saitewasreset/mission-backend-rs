use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use log::{error, info};
use redis::Commands;
use serde::de::DeserializeOwned;
use crate::{DbConn, DbPool, Mapping};
use crate::cache::kpi::CachedGlobalKPIState;
use crate::cache::mission::{CacheTimeInfo, MissionCachedInfo, MissionKPICachedInfo};
use crate::kpi::KPIConfig;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CacheType {
    MissionRaw,
    MissionKPIRaw,
    GlobalKPIState,
}

impl Display for CacheType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheType::MissionRaw => write!(f, "MissionRaw"),
            CacheType::MissionKPIRaw => write!(f, "MissionKPIRaw"),
            CacheType::GlobalKPIState => write!(f, "GlobalKPIState"),
        }
    }
}

impl CacheType {
    pub fn update_cache(&self, context: &CacheContext) -> Result<CacheTimeInfo, CacheGenerationError> {
        match self {
            CacheType::MissionRaw => MissionCachedInfo::generate_and_write(context),
            CacheType::MissionKPIRaw => MissionKPICachedInfo::generate_and_write(context),
            CacheType::GlobalKPIState => CachedGlobalKPIState::generate_and_write(context),
        }
    }
}

pub fn get_db_redis_conn(db_pool: &DbPool, redis_client: &redis::Client) -> Result<(DbConn, redis::Connection), CacheGenerationError> {
    let db_conn = db_pool
        .get()
        .map_err(|e| CacheGenerationError::InternalError(format!("cannot get db connection from pool: {}", e)))?;
    let redis_conn = redis_client
        .get_connection()
        .map_err(|e| CacheGenerationError::InternalError(format!("cannot get redis connection: {}", e)))?;

    Ok((db_conn, redis_conn))
}

pub struct CacheContext {
    pub mapping: Mapping,
    pub kpi_config: Option<KPIConfig>,
    pub db_pool: DbPool,
    pub redis_client: redis::Client,
}

pub trait Cacheable {
    fn name(&self) -> &str;
    fn generate_and_write(context: &CacheContext) -> Result<CacheTimeInfo, CacheGenerationError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CacheGenerationError {
    InternalError(String),
    ConfigError(String),
}

impl Display for CacheGenerationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheGenerationError::InternalError(msg) => write!(f, "internal error: {}", msg),
            CacheGenerationError::ConfigError(msg) => write!(f, "config error: {}", msg),
        }
    }
}

impl Error for CacheGenerationError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CacheError {
    NoData,
    MalformedData(String),
    InternalError(String),
}

impl Display for CacheError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::NoData => write!(f, "no data"),
            CacheError::MalformedData(msg) => write!(f, "malformed data: {}", msg),
            CacheError::InternalError(msg) => write!(f, "internal error: {}", msg),
        }
    }
}

impl Error for CacheError {}

pub type CacheStatusMap = HashMap<CacheType, (i64, Result<CacheTimeInfo, CacheGenerationError>)>;

pub struct CacheManager {
    working: Arc<AtomicBool>,
    cache_status: Arc<Mutex<CacheStatusMap>>,
    cache_context: Arc<Mutex<CacheContext>>,
    job_tx: std::sync::mpsc::SyncSender<CacheType>,
}

impl CacheManager {
    pub fn new(cache_context: CacheContext) -> Self {
        let (tx, rx) = std::sync::mpsc::sync_channel(8);

        let result = CacheManager {
            working: Arc::new(AtomicBool::new(false)),
            cache_status: Arc::new(Mutex::new(HashMap::new())),
            cache_context: Arc::new(Mutex::new(cache_context)),
            job_tx: tx,
        };

        let thread_working = Arc::clone(&result.working);
        let thread_cache_status = Arc::clone(&result.cache_status);
        let thread_cache_context = Arc::clone(&result.cache_context);

        std::thread::Builder::new().name("cache manager thread".to_string()).spawn(move || {
            while let Ok(cache_type) = rx.recv() {
                thread_working.store(true, std::sync::atomic::Ordering::Relaxed);
                info!("updating cache: {:?}", cache_type);
                let context = thread_cache_context.lock().unwrap();

                let result = cache_type.update_cache(&context);

                match &result {
                    Ok(time_info) => {
                        info!("cache updated: {:?} in {}", cache_type, time_info);
                    }
                    Err(e) => {
                        error!("cannot update cache {:?}: {}", cache_type, e);
                    }
                }

                let mut cache_status = thread_cache_status.lock().unwrap();

                cache_status.insert(cache_type, (chrono::Utc::now().timestamp(), result));
                thread_working.store(false, std::sync::atomic::Ordering::Relaxed);
            }
        }).unwrap();

        result
    }

    pub fn get_cache_status(&self, cache_type: &CacheType) -> Option<(i64, Result<CacheTimeInfo, CacheGenerationError>)> {
        let cache_status = self.cache_status.lock().unwrap();
        cache_status.get(cache_type).cloned()
    }

    pub fn get_cache_status_all(&self) -> CacheStatusMap {
        self.cache_status.lock().unwrap().clone()
    }

    pub fn update_mapping(&self, mapping: Mapping) {
        self.cache_context.lock().unwrap().mapping = mapping;
    }

    pub fn update_kpi_config(&self, kpi_config: KPIConfig) {
        self.cache_context.lock().unwrap().kpi_config = Some(kpi_config);
    }

    pub fn try_schedule(&self, cache_type: CacheType) -> Result<bool, ()> {
        match self.job_tx.try_send(cache_type) {
            Ok(_) => Ok(true),
            Err(e) => {
                match e {
                    std::sync::mpsc::TrySendError::Full(_) => Ok(false),
                    std::sync::mpsc::TrySendError::Disconnected(_) => Err(()),
                }
            }
        }
    }

    pub fn try_schedule_all(&self) -> Result<bool, ()> {
        let cache_type_list = [CacheType::MissionRaw, CacheType::MissionKPIRaw, CacheType::GlobalKPIState];

        for cache_type in cache_type_list {
            if self.try_schedule(cache_type)? == false {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub fn get_mapping(&self) -> Mapping {
        self.cache_context.lock().unwrap().mapping.clone()
    }

    pub fn get_kpi_config(&self) -> Option<KPIConfig> {
        self.cache_context.lock().unwrap().kpi_config.clone()
    }

    pub fn is_working(&self) -> bool {
        self.working.load(std::sync::atomic::Ordering::Relaxed)
    }
}

pub fn get_from_redis<T: DeserializeOwned>(redis_conn: &mut redis::Connection, redis_key: &str) -> Result<T, CacheError> {
    redis_conn.exists::<_, ()>(redis_key)
        .map_err(|_| CacheError::NoData)?;

    let data: Vec<u8> = redis_conn.get(redis_key)
        .map_err(|e| CacheError::InternalError(e.to_string()))?;

    rmp_serde::from_read(&data[..])
        .map_err(|e| CacheError::MalformedData(format!("cannot deserialize data: {}", e)))
}