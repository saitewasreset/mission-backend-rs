use actix_web::{
    post,
    web::{self, Buf, Bytes, Data, Json},
    HttpRequest,
};

use crate::db::mission_log::*;
use crate::{db, DbPool};
use crate::{APIResponse, AppState};
use log::{error, info, warn};
use serde::Serialize;
use std::io::Read;
use std::time::{Duration, Instant};

#[derive(Serialize)]
pub struct LoadResult {
    pub load_count: i32,
    pub decode_time: String,
    pub load_time: String,
}

#[post("/load_mission")]
pub async fn load_mission(
    requests: HttpRequest,
    raw_body: Bytes,
    app_state: Data<AppState>,
    db_pool: Data<DbPool>,
) -> Json<APIResponse<LoadResult>> {
    if let Some(access_token) = app_state.access_token.clone() {
        if let Some(provieded_access_token) = requests.cookie("access_token") {
            if provieded_access_token.value() != access_token {
                return Json(APIResponse::unauthorized());
            }
        } else {
            return Json(APIResponse::unauthorized());
        }
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

                    return Json(APIResponse::ok(response_data));
                }
                Err(()) => {
                    return Json(APIResponse::internal_error());
                }
            }
        }
        Err(e) => {
            warn!("failed to decode the payload: {}", e);
            return Json(APIResponse::bad_request("failed to decode the payload"));
        }
    }
}

fn decompress_zstd_payload(data: Bytes) -> Result<(Duration, Vec<u8>), std::io::Error> {
    let begin = Instant::now();
    let mut decoder = zstd::Decoder::new(data.reader()).unwrap();
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

    Ok((begin.elapsed(), load_count))
}
