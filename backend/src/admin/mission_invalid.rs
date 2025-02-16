use diesel::Insertable;
use diesel::prelude::*;
use common::admin::APIMissionInvalid;
use crate::db::models::MissionInvalid;
use crate::db::schema::mission_invalid;
use crate::DbConn;


#[derive(Insertable)]
#[diesel(table_name = mission_invalid)]
struct NewMissionInvalid {
    pub mission_id: i32,
    pub reason: String,
}

pub fn check_invalid_record_exist(db_conn: &mut DbConn, target_mission_id: i32) -> Result<bool, String> {
    use crate::db::schema::mission_invalid::dsl::*;

    let invalid_record = mission_invalid
        .select(MissionInvalid::as_select())
        .filter(mission_id.eq(target_mission_id))
        .first(db_conn)
        .optional()
        .map_err(|e| format!("cannot query mission_invalid: {}", e))?;

    Ok(invalid_record.is_some())
}

pub fn add_mission_invalid(db_conn: &mut DbConn, mission_id: i32, reason: String) -> Result<(), String> {
    let new_mission_invalid = NewMissionInvalid {
        mission_id,
        reason,
    };

    diesel::insert_into(mission_invalid::table)
        .values(&new_mission_invalid)
        .execute(db_conn)
        .map_err(|e| format!("cannot insert mission_invalid: {}", e))?;

    Ok(())
}

pub fn delete_mission_invalid(db_conn: &mut DbConn, target_mission_id: i32) -> Result<(), String> {
    use crate::db::schema::mission_invalid::dsl::*;

    diesel::delete(mission_invalid.filter(mission_id.eq(target_mission_id)))
        .execute(db_conn)
        .map_err(|e| format!("cannot delete mission_invalid: {}", e))?;

    Ok(())
}

pub fn get_mission_invalid(db_conn: &mut DbConn) -> Result<Vec<APIMissionInvalid>, String> {
    use crate::db::schema::mission_invalid::dsl::*;

    Ok(mission_invalid
        .select(MissionInvalid::as_select())
        .load(db_conn)
        .map_err(|e| format!("cannot query mission_invalid: {}", e))?
        .into_iter()
        .map(|db_mission_invalid| {
            APIMissionInvalid {
                mission_id: db_mission_invalid.mission_id,
                reason: db_mission_invalid.reason,
            }
        })
        .collect())
}