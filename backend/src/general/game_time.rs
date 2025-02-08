use crate::cache::mission::MissionCachedInfo;
use actix_web::{
    get,
    web::{self, Data, Json},
};
use chrono::{DateTime, Timelike};
use std::collections::HashMap;
use common::general::{GameTimeInfo, GAME_TIME_RESOLUTION_SEC, MISSION_TIME_RESOLUTION_SEC};
use crate::{APIResponse, DbPool};
use crate::cache::manager::get_db_redis_conn;

#[get("/game_time")]
async fn get_game_time(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
) -> Json<APIResponse<GameTimeInfo>> {
    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;

        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;


        let result = generate(&cached_mission_list);

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    Json(APIResponse::from_result(result, "cannot get game time info"))
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
