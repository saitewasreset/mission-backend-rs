use actix_web::{
    post,
    web::{self, Buf, Bytes, Data, Json},
    HttpRequest,
};

use crate::db::{models::*, schema::*};
use crate::{db, DbPool};
use crate::{APIResponse, AppState};
use diesel::prelude::*;
use log::{error, info, warn};
use std::time::{Duration, Instant};
use std::{collections::HashMap, io::Read};
use common::INVALID_MISSION_TIME_THRESHOLD;
use common::mission::LoadResult;
use common::mission_log::LogContent;

#[post("/load_mission")]
pub async fn load_mission(
    requests: HttpRequest,
    raw_body: Bytes,
    app_state: Data<AppState>,
    db_pool: Data<DbPool>,
) -> Json<APIResponse<LoadResult>> {
    if !app_state.check_access_token(&requests) {
        return Json(APIResponse::unauthorized());
    }

    let decode_result = web::block(|| decompress_zstd_payload(raw_body))
        .await
        .unwrap();

    let (decode_time, decompressed) = match decode_result {
        Ok(x) => x,
        Err(e) => {
            warn!("failed to decompress the payload: {}", e);
            return Json(APIResponse::bad_request("failed to decompress the payload"));
        }
    };

    match rmp_serde::from_read::<_, Vec<LogContent>>(&decompressed[..]) {
        Ok(mission_list) => {
            match web::block(|| load_mission_db(db_pool, mission_list))
                .await
                .unwrap()
            {
                Ok((load_time, load_count)) => {
                    let response_data = LoadResult {
                        load_count,
                        load_time: format!("{:?}", load_time),
                        decode_time: format!("{:?}", decode_time),
                    };

                    Json(APIResponse::ok(response_data))
                }
                Err(()) => {
                    Json(APIResponse::internal_error())
                }
            }
        }
        Err(e) => {
            warn!("failed to decode the payload: {}", e);
            Json(APIResponse::bad_request("failed to decode the payload"))
        }
    }
}

fn decompress_zstd_payload(data: Bytes) -> Result<(Duration, Vec<u8>), std::io::Error> {
    let begin = Instant::now();
    let mut decoder = zstd::Decoder::new(data.reader())?;
    let mut decompressed = Vec::new();

    let decode_result = decoder.read_to_end(&mut decompressed);

    match decode_result {
        Ok(_) => Ok((begin.elapsed(), decompressed)),
        Err(e) => Err(e),
    }
}

fn load_mission_db(
    db_pool: Data<DbPool>,
    log_list: Vec<LogContent>,
) -> Result<(Duration, i32), ()> {
    let begin = Instant::now();
    let mut conn = match db_pool.get() {
        Ok(conn) => conn,
        Err(e) => {
            error!("cannot get db connection from pool: {}", e);
            return Err(());
        }
    };

    let load_count = log_list.len() as i32;

    for log in log_list {
        let current_mission_timestamp = log.mission_info.begin_timestamp;
        info!("loading mission: {}", current_mission_timestamp);
        if let Err(e) = db::mission::load_mission(log, &mut conn) {
            error!(
                "db error while loading mission {}: {}",
                current_mission_timestamp, e
            );
            return Err(());
        }
    }

    mark_invalid_mission(db_pool)?;

    Ok((begin.elapsed(), load_count))
}

fn mark_invalid_mission(db_pool: Data<DbPool>) -> Result<(), ()> {
    let mut conn = match db_pool.get() {
        Ok(conn) => conn,
        Err(e) => {
            error!("cannot get db connection from pool: {}", e);
            return Err(());
        }
    };

    let all_mission = match mission::table.select(Mission::as_select()).load(&mut conn) {
        Ok(x) => x,
        Err(e) => {
            error!("cannot get mission list: {}", e);
            return Err(());
        }
    };

    let mission_player_info = match player_info::table
        .select(PlayerInfo::as_select())
        .load(&mut conn)
    {
        Ok(x) => x,
        Err(e) => {
            error!("cannot get player info list: {}", e);
            return Err(());
        }
    };

    let player_info_by_mission = mission_player_info
        .grouped_by(&all_mission)
        .into_iter()
        .zip(all_mission)
        .map(|(player_info_list, mission)| ((mission.id, mission.mission_time), player_info_list))
        .collect::<HashMap<_, _>>();

    let mut invalid_mission_id_to_reason: HashMap<i32, &str> = HashMap::new();

    for ((mission_id, mission_time), player_list) in player_info_by_mission {
        if mission_time < INVALID_MISSION_TIME_THRESHOLD {
            invalid_mission_id_to_reason.insert(mission_id, "任务时间过短");
            continue;
        }

        if player_list.len() <= 1 {
            invalid_mission_id_to_reason.insert(mission_id, "单人游戏");
            continue;
        }
    }

    for (mission_id, reason) in invalid_mission_id_to_reason {
        if let Err(e) = diesel::insert_into(mission_invalid::table)
            .values((
                mission_invalid::mission_id.eq(mission_id),
                mission_invalid::reason.eq(reason),
            ))
            .on_conflict(mission_invalid::mission_id)
            .do_update()
            .set(mission_invalid::reason.eq(reason))
            .execute(&mut conn)
        {
            error!("cannot insert into invalid mission: {}", e);
            return Err(());
        }
    }

    Ok(())
}
