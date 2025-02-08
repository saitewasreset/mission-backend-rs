use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::error::Error;
use std::marker::PhantomData;
use std::sync::Arc;
use common::{APIResponse, Mapping};
use common::kpi::KPIConfig;
use common::mission::APIMission;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};

pub enum API {
    LoadMapping,
    LoadWatchList,
    LoadKPI,
    DeleteMission,
    APIMissionList,
    Login,
    CheckSession,
}

impl API {
    pub fn get_url(&self, api_endpoint: &str) -> String {
        match self {
            API::LoadMapping => format!("{}/admin/load_mapping", api_endpoint),
            API::LoadWatchList => format!("{}/admin/load_watchlist", api_endpoint),
            API::LoadKPI => format!("{}/admin/load_kpi", api_endpoint),
            API::DeleteMission => format!("{}/admin/delete_mission", api_endpoint),
            API::APIMissionList => format!("{}/mission/api_mission_list", api_endpoint),
            API::Login => format!("{}/login", api_endpoint),
            API::CheckSession => format!("{}/check_session", api_endpoint),
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
    _data: PhantomData<T>,
}

impl<T> MissionMonitorClient<T> {
    pub fn new(api_endpoint: String) -> MissionMonitorClient<NotAuthenticated> {
        MissionMonitorClient {
            client: Client::new(),
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
}

impl MissionMonitorClient<NotAuthenticated> {
    pub fn login(mut self, access_token: String) -> Result<MissionMonitorClient<Authenticated>, (APIResult<()>, Self)> {
        let result: APIResult<()> = self.post(API::Login, access_token);

        match result {
            APIResult::Success(()) => {
                Ok(MissionMonitorClient {
                    client: self.client,
                    api_endpoint: self.api_endpoint,
                    _data: PhantomData,
                })
            }
            x => {
                Err((x, self))
            }
        }
    }

    pub fn load_cookie(mut self, cookie_storage_content: &[u8]) -> Result<MissionMonitorClient<Authenticated>, (String, Self)> {
        match CookieStore::load_json(cookie_storage_content) {
            Ok(cookie_store) => {
                self.client = reqwest::blocking::Client::builder()
                    .cookie_provider(Arc::new(CookieStoreMutex::new(cookie_store)))
                    .build()
                    .unwrap();

                match self.get(API::CheckSession) {
                    APIResult::Success(()) => {
                        Ok(MissionMonitorClient {
                            client: self.client,
                            api_endpoint: self.api_endpoint,
                            _data: PhantomData,
                        })
                    }
                    APIResult::APIError(_, message) => {
                        Err((message, self))
                    }
                    APIResult::NetworkError(e) => {
                        Err((format!("network error: {}", e), self))
                    }
                }
            }
            Err(e) => {
                Err((format!("cannot parse stored cookie: {}", e), self))
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
}