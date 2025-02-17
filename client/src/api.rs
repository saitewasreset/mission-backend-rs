use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::error::Error;
use std::fs::File;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;
use common::{APIResponse, Mapping};
use common::kpi::{APIAssignedKPI, APIDeleteAssignedKPI, KPIConfig};
use common::mission::APIMission;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use common::admin::{APIMissionInvalid, APISetMissionInvalid};
use common::cache::{APICacheStatus, APICacheType};
use crate::ClientError;

pub enum API {
    LoadMission,
    LoadMapping,
    LoadWatchList,
    LoadKPI,
    DeleteMission,
    APIMissionList,
    Login,
    CheckSession,
    UpdateCache,
    GetCacheStatus,
    SetMissionInvalid,
    GetMissionInvalid,
    GetAssignedKPI,
    SetAssignedKPI,
    DeleteAssignedKPI,
}

impl API {
    pub fn get_url(&self, api_endpoint: &str) -> String {
        match self {
            API::LoadMission => format!("{}/mission/load_mission", api_endpoint),
            API::LoadMapping => format!("{}/admin/load_mapping", api_endpoint),
            API::LoadWatchList => format!("{}/admin/load_watchlist", api_endpoint),
            API::LoadKPI => format!("{}/admin/load_kpi", api_endpoint),
            API::DeleteMission => format!("{}/admin/delete_mission", api_endpoint),
            API::APIMissionList => format!("{}/mission/api_mission_list", api_endpoint),
            API::Login => format!("{}/login", api_endpoint),
            API::CheckSession => format!("{}/check_session", api_endpoint),
            API::UpdateCache => format!("{}/cache/update_cache", api_endpoint),
            API::GetCacheStatus => format!("{}/cache/get_cache_status", api_endpoint),
            API::SetMissionInvalid => format!("{}/admin/set_mission_invalid", api_endpoint),
            API::GetMissionInvalid => format!("{}/admin/mission_invalid", api_endpoint),
            API::GetAssignedKPI => format!("{}/kpi/assigned_kpi", api_endpoint),
            API::SetAssignedKPI => format!("{}/kpi/set_assigned_kpi", api_endpoint),
            API::DeleteAssignedKPI => format!("{}/kpi/delete_assigned_kpi", api_endpoint),
        }
    }
}

pub enum APIResult<T: DeserializeOwned> {
    Success(T),
    APIError(i32, String),
    NetworkError(Box<dyn Error>),
}

impl<T> From<reqwest::Result<reqwest::blocking::Response>> for APIResult<T>
where
    T: Serialize + DeserializeOwned,
{
    fn from(response: reqwest::Result<reqwest::blocking::Response>) -> Self {
        match response {
            Ok(response) => {
                match response.bytes() {
                    Ok(bytes) => {
                        match serde_json::from_slice::<APIResponse<T>>(&bytes[..]) {
                            Ok(api_response) => {
                                if api_response.code == 200 {
                                    APIResult::Success(api_response.data.unwrap())
                                } else {
                                    APIResult::APIError(api_response.code, api_response.message)
                                }
                            }
                            Err(decode_error) => {
                                APIResult::NetworkError(Box::new(decode_error))
                            }
                        }
                    }
                    Err(ne) => APIResult::NetworkError(Box::new(ne)),
                }
            }
            Err(ne) => APIResult::NetworkError(Box::new(ne)),
        }
    }
}

pub struct NotAuthenticated;
pub struct Authenticated;

pub struct MissionMonitorClient<T> {
    client: Client,
    api_endpoint: String,
    cookie_provider: Arc<CookieStoreMutex>,
    _data: PhantomData<T>,
}

impl<T> MissionMonitorClient<T> {
    pub fn new(api_endpoint: String) -> MissionMonitorClient<NotAuthenticated> {
        MissionMonitorClient {
            client: Client::new(),
            cookie_provider: Arc::new(CookieStoreMutex::new(CookieStore::default())),
            api_endpoint,
            _data: PhantomData,
        }
    }
    fn get_url_for_api(&self, api: API) -> String {
        api.get_url(&self.api_endpoint)
    }

    fn get<Return>(&mut self, api: API) -> APIResult<Return>
    where
        Return: Serialize + DeserializeOwned,
    {
        self.client.get(self.get_url_for_api(api)).send().into()
    }

    fn post<Data, Return>(&mut self, api: API, data: Data) -> APIResult<Return>
    where
        Data: Serialize + DeserializeOwned,
        Return: Serialize + DeserializeOwned,
    {
        let serialized = serde_json::to_vec(&data).unwrap();

        let response = self
            .client
            .post(self.get_url_for_api(api))
            .body(serialized)
            .send();

        response.into()
    }

    pub fn get_api_mission_list(&mut self) -> APIResult<Vec<APIMission>> {
        self.get(API::APIMissionList)
    }

    pub fn get_assigned_kpi(&mut self) -> APIResult<Vec<APIAssignedKPI>> {
        self.get(API::GetAssignedKPI)
    }
}

impl MissionMonitorClient<NotAuthenticated> {
    pub fn login(mut self, access_token: String) -> Result<MissionMonitorClient<Authenticated>, (APIResult<()>, Self)> {
        let result: APIResult<()> = self.post(API::Login, access_token);

        match result {
            APIResult::Success(()) => {
                Ok(MissionMonitorClient {
                    client: self.client,
                    cookie_provider: self.cookie_provider,
                    api_endpoint: self.api_endpoint,
                    _data: PhantomData,
                })
            }
            x => {
                Err((x, self))
            }
        }
    }

    pub fn load_cookie(mut self, cookie_storage_content: &[u8]) -> Result<MissionMonitorClient<Authenticated>, (ClientError, Self)> {
        match cookie_store::serde::json::load(cookie_storage_content) {
            Ok(cookie_store) => {
                self.cookie_provider = Arc::new(CookieStoreMutex::new(cookie_store));

                self.client = Client::builder()
                    .cookie_provider(Arc::clone(&self.cookie_provider))
                    .build()
                    .unwrap();


                Ok(MissionMonitorClient {
                    client: self.client,
                    cookie_provider: self.cookie_provider,
                    api_endpoint: self.api_endpoint,
                    _data: PhantomData,
                })
            }
            Err(e) => {
                Err((ClientError::ParseError(format!("cannot parse stored cookie: {}", e)), self))
            }
        }
    }
}

impl MissionMonitorClient<Authenticated> {
    pub fn load_mapping(&mut self, mapping: Mapping) -> APIResult<()> {
        self.post(API::LoadMapping, mapping)
    }

    pub fn load_kpi(&mut self, kpi_config: KPIConfig) -> APIResult<()> {
        self.post(API::LoadKPI, kpi_config)
    }

    pub fn load_watchlist(&mut self, watchlist: Vec<String>) -> APIResult<()> {
        self.post(API::LoadWatchList, watchlist)
    }

    pub fn delete_mission(&mut self, mission_id_list: Vec<i32>) -> APIResult<()> {
        self.post(API::DeleteMission, mission_id_list)
    }

    pub fn load_mission(&mut self, payload: Vec<u8>) -> APIResult<()> {
        self.post(API::LoadMission, payload)
    }

    pub fn save_cookie(&self, cookie_path: impl AsRef<Path>) -> Result<(), ClientError> {
        let mut save_file = File::open(cookie_path).map_err(|e| ClientError::InputError(format!("cannot open cookie storage file: {}", e)))?;

        cookie_store::serde::json::save(&self.cookie_provider.lock().unwrap(), &mut save_file).map_err(|e| ClientError::InputError(format!("cannot save cookie storage: {}", e)))?;
        Ok(())
    }

    pub fn check_session(&mut self) -> APIResult<()> {
        self.get(API::CheckSession)
    }

    pub fn update_cache(&mut self, cache_type: APICacheType) -> APIResult<()> {
        self.post(API::UpdateCache, cache_type)
    }

    pub fn get_cache_status(&mut self) -> APIResult<APICacheStatus> {
        self.get(API::GetCacheStatus)
    }

    pub fn set_mission_invalid(&mut self, mission_invalid_data: APISetMissionInvalid) -> APIResult<()> {
        self.post(API::SetMissionInvalid, mission_invalid_data)
    }

    pub fn get_mission_invalid(&mut self) -> APIResult<Vec<APIMissionInvalid>> {
        self.get(API::GetMissionInvalid)
    }

    pub fn set_assigned_kpi(&mut self, assigned_kpi: APIAssignedKPI) -> APIResult<()> {
        self.post(API::SetAssignedKPI, assigned_kpi)
    }

    pub fn delete_assigned_kpi(&mut self, to_delete_assigned_kpi: APIDeleteAssignedKPI) -> APIResult<()> {
        self.post(API::DeleteAssignedKPI, to_delete_assigned_kpi)
    }
}