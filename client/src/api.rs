use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::error::Error;
use std::marker::PhantomData;
use common::{APIResponse, Mapping};
use common::kpi::KPIConfig;

pub enum API {
    LoadMapping,
    LoadWatchList,
    LoadKPI,
    DeleteMission,
}

impl API {
    pub fn get_url(&self, api_endpoint: &str) -> String {
        match self {
            API::LoadMapping => format!("{}/admin/load_mapping", api_endpoint),
            API::LoadWatchList => format!("{}/admin/load_watchlist", api_endpoint),
            API::LoadKPI => format!("{}/admin/load_kpi", api_endpoint),
            API::DeleteMission => format!("{}/admin/delete_mission", api_endpoint),
        }
    }
}

pub enum APIResult<T: DeserializeOwned> {
    Success(T),
    APIError(i32, String),
    NetworkError(Box<dyn Error>),
}

impl<T, NE> From<Result<APIResponse<T>, NE>> for APIResult<T>
where
    T: Serialize + DeserializeOwned,
    NE: Error + 'static,
{
    fn from(result: Result<APIResponse<T>, NE>) -> Self {
        match result {
            Ok(APIResponse { code, message, data }) => {
                if code == 200 {
                    APIResult::Success(data.unwrap())
                } else {
                    APIResult::APIError(code, message)
                }
            }
            Err(e) => APIResult::NetworkError(Box::new(e)),
        }
    }
}

struct NotAuthenticated;
struct Authenticated;

pub struct MissionMonitorClient<T> {
    client: Client,
    api_endpoint: String,
    _data: PhantomData<T>,
}

impl<T> MissionMonitorClient<T> {
    fn get_url_for_api(&self, api: API) -> String {
        api.get_url(&self.api_endpoint)
    }

    fn post<Data, Return>(&mut self, api: API, data: Data) -> APIResult<Return>
    where
        Data: Serialize + DeserializeOwned,
        Return: Serialize + DeserializeOwned,
    {
        let serialized = serde_json::to_vec(&data).unwrap();

        let response = self
            .client
            .post(&self.get_url_for_api(api))
            .body(serialized)
            .send();

        match response {
            Ok(response) => {
                match response.bytes() {
                    Ok(bytes) => {
                        match serde_json::from_slice::<APIResponse<Return>>(&bytes[..]) {
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

impl MissionMonitorClient<NotAuthenticated> {}

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