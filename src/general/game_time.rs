use crate::cache::mission::MissionCachedInfo;
use actix_web::{
    get,
    web::{self, Data, Json},
};
use chrono::{DateTime, Timelike};
use log::{debug, error};
use serde::Serialize;
use std::collections::HashMap;
use std::time::Instant;

const MISSION_TIME_RESOLUTION_SEC: u16 = 15;
const GAME_TIME_RESOLUTION_SEC: u32 = 60;

#[derive(Serialize)]
pub struct GameTimeInfo {
    #[serde(rename = "missionTimeResolution")]
    pub mission_time_resolution: u16,
    #[serde(rename = "gameTimeResolution")]
    pub game_time_resolution: u32,
    #[serde(rename = "missionTimeDistribution")]
    pub mission_time_distribution: HashMap<i16, i32>,
    #[serde(rename = "gameTimeDistribution")]
    pub game_time_distribution: HashMap<i32, i32>,
}
use crate::{APIResponse, AppState, DbPool};

#[get("/game_time")]
async fn get_game_time(
    app_state: Data<AppState>,
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
) -> Json<APIResponse<GameTimeInfo>> {
    let mapping = app_state.mapping.lock().unwrap();

    let entity_blacklist_set = mapping.entity_blacklist_set.clone();
    let entity_combine = mapping.entity_combine.clone();
    let weapon_combine = mapping.weapon_combine.clone();

    drop(mapping);
    let result = web::block(move || {
        let begin = Instant::now();

        let mut db_conn = match db_pool.get() {
            Ok(x) => x,
            Err(e) => {
                error!("cannot get db connection from pool: {}", e);
                return Err(());
            }
        };

        let mut redis_conn = match redis_client.get_connection() {
            Ok(x) => x,
            Err(e) => {
                error!("cannot get redis connection: {}", e);
                return Err(());
            }
        };

        let cached_mission_list = match MissionCachedInfo::get_cached_all(
            &mut db_conn,
            &mut redis_conn,
            &entity_blacklist_set,
            &entity_combine,
            &weapon_combine,
        ) {
            Ok(x) => x,
            Err(()) => {
                error!("cannot get cached mission list");
                return Err(());
            }
        };

        debug!("data prepared in {:?}", begin.elapsed());
        let begin = Instant::now();

        let result = generate(&cached_mission_list);

        debug!("game time info generated in {:?}", begin.elapsed());

        Ok(result)
    })
    .await
    .unwrap();

    match result {
        Ok(x) => Json(APIResponse::ok(x)),
        Err(()) => Json(APIResponse::internal_error()),
    }
}

fn generate(cached_mission_list: &[MissionCachedInfo]) -> GameTimeInfo {
    let mut mission_time_distribution: HashMap<i16, i32> =
        HashMap::with_capacity(60 * 60 / MISSION_TIME_RESOLUTION_SEC as usize);
    let mut game_time_distribution: HashMap<i32, i32> =
        HashMap::with_capacity(60 * 60 * 24 / GAME_TIME_RESOLUTION_SEC as usize);

    let time_info_list = cached_mission_list
        .iter()
        .map(|mission| {
            let mission_info = &mission.mission_info;

            (
                mission_info.begin_timestamp,
                mission_info.begin_timestamp + mission_info.mission_time as i64,
                mission_info.mission_time,
            )
        })
        .collect::<Vec<_>>();

    for (begin_timestamp, end_timestamp, mission_time) in time_info_list {
        let begin_datetime = DateTime::from_timestamp(begin_timestamp, 0).unwrap();
        let end_datetime = DateTime::from_timestamp(end_timestamp, 0).unwrap();

        let begin_from_midnight = begin_datetime.num_seconds_from_midnight();
        let end_from_midnight = end_datetime.num_seconds_from_midnight();

        for res in begin_from_midnight / GAME_TIME_RESOLUTION_SEC
            ..end_from_midnight / GAME_TIME_RESOLUTION_SEC
        {
            *(game_time_distribution.entry(res as i32).or_default()) += 1;
        }

        *(mission_time_distribution
            .entry(mission_time / MISSION_TIME_RESOLUTION_SEC as i16)
            .or_default()) += 1;
    }

    GameTimeInfo {
        mission_time_resolution: MISSION_TIME_RESOLUTION_SEC,
        game_time_resolution: GAME_TIME_RESOLUTION_SEC,
        mission_time_distribution,
        game_time_distribution,
    }
}
