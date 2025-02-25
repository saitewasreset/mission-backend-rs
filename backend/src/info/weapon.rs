use std::collections::{HashMap, HashSet};

use crate::cache::mission::MissionCachedInfo;
use crate::{APIResponse, DbPool};
use actix_web::{
    get,
    web::{self, Data, Json},
};

use crate::db::models::*;
use crate::db::schema::*;
use common::{WEAPON_ORDER, WEAPON_TYPE};
use diesel::prelude::*;
use crate::cache::manager::get_db_redis_conn;

// character_game_id -> weapon_type(0, 1) -> Vec<(weapon_game_id, preference_index)>
type WeaponPreferenceResponse = HashMap<String, HashMap<i16, Vec<(String, f64)>>>;

fn generate(
    mission_cached_info_list: &[MissionCachedInfo],
    invalid_mission_id_list: &[i32],
    character_id_to_game_id: &HashMap<i16, String>,
    weapon_id_to_game_id: &HashMap<i16, String>,
) -> WeaponPreferenceResponse {
    let invalid_mission_id_set = invalid_mission_id_list
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let mission_cached_info_list = mission_cached_info_list
        .iter()
        .filter(|info| !invalid_mission_id_set.contains(&info.mission_info.id))
        .collect::<Vec<_>>();

    // character_id -> player_id -> weapon_id -> mission_set
    let mut character_weapon_mission_set: HashMap<i16, HashMap<i16, HashMap<i16, HashSet<i32>>>> =
        HashMap::new();

    for mission in mission_cached_info_list {
        for player_info in &mission.player_info {
            if let Some(player_damage_info) = mission.damage_info.get(&player_info.player_id) {
                for damage_pack in player_damage_info.values() {
                    character_weapon_mission_set
                        .entry(player_info.character_id)
                        .or_default()
                        .entry(player_info.player_id)
                        .or_default()
                        .entry(damage_pack.weapon_id)
                        .or_default()
                        .insert(mission.mission_info.id);
                }
            }
        }
    }

    // character_id -> player_id -> weapon_id -> preference_index
    let mut character_player_weapon_preference: HashMap<i16, HashMap<i16, HashMap<i16, f64>>> =
        HashMap::with_capacity(character_weapon_mission_set.len());

    for (&character_id, player_weapon_mission_set) in &character_weapon_mission_set {
        for (&player_id, weapon_mission_set) in player_weapon_mission_set {
            let total_count = weapon_mission_set
                .values()
                .map(|s| s.len() as i32)
                .sum::<i32>();

            for (&weapon_id, mission_set) in weapon_mission_set {
                let preference_index = mission_set.len() as f64 / total_count as f64;
                character_player_weapon_preference
                    .entry(character_id)
                    .or_default()
                    .entry(player_id)
                    .or_default()
                    .insert(weapon_id, preference_index);
            }
        }
    }

    // character_id -> weapon_id -> f64
    let mut character_weapon_preference: HashMap<i16, HashMap<i16, f64>> =
        HashMap::with_capacity(character_weapon_mission_set.len());

    for (character_id, player_weapon_preference) in character_player_weapon_preference {
        for weapon_preference in player_weapon_preference.values() {
            for (&weapon_id, &preference_index) in weapon_preference {
                *character_weapon_preference
                    .entry(character_id)
                    .or_default()
                    .entry(weapon_id)
                    .or_default() += preference_index;
            }
        }
    }

    let mut result: WeaponPreferenceResponse =
        HashMap::with_capacity(character_weapon_mission_set.len());

    for (character_id, weapon_preference) in character_weapon_preference {
        let character_game_id = character_id_to_game_id.get(&character_id).unwrap();
        for (weapon_id, preference_index) in weapon_preference {
            let current_weapon_game_id = weapon_id_to_game_id.get(&weapon_id).unwrap().clone();
            let current_weapon_type = match WEAPON_TYPE.get(current_weapon_game_id.as_str()) {
                Some(&x) => x,
                None => continue,
            };
            result
                .entry(character_game_id.clone())
                .or_default()
                .entry(current_weapon_type)
                .or_default()
                .push((current_weapon_game_id, preference_index));
        }
    }

    result
        .iter_mut()
        .flat_map(|(_, v)| v.iter_mut())
        .for_each(|(_, v)| {
            v.sort_unstable_by(|(a_weapon_game_id, _), (b_weapon_game_id, _)| {
                WEAPON_ORDER
                    .get(a_weapon_game_id.as_str())
                    .unwrap_or(&0)
                    .cmp(WEAPON_ORDER.get(b_weapon_game_id.as_str()).unwrap_or(&0))
            })
        });

    result
}

#[get("/weapon_preference")]
async fn get_weapon_preference(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
) -> Json<APIResponse<WeaponPreferenceResponse>> {
    let result = web::block(move || {
        let (mut db_conn, mut redis_conn) = get_db_redis_conn(&db_pool, &redis_client)
            .map_err(|e| format!("cannot get connection: {}", e))?;

        let invalid_mission_id_list: Vec<i32> = mission_invalid::table
            .select(mission_invalid::mission_id)
            .load(&mut db_conn)
            .map_err(|e| format!("cannot get invalid mission list from db: {}", e))?;

        let character_list = character::table
            .select(Character::as_select())
            .load(&mut db_conn)
            .map_err(|e| format!("cannot get character list: {}", e))?;

        let character_id_to_game_id = character_list
            .into_iter()
            .map(|character| (character.id, character.character_game_id))
            .collect::<HashMap<_, _>>();

        let weapon_list = weapon::table
            .select(Weapon::as_select())
            .load(&mut db_conn)
            .map_err(|e| format!("cannot get weapon list: {}", e))?;

        let weapon_id_to_game_id = weapon_list
            .into_iter()
            .map(|weapon| (weapon.id, weapon.weapon_game_id))
            .collect::<HashMap<_, _>>();

        let cached_mission_list = MissionCachedInfo::try_get_cached_all(&mut db_conn, &mut redis_conn)
            .map_err(|e| format!("cannot get cached mission info: {}", e))?;

        let result = generate(
            &cached_mission_list,
            &invalid_mission_id_list,
            &character_id_to_game_id,
            &weapon_id_to_game_id,
        );

        Ok::<_, String>(result)
    })
        .await
        .unwrap();

    Json(APIResponse::from_result(result, "cannot get weapon preference info"))
}
