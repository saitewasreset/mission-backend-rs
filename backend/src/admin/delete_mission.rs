use crate::db::schema::*;
use diesel::prelude::*;
use log::info;
use crate::DbConn;

pub fn delete_mission(db_conn: &mut DbConn, mission_id: i32) -> Result<(), String> {
    info!("deleting mission {}", mission_id);

    diesel::delete(damage_info::table.filter(damage_info::mission_id.eq(mission_id)))
        .execute(db_conn)
        .map_err(|e| {
            format!(
                "cannot delete damage_info for mission {}: {}",
                mission_id, e
            )
        })?;

    diesel::delete(kill_info::table.filter(kill_info::mission_id.eq(mission_id)))
        .execute(db_conn)
        .map_err(|e| {
            format!("cannot delete kill_info for mission {}: {}", mission_id, e)
        })?;

    diesel::delete(resource_info::table.filter(resource_info::mission_id.eq(mission_id)))
        .execute(db_conn)
        .map_err(|e| {
            format!(
                "cannot delete resource_info for mission {}: {}",
                mission_id, e
            )
        })?;

    diesel::delete(supply_info::table.filter(supply_info::mission_id.eq(mission_id)))
        .execute(db_conn)
        .map_err(|e| {
            format!(
                "cannot delete supply_info for mission {}: {}",
                mission_id, e
            )
        })?;
    diesel::delete(player_info::table.filter(player_info::mission_id.eq(mission_id)))
        .execute(db_conn)
        .map_err(|e| {
            format!(
                "cannot delete player_info for mission {}: {}",
                mission_id, e
            )
        })?;
    diesel::delete(mission::table.filter(mission::id.eq(mission_id)))
        .execute(db_conn)
        .map_err(|e| {
            format!("cannot delete mission {}: {}", mission_id, e)
        })?;

    Ok(())
}
