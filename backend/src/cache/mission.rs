use std::borrow::Borrow;
use common::kpi::PlayerAssignedKPIInfo;
use common::damage::{DamagePack, KillPack, SupplyPack, WeaponPack};
use crate::db::models::*;
use crate::db::schema::*;
use crate::kpi::{apply_weight_table, friendly_fire_index};
use common::kpi::{
    CharacterKPIType, KPIComponent, KPIConfig,
};
use crate::DbConn;
use common::{FLOAT_EPSILON, NITRA_GAME_ID};
use diesel::prelude::*;
use diesel::RunQueryDsl;
use redis::Commands;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::hash::Hash;
use std::ops::{Add, AddAssign};
use std::time::{Duration, Instant};
use diesel::associations::{BelongsTo, HasTable};
use log::error;
use crate::cache::manager::{get_from_redis, CacheContext, CacheError, CacheGenerationError, Cacheable};
// 用于缓存输出任务详情及计算任务KPI、玩家KPI、赋分信息等需要的任务信息
// depends on:
// - mapping: entity_blacklist, entity_combine, weapon_combine

#[derive(Default, Debug, Copy, Clone, Hash)]
pub struct CacheTimeInfo {
    pub count: usize,
    pub load_from_db: Option<Duration>,
    pub generate: Duration,
}

impl Display for CacheTimeInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "count: {}, total: {:?} = {:?}(load_from_db) + {:?}(generate)",
            self.count,
            self.load_from_db.unwrap_or_default() + self.generate,
            self.load_from_db.unwrap_or_default(),
            self.generate
        )
    }
}

impl CacheTimeInfo {
    pub fn from_duration_load_from_db(duration: Duration) -> Self {
        CacheTimeInfo {
            count: 1,
            load_from_db: Some(duration),
            generate: Duration::default(),
        }
    }

    pub fn from_duration_generate(duration: Duration) -> Self {
        CacheTimeInfo {
            count: 1,
            load_from_db: None,
            generate: duration,
        }
    }

    pub fn add_load_from_db(&mut self, duration: Duration) {
        self.load_from_db = Some(self.load_from_db.unwrap_or_default() + duration);
    }

    pub fn add_generate(&mut self, duration: Duration) {
        self.generate += duration;
    }

    pub fn count(mut self, count: usize) -> Self {
        self.count = count;

        self
    }
}

#[derive(Serialize, Deserialize)]
pub struct MissionCachedInfo {
    pub mission_info: Mission,
    pub player_info: Vec<PlayerInfo>,
    // player_id -> index
    pub player_index: HashMap<i16, f64>,
    // player_id -> info
    pub kill_info: HashMap<i16, HashMap<String, KillPack>>,
    // player_id -> info
    pub damage_info: HashMap<i16, HashMap<String, DamagePack>>,
    pub weapon_damage_info: HashMap<String, WeaponPack>,
    // player_id -> resource_game_id -> total_amount
    pub resource_info: HashMap<i16, HashMap<String, f64>>,
    // player_id -> count
    pub revive_count: HashMap<i16, i16>,
    // player_id -> count
    pub death_count: HashMap<i16, i16>,
    // player_id -> info
    pub supply_info: HashMap<i16, Vec<SupplyPack>>,
}

fn combine_player_info<IK, OK, V, F, O>(origin_map: HashMap<OK, HashMap<IK, V>>, key_func: F) -> HashMap<IK, O>
where
    IK: Eq + Hash,
    OK: Eq + Hash,
    F: Fn(V) -> O,
    O: Add + AddAssign + Default,
{
    let mut result = HashMap::new();

    for (s, val) in origin_map.into_iter().flat_map(|(_, v)| v.into_iter()) {
        *result.entry(s).or_default() += key_func(val);
    }

    result
}

fn map_inner_value<IK, OK, V, F, O>(origin_map: HashMap<OK, HashMap<IK, V>>, key_func: F) -> HashMap<OK, HashMap<IK, O>>
where
    IK: Eq + Hash,
    OK: Eq + Hash,
    F: Fn(V) -> Option<O>,
    O: Add + AddAssign + Default,
{
    let mut result = HashMap::with_capacity(origin_map.len());

    for (k, v) in origin_map {
        let inner_map = v.into_iter()
            .flat_map(|(k, v)| {
                let new_val = key_func(v);

                new_val.map(|x| (k, x))
            })
            .collect::<HashMap<_, _>>();
        result.insert(k, inner_map);
    }

    result
}

fn clone_inner_key<OK, IK, V>(origin_map: HashMap<OK, HashMap<&IK, V>>) -> HashMap<OK, HashMap<IK, V>>
where
    OK: Clone + Eq + Hash,
    IK: Clone + Eq + Hash,
{
    origin_map
        .into_iter()
        .map(|(k, v)| {
            let inner_map = v
                .into_iter()
                .map(|(k, v)| (k.clone(), v))
                .collect::<HashMap<_, _>>();
            (k, inner_map)
        })
        .collect::<HashMap<_, _>>()
}

fn db_group_by_mission<'a, Child>(parent: &'a Vec<Mission>, children: Vec<Child>) -> HashMap<i32, Vec<Child>>
where
    Child: BelongsTo<Mission>,
    <&'a Mission as Identifiable>::Id: Borrow<Child::ForeignKey>,
{
    children
        .grouped_by(parent)
        .into_iter()
        .zip(parent)
        .map(|(children, parent)| (parent.id, children))
        .collect::<HashMap<_, _>>()
}

impl MissionCachedInfo {
    pub fn combine_kill_info(origin: HashMap<i16, HashMap<String, KillPack>>) -> HashMap<String, f64> {
        combine_player_info(origin, |kill_pack| kill_pack.total_amount as f64)
    }

    pub fn combine_damage_info(origin: HashMap<i16, HashMap<String, DamagePack>>) -> HashMap<String, f64> {
        combine_player_info(origin, |damage_pack| {
            if damage_pack.taker_type == 1 {
                0.0
            } else {
                damage_pack.total_amount
            }
        })
    }

    pub fn combine_resource_info(origin: HashMap<i16, HashMap<String, f64>>) -> HashMap<String, f64> {
        combine_player_info(origin, |x| x)
    }
}

pub fn cache_write_redis(data: impl Serialize, key: &str, redis_conn: &mut redis::Connection) -> Result<(), String> {
    let serialized = rmp_serde::to_vec(&data).map_err(|e| format!("cannot serialize data: {}", e))?;
    redis_conn.set::<_, _, ()>(key, serialized).map_err(|e| format!("cannot write data to redis: {}", e))?;

    Ok(())
}

struct MissionRawInfo {
    mission: Mission,
    player_info_list: Vec<PlayerInfo>,
    raw_kill_info_list: Vec<KillInfo>,
    raw_damage_info_list: Vec<DamageInfo>,
    raw_resource_info_list: Vec<ResourceInfo>,
    raw_supply_info_list: Vec<SupplyInfo>,
}

struct IDMapping {
    id_to_player_name: HashMap<i16, String>,
    id_to_entity_game_id: HashMap<i16, String>,
    id_to_weapon_game_id: HashMap<i16, String>,
    id_to_resource_game_id: HashMap<i16, String>,
}

impl IDMapping {
    fn load_from_db(conn: &mut DbConn) -> Result<IDMapping, String> {
        let player_list: Vec<Player> = player::table.load(conn).map_err(|e| format!("cannot load player from db: {}", e))?;

        let entity_list: Vec<Entity> = entity::table.load(conn).map_err(|e| format!("cannot load entity from db: {}", e))?;

        let resource_list: Vec<Resource> = resource::table.load(conn).map_err(|e| format!("cannot load resource from db: {}", e))?;

        let weapon_list: Vec<Weapon> = weapon::table.load(conn).map_err(|e| format!("cannot load weapon from db: {}", e))?;

        let id_to_player_name = player_list
            .into_iter()
            .map(|player| (player.id, player.player_name))
            .collect::<HashMap<_, _>>();

        let id_to_entity_game_id = entity_list
            .into_iter()
            .map(|entity| (entity.id, entity.entity_game_id))
            .collect::<HashMap<_, _>>();

        let id_to_resource_game_id = resource_list
            .into_iter()
            .map(|resource| (resource.id, resource.resource_game_id))
            .collect::<HashMap<_, _>>();

        let id_to_weapon_game_id = weapon_list
            .into_iter()
            .map(|weapon| (weapon.id, weapon.weapon_game_id))
            .collect::<HashMap<_, _>>();

        Ok(IDMapping {
            id_to_player_name,
            id_to_entity_game_id,
            id_to_weapon_game_id,
            id_to_resource_game_id,
        })
    }
}

impl MissionCachedInfo {
    fn generate(
        mission_raw_info: MissionRawInfo,
        entity_blacklist_set: &HashSet<String>,
        entity_combine: &HashMap<String, String>,
        weapon_combine: &HashMap<String, String>,
        id_mapping: &IDMapping,
    ) -> (Self, CacheTimeInfo) {
        let begin = Instant::now();

        let mission_info = mission_raw_info.mission;
        let player_info_list = mission_raw_info.player_info_list;
        let raw_kill_info_list = mission_raw_info.raw_kill_info_list;
        let raw_damage_info_list = mission_raw_info.raw_damage_info_list;
        let raw_resource_info_list = mission_raw_info.raw_resource_info_list;
        let raw_supply_info_list = mission_raw_info.raw_supply_info_list;

        let id_to_player_name = &id_mapping.id_to_player_name;
        let id_to_entity_game_id = &id_mapping.id_to_entity_game_id;
        let id_to_weapon_game_id = &id_mapping.id_to_weapon_game_id;
        let id_to_resource_game_id = &id_mapping.id_to_resource_game_id;

        let mut player_index = HashMap::with_capacity(player_info_list.len());
        let mut revive_count = HashMap::with_capacity(player_info_list.len());
        let mut death_count = HashMap::with_capacity(player_info_list.len());

        for current_player_info in &player_info_list {
            player_index.insert(
                current_player_info.player_id,
                current_player_info.present_time as f64 / mission_info.mission_time as f64,
            );
            revive_count.insert(
                current_player_info.player_id,
                current_player_info.revive_num,
            );
            death_count.insert(current_player_info.player_id, current_player_info.death_num);
        }

        let mut kill_info = HashMap::with_capacity(player_info_list.len());

        for current_kill_info in raw_kill_info_list {
            let record_entity_game_id = id_to_entity_game_id
                .get(&current_kill_info.entity_id)
                .unwrap();

            let killed_entity_game_id = entity_combine
                .get(record_entity_game_id)
                .unwrap_or(record_entity_game_id);

            if entity_blacklist_set.contains(killed_entity_game_id) {
                continue;
            }

            let player_kill_map = kill_info
                .entry(current_kill_info.player_id)
                .or_insert(HashMap::new());

            let entity_kill_entry =
                player_kill_map
                    .entry(killed_entity_game_id)
                    .or_insert(KillPack {
                        taker_id: current_kill_info.entity_id,
                        taker_name: killed_entity_game_id.clone(),
                        total_amount: 0,
                    });

            entity_kill_entry.total_amount += 1;
        }

        let weapon_game_id_to_id = id_to_weapon_game_id
            .iter()
            .map(|(&k, v)| (v, k))
            .collect::<HashMap<_, _>>();

        let mut damage_info = HashMap::with_capacity(player_info_list.len());

        let mut weapon_details = HashMap::new();

        for current_damage_info in raw_damage_info_list {
            // 0→unknown 1→ player 2→enemy
            if current_damage_info.causer_type != 1 {
                continue;
            }

            let (taker_game_id, taker_type) = match current_damage_info.taker_type {
                1 => (
                    id_to_player_name
                        .get(&current_damage_info.taker_id)
                        .unwrap(),
                    1,
                ),
                x => {
                    let record_entity_game_id = id_to_entity_game_id
                        .get(&current_damage_info.taker_id)
                        .unwrap();

                    let entity_game_id = entity_combine
                        .get(record_entity_game_id)
                        .unwrap_or(record_entity_game_id);

                    if entity_blacklist_set.contains(entity_game_id) {
                        continue;
                    }

                    (entity_game_id, x)
                }
            };

            let player_damage_map = damage_info
                .entry(current_damage_info.causer_id)
                .or_insert(HashMap::new());

            let player_damage_entry =
                player_damage_map
                    .entry(taker_game_id)
                    .or_insert(DamagePack {
                        taker_id: current_damage_info.taker_id,
                        taker_type,
                        weapon_id: current_damage_info.weapon_id,
                        total_amount: 0.0,
                    });
            player_damage_entry.total_amount += current_damage_info.damage;

            let record_weapon_game_id = id_to_weapon_game_id
                .get(&current_damage_info.weapon_id)
                .unwrap();

            let weapon_game_id = weapon_combine
                .get(record_weapon_game_id)
                .unwrap_or(record_weapon_game_id);

            let detail_map = weapon_details
                .entry(weapon_game_id)
                .or_insert(HashMap::new());

            let detail_entry = detail_map.entry(taker_game_id).or_insert(DamagePack {
                taker_id: current_damage_info.taker_id,
                taker_type,
                weapon_id: current_damage_info.weapon_id,
                total_amount: 0.0,
            });

            detail_entry.total_amount += current_damage_info.damage;
        }

        let mut resource_info = HashMap::with_capacity(player_info_list.len());

        for current_resource_info in raw_resource_info_list {
            let resource_game_id = id_to_resource_game_id
                .get(&current_resource_info.resource_id)
                .unwrap();

            let player_resource_info_map = resource_info
                .entry(current_resource_info.player_id)
                .or_insert(HashMap::new());

            let resource_entry = player_resource_info_map
                .entry(resource_game_id)
                .or_insert(0.0);

            *resource_entry += current_resource_info.amount;
        }

        let mut supply_info = HashMap::with_capacity(player_info_list.len());

        for current_supply_info in raw_supply_info_list {
            let player_supply_list = supply_info
                .entry(current_supply_info.player_id)
                .or_insert(Vec::new());

            player_supply_list.push(SupplyPack {
                ammo: current_supply_info.ammo,
                health: current_supply_info.health,
            })
        }

        let weapon_damage_info = weapon_details
            .into_iter()
            .map(|(weapon_game_id, detail)| {
                let weapon_id = weapon_game_id_to_id.get(weapon_game_id).unwrap();
                let total_damage = detail
                    .values()
                    .map(|v| v.total_amount)
                    .sum::<f64>();
                let detail_map = detail
                    .into_iter()
                    .map(|(k, v)| (k.clone(), v))
                    .collect::<HashMap<_, _>>();

                (
                    weapon_game_id.clone(),
                    WeaponPack {
                        weapon_id: *weapon_id,
                        total_amount: total_damage,
                        detail: detail_map,
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        // Convert inner HashMap<&String, _> to HashMap<String, _>
        let kill_info = clone_inner_key(kill_info);

        let damage_info = clone_inner_key(damage_info);

        let weapon_damage_info = weapon_damage_info
            .into_iter()
            .map(|(k, v)| (k.clone(), v))
            .collect::<HashMap<_, _>>();

        let resource_info = clone_inner_key(resource_info);

        let elapsed = begin.elapsed();

        (
            MissionCachedInfo {
                mission_info: mission_info.clone(),
                player_info: player_info_list.clone(),
                player_index,
                kill_info,
                damage_info,
                weapon_damage_info,
                resource_info,
                revive_count,
                death_count,
                supply_info,
            },
            CacheTimeInfo {
                count: 1,
                load_from_db: None,
                generate: elapsed,
            },
        )
    }

    pub fn from_db(
        conn: &mut DbConn,
        entity_blacklist_set: &HashSet<String>,
        entity_combine: &HashMap<String, String>,
        weapon_combine: &HashMap<String, String>,
        mission_id: i32,
    ) -> Result<(Self, CacheTimeInfo), String> {
        let begin = Instant::now();

        let id_mapping = IDMapping::load_from_db(conn)?;

        let mission_info: Mission = mission::table
            .filter(mission::id.eq(mission_id))
            .get_result(conn).map_err(|e| format!("cannot load mission_id = {} from db: {}", mission_id, e))?;

        let player_info: Vec<PlayerInfo> = PlayerInfo::belonging_to(&mission_info).load(conn).map_err(|e| format!(
            "cannot load player info for mission_id = {} from db: {}", mission_id, e
        ))?;


        let damage_info: Vec<DamageInfo> = DamageInfo::belonging_to(&mission_info).load(conn).map_err(|e| format!(
            "cannot load damage info for mission_id = {} from db: {}", mission_id, e
        ))?;


        let kill_info: Vec<KillInfo> = KillInfo::belonging_to(&mission_info).load(conn).map_err(|e| format!(
            "cannot load kill info for mission_id = {} from db: {}", mission_id, e
        ))?;

        let resource_info: Vec<ResourceInfo> =
            ResourceInfo::belonging_to(&mission_info).load(conn).map_err(|e| format!(
                "cannot load resource info for mission_id = {} from db: {}", mission_id, e
            ))?;

        let supply_info: Vec<SupplyInfo> = SupplyInfo::belonging_to(&mission_info).load(conn).map_err(|e| format!(
            "cannot load supply info for mission_id = {} from db: {}", mission_id, e
        ))?;


        let mission_raw_info = MissionRawInfo {
            mission: mission_info,
            player_info_list: player_info,
            raw_kill_info_list: kill_info,
            raw_damage_info_list: damage_info,
            raw_resource_info_list: resource_info,
            raw_supply_info_list: supply_info,
        };

        let load_from_db_elapsed = begin.elapsed();

        let (result, generate_elapsed) = Self::generate(
            mission_raw_info,
            entity_blacklist_set,
            entity_combine,
            weapon_combine,
            &id_mapping,
        );

        Ok(
            (result,
             CacheTimeInfo {
                 count: 1,
                 load_from_db: Some(load_from_db_elapsed),
                 generate: generate_elapsed.generate,
             })
        )
    }

    pub fn from_db_all(
        conn: &mut DbConn,
        entity_blacklist_set: &HashSet<String>,
        entity_combine: &HashMap<String, String>,
        weapon_combine: &HashMap<String, String>,
    ) -> Result<(Vec<Self>, CacheTimeInfo), String> {
        let begin = Instant::now();

        let id_mapping = IDMapping::load_from_db(conn)?;

        let all_mission_info = mission::table.select(Mission::as_select()).load(conn).map_err(|e| format!("cannot load missions from db: {}", e))?;

        let all_player_info: Vec<PlayerInfo> =
            PlayerInfo::belonging_to(&all_mission_info).load(conn).map_err(|e| format!("cannot load player info from db: {}", e))?;

        let all_damage_info: Vec<DamageInfo> =
            DamageInfo::belonging_to(&all_mission_info).load(conn).map_err(|e| format!("cannot load damage info from db: {}", e))?;

        let all_kill_info: Vec<KillInfo> =
            KillInfo::belonging_to(&all_mission_info).load(conn).map_err(|e| format!("cannot load kill info from db: {}", e))?;

        let all_resource_info: Vec<ResourceInfo> =
            ResourceInfo::belonging_to(&all_mission_info).load(conn).map_err(|e| format!("cannot load resource info from db: {}", e))?;

        let all_supply_info: Vec<SupplyInfo> =
            SupplyInfo::belonging_to(&all_mission_info).load(conn).map_err(|e| format!("cannot load supply info from db: {}", e))?;

        let load_from_db_elapsed = begin.elapsed();
        let begin = Instant::now();

        let player_info_by_mission = db_group_by_mission(&all_mission_info, all_player_info);

        let damage_info_by_mission = db_group_by_mission(&all_mission_info, all_damage_info);

        let kill_info_by_mission = db_group_by_mission(&all_mission_info, all_kill_info);

        let resource_info_by_mission = db_group_by_mission(&all_mission_info, all_resource_info);

        let supply_info_by_mission = db_group_by_mission(&all_mission_info, all_supply_info);


        let mut mission_info_list = Vec::with_capacity(all_mission_info.len());

        for mission in all_mission_info {
            let mission_id = mission.id;
            mission_info_list.push(MissionRawInfo {
                mission,
                player_info_list: player_info_by_mission.get(&mission_id).unwrap().clone(),
                raw_kill_info_list: kill_info_by_mission.get(&mission_id).unwrap().clone(),
                raw_damage_info_list: damage_info_by_mission.get(&mission_id).unwrap().clone(),
                raw_resource_info_list: resource_info_by_mission.get(&mission_id).unwrap().clone(),
                raw_supply_info_list: supply_info_by_mission.get(&mission_id).unwrap().clone(),
            })
        }

        let result = mission_info_list
            .into_iter()
            .map(|mission_raw_info| {
                Self::generate(
                    mission_raw_info,
                    entity_blacklist_set,
                    entity_combine,
                    weapon_combine,
                    &id_mapping,
                )
                    .0
            })
            .collect::<Vec<_>>();

        let generate_elapsed = begin.elapsed();

        let count = result.len();

        Ok((result, CacheTimeInfo {
            count,
            load_from_db: Some(load_from_db_elapsed),
            generate: generate_elapsed,
        }))
    }

    pub fn try_get_cached(
        redis_conn: &mut redis::Connection,
        mission_id: i32,
    ) -> Result<Self, CacheError> {
        let redis_key = format!("mission_raw:{}", mission_id);

        get_from_redis(redis_conn, &redis_key)
    }

    pub fn try_get_cached_all(
        db_conn: &mut DbConn,
        redis_conn: &mut redis::Connection,
    ) -> Result<Vec<Self>, CacheError> {
        let mission_list = mission::table
            .select(Mission::as_select())
            .load(db_conn)
            .map_err(|e| CacheError::InternalError(format!("cannot get mission list from db: {}", e)))?;

        let mut result = Vec::with_capacity(mission_list.len());

        for mission in mission_list {
            let redis_key = format!("mission_raw:{}", mission.id);

            result.push(get_from_redis(redis_conn, &redis_key)?);
        }

        Ok(result)
    }
}

impl Cacheable for MissionCachedInfo {
    fn name(&self) -> &str {
        "mission_raw"
    }
    fn generate_and_write(context: &CacheContext) -> Result<CacheTimeInfo, CacheGenerationError> {
        let begin = Instant::now();

        let entity_blacklist_set = &context.mapping.entity_blacklist_set;
        let entity_combine = &context.mapping.entity_combine;
        let weapon_combine = &context.mapping.weapon_combine;

        let (mut db_conn, mut redis_conn) = crate::cache::manager::get_db_redis_conn(
            &context.db_pool, &context.redis_client)?;

        let load_from_db_duration = begin.elapsed();

        let (cache_result, mut time_info) = MissionCachedInfo::from_db_all(
            &mut db_conn,
            entity_blacklist_set,
            entity_combine,
            weapon_combine,
        ).map_err(|e| CacheGenerationError::InternalError(format!("cannot update mission raw cache: {}", e)))?;

        for cached_info in cache_result {
            let redis_key = format!("mission_raw:{}", cached_info.mission_info.id);
            cache_write_redis(&cached_info, &redis_key, &mut redis_conn).map_err(CacheGenerationError::InternalError)?;
        }

        let _ = redis::cmd("SAVE").exec(&mut redis_conn);

        time_info.add_load_from_db(load_from_db_duration);

        Ok(time_info)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct PlayerRawKPIData {
    pub source_value: f64,
    pub weighted_value: f64,
    pub mission_total_weighted_value: f64,
    pub raw_index: f64,
}

#[derive(Serialize, Deserialize)]

// depends on:
// - MissionCachedInfo
// - KPIConfig
// - mapping: scout_special_player
pub struct MissionKPICachedInfo {
    pub mission_id: i32,
    pub damage_map: HashMap<i16, HashMap<String, f64>>,
    pub kill_map: HashMap<i16, HashMap<String, f64>>,
    pub resource_map: HashMap<i16, HashMap<String, f64>>,
    pub total_damage_map: HashMap<String, f64>,
    pub total_kill_map: HashMap<String, f64>,
    pub total_resource_map: HashMap<String, f64>,
    pub player_id_to_kpi_character: HashMap<i16, CharacterKPIType>,
    pub raw_kpi_data: HashMap<i16, HashMap<KPIComponent, PlayerRawKPIData>>,
    // player_id -> PlayerAssignedKPIInfo
    pub assigned_kpi_info: HashMap<i16, PlayerAssignedKPIInfo>,
}

impl MissionKPICachedInfo {
    fn generate(
        mission_info: &MissionCachedInfo,
        mission_assigned_kpi_info: impl AsRef<[AssignedKPI]>,
        character_id_to_game_id: &HashMap<i16, String>,
        player_id_to_name: &HashMap<i16, String>,
        scout_special_player_set: &HashSet<String>,
        kpi_config: &KPIConfig,
    ) -> (Self, CacheTimeInfo) {
        let begin = Instant::now();

        let damage_map = map_inner_value(mission_info.damage_info.clone(), |damage_pack| {
            if damage_pack.taker_type == 1 {
                None
            } else {
                Some(damage_pack.total_amount)
            }
        });

        let kill_map = map_inner_value(mission_info.kill_info.clone(), |kill_pack| Some(kill_pack.total_amount as f64));

        let resource_map = map_inner_value(mission_info.resource_info.clone(), Some);

        let total_damage_map = MissionCachedInfo::combine_damage_info(mission_info.damage_info.clone());
        let total_kill_map = MissionCachedInfo::combine_kill_info(mission_info.kill_info.clone());
        let total_resource_map = MissionCachedInfo::combine_resource_info(mission_info.resource_info.clone());

        let total_weighted_resource_map =
            apply_weight_table(&total_resource_map, &kpi_config.resource_weight_table);

        let mut player_id_to_kpi_character = HashMap::with_capacity(mission_info.player_info.len());

        let total_revive_count = mission_info
            .player_info
            .iter()
            .map(|player_info| player_info.revive_num)
            .sum::<i16>() as f64;

        let total_death_count = mission_info
            .player_info
            .iter()
            .map(|player_info| player_info.death_num)
            .sum::<i16>() as f64;

        let total_supply_count = mission_info
            .supply_info
            .values()
            .map(|supply_list| supply_list.len())
            .sum::<usize>() as f64;

        let mut raw_kpi_data = HashMap::new();

        for player_info in &mission_info.player_info {
            let player_name = player_id_to_name.get(&player_info.player_id).unwrap();
            let player_character_game_id = character_id_to_game_id
                .get(&player_info.character_id)
                .unwrap();

            let player_character_kpi_type = CharacterKPIType::from_player(
                player_character_game_id,
                player_name,
                scout_special_player_set,
            );

            player_id_to_kpi_character.insert(player_info.player_id, player_character_kpi_type);

            let character_weight_table = kpi_config
                .character_weight_table
                .get(&player_character_kpi_type)
                .map_or(HashMap::new(), |x| x.clone());
            // Kill

            let source_kill = kill_map
                .get(&player_info.player_id)
                .unwrap_or(&HashMap::new())
                .values()
                .sum::<f64>();

            let weighted_kill_map = apply_weight_table(
                kill_map
                    .get(&player_info.player_id)
                    .unwrap_or(&HashMap::new()),
                &character_weight_table,
            );

            let weighted_kill = weighted_kill_map.values().sum::<f64>();
            let mission_total_weighted_kill =
                apply_weight_table(&total_kill_map, &character_weight_table)
                    .values()
                    .sum::<f64>();

            // Damage

            let source_damage = damage_map
                .get(&player_info.player_id)
                .unwrap_or(&HashMap::new())
                .values()
                .sum::<f64>();

            let weighted_damage_map = apply_weight_table(
                damage_map
                    .get(&player_info.player_id)
                    .unwrap_or(&HashMap::new()),
                &character_weight_table,
            );

            let weighted_damage = weighted_damage_map.values().sum::<f64>();
            let mission_total_weighted_damage =
                apply_weight_table(&total_damage_map, &character_weight_table)
                    .values()
                    .sum::<f64>();

            // Priority
            let priority_map = apply_weight_table(
                damage_map
                    .get(&player_info.player_id)
                    .unwrap_or(&HashMap::new()),
                &kpi_config.priority_table,
            );

            let priority_damage = priority_map.values().sum::<f64>();
            let mission_total_priority_damage =
                apply_weight_table(&total_damage_map, &kpi_config.priority_table)
                    .values()
                    .sum::<f64>();

            // Revive

            let player_revive_count = player_info.revive_num as f64;

            // Death

            let player_death_count = player_info.death_num as f64;

            // FriendlyFire

            let player_friendly_fire = mission_info
                .damage_info
                .get(&player_info.player_id)
                .unwrap_or(&HashMap::new())
                .iter()
                .filter(|(_, pack)| pack.taker_type == 1 && pack.taker_id != player_info.player_id)
                .map(|(_, pack)| pack.total_amount)
                .sum::<f64>();

            let player_overall_damage = source_damage + player_friendly_fire;

            let player_ff_index = match player_overall_damage {
                0.0..FLOAT_EPSILON => 1.0,
                _ => friendly_fire_index(player_friendly_fire / player_overall_damage),
            };

            // Nitra

            let player_nitra = *resource_map
                .get(&player_info.player_id)
                .unwrap_or(&HashMap::new())
                .get(NITRA_GAME_ID)
                .unwrap_or(&0.0);

            let total_nitra = *total_resource_map.get(NITRA_GAME_ID).unwrap_or(&0.0);

            // Minerals

            let player_source_minerals = resource_map
                .get(&player_info.player_id)
                .unwrap_or(&HashMap::new())
                .values()
                .sum::<f64>();

            let player_weighted_minerals = apply_weight_table(
                resource_map
                    .get(&player_info.player_id)
                    .unwrap_or(&HashMap::new()),
                &kpi_config.resource_weight_table,
            )
                .values()
                .sum::<f64>();

            let total_weighted_minerals = total_weighted_resource_map.values().sum::<f64>();

            // Supply

            let player_supply_count = mission_info
                .supply_info
                .get(&player_info.player_id)
                .unwrap_or(&Vec::new())
                .len() as f64;

            let mut player_raw_kpi_data = HashMap::new();

            player_raw_kpi_data.insert(
                KPIComponent::Kill,
                PlayerRawKPIData {
                    source_value: source_kill,
                    weighted_value: weighted_kill,
                    mission_total_weighted_value: mission_total_weighted_kill,
                    raw_index: match mission_total_weighted_kill {
                        0.0..FLOAT_EPSILON => 0.0,
                        _ => weighted_kill / mission_total_weighted_kill,
                    },
                },
            );

            player_raw_kpi_data.insert(
                KPIComponent::Damage,
                PlayerRawKPIData {
                    source_value: source_damage,
                    weighted_value: weighted_damage,
                    mission_total_weighted_value: mission_total_weighted_damage,
                    raw_index: match mission_total_weighted_damage {
                        0.0..FLOAT_EPSILON => 0.0,
                        _ => weighted_damage / mission_total_weighted_damage,
                    },
                },
            );

            player_raw_kpi_data.insert(
                KPIComponent::Priority,
                PlayerRawKPIData {
                    source_value: source_damage,
                    weighted_value: priority_damage,
                    mission_total_weighted_value: mission_total_priority_damage,
                    raw_index: match mission_total_priority_damage {
                        0.0..FLOAT_EPSILON => 0.0,
                        _ => priority_damage / mission_total_priority_damage,
                    },
                },
            );

            player_raw_kpi_data.insert(
                KPIComponent::Revive,
                PlayerRawKPIData {
                    source_value: player_revive_count,
                    weighted_value: player_revive_count,
                    mission_total_weighted_value: total_revive_count,
                    raw_index: match total_revive_count {
                        0.0..FLOAT_EPSILON => 1.0,
                        _ => player_revive_count / total_revive_count,
                    },
                },
            );

            player_raw_kpi_data.insert(
                KPIComponent::Death,
                PlayerRawKPIData {
                    source_value: player_death_count,
                    weighted_value: player_death_count,
                    mission_total_weighted_value: total_death_count,
                    raw_index: match total_death_count {
                        0.0..FLOAT_EPSILON => 0.0,
                        _ => -player_death_count / total_death_count,
                    },
                },
            );

            player_raw_kpi_data.insert(
                KPIComponent::FriendlyFire,
                PlayerRawKPIData {
                    source_value: player_friendly_fire,
                    weighted_value: player_ff_index,
                    mission_total_weighted_value: 0.0,
                    raw_index: player_ff_index,
                },
            );

            player_raw_kpi_data.insert(
                KPIComponent::Nitra,
                PlayerRawKPIData {
                    source_value: player_nitra,
                    weighted_value: player_nitra,
                    mission_total_weighted_value: total_nitra,
                    raw_index: match total_nitra {
                        0.0..FLOAT_EPSILON => 0.0,
                        _ => player_nitra / total_nitra,
                    },
                },
            );

            player_raw_kpi_data.insert(
                KPIComponent::Minerals,
                PlayerRawKPIData {
                    source_value: player_source_minerals,
                    weighted_value: player_weighted_minerals,
                    mission_total_weighted_value: total_weighted_minerals,
                    raw_index: match total_weighted_minerals {
                        0.0..FLOAT_EPSILON => 0.0,
                        _ => player_weighted_minerals / total_weighted_minerals,
                    },
                },
            );

            player_raw_kpi_data.insert(
                KPIComponent::Supply,
                PlayerRawKPIData {
                    source_value: player_supply_count,
                    weighted_value: player_supply_count,
                    mission_total_weighted_value: total_supply_count,
                    raw_index: match total_supply_count {
                        0.0..FLOAT_EPSILON => 0.0,
                        _ => -player_supply_count / total_supply_count,
                    },
                },
            );

            raw_kpi_data.insert(player_info.player_id, player_raw_kpi_data);
        }

        let mut assigned_kpi_info_by_player_id = HashMap::new();

        for assigned_info in mission_assigned_kpi_info.as_ref() {
            let entry = assigned_kpi_info_by_player_id.entry(assigned_info.player_id).or_insert(PlayerAssignedKPIInfo {
                by_component: HashMap::new(),
                overall: None,
                note: assigned_info.note.clone().unwrap_or_default(),
            });

            if assigned_info.kpi_component_delta_value != 0.0 {
                let target_kpi_component = if let Ok(x) = (assigned_info.target_kpi_component as usize).try_into() {
                    x
                } else {
                    error!("invalid target_kpi_component: {}", assigned_info.target_kpi_component);
                    KPIComponent::Kill
                };
                entry.by_component.insert(target_kpi_component, assigned_info.kpi_component_delta_value);
            }

            if assigned_info.total_delta_value != 0.0 {
                entry.overall = Some(assigned_info.total_delta_value);
            }
        }

        let result = MissionKPICachedInfo {
            mission_id: mission_info.mission_info.id,
            damage_map,
            kill_map,
            resource_map: resource_map.clone(),
            total_damage_map,
            total_kill_map,
            total_resource_map,
            player_id_to_kpi_character,
            raw_kpi_data,
            assigned_kpi_info: assigned_kpi_info_by_player_id,
        };

        let elapsed = begin.elapsed();

        (result, CacheTimeInfo {
            count: 1,
            load_from_db: None,
            generate: elapsed,
        })
    }

    pub fn from_redis_all(
        db_conn: &mut DbConn,
        redis_conn: &mut redis::Connection,
        character_id_to_game_id: &HashMap<i16, String>,
        player_id_to_name: &HashMap<i16, String>,
        scout_special_player_set: &HashSet<String>,
        kpi_config: &KPIConfig,
    ) -> Result<(Vec<Self>, CacheTimeInfo), CacheError> {
        let begin = Instant::now();

        let mission_list = MissionCachedInfo::try_get_cached_all(db_conn, redis_conn)?;

        let all_assigned_kpi_info: Vec<AssignedKPI> = AssignedKPI::table()
            .load(db_conn)
            .map_err(|e| CacheError::InternalError(format!("cannot load assigned kpi info from db: {}", e)))?;

        let load_from_redis_elapsed = begin.elapsed();
        let begin = Instant::now();

        let mut assigned_kpi_info_by_mission: HashMap<i32, Vec<AssignedKPI>> = HashMap::new();

        for assigned_kpi in all_assigned_kpi_info {
            assigned_kpi_info_by_mission
                .entry(assigned_kpi.mission_id)
                .or_default()
                .push(assigned_kpi);
        }

        let mut result = Vec::with_capacity(mission_list.len());

        for mission_info in &mission_list {
            let generated = Self::generate(
                mission_info,
                assigned_kpi_info_by_mission.get(&mission_info.mission_info.id).unwrap_or(&Vec::new()),
                character_id_to_game_id,
                player_id_to_name,
                scout_special_player_set,
                kpi_config,
            )
                .0;
            result.push(generated);
        }

        let generate_elapsed = begin.elapsed();

        let count = result.len();

        Ok((result, CacheTimeInfo {
            count,
            load_from_db: Some(load_from_redis_elapsed),
            generate: generate_elapsed,
        }))
    }

    pub fn try_get_cached(
        redis_conn: &mut redis::Connection,
        mission_id: i32,
    ) -> Result<Self, CacheError> {
        let redis_key = format!("mission_kpi_raw:{}", mission_id);

        get_from_redis(redis_conn, &redis_key)
    }

    pub fn try_get_cached_all(
        db_conn: &mut DbConn,
        redis_conn: &mut redis::Connection,
    ) -> Result<Vec<Self>, CacheError> {
        let mission_list = MissionCachedInfo::try_get_cached_all(db_conn, redis_conn)?;

        let mut result = Vec::with_capacity(mission_list.len());

        for mission_info in &mission_list {
            let mission_id = mission_info.mission_info.id;

            let cached_content = Self::try_get_cached(redis_conn, mission_id)?;
            result.push(cached_content);
        }

        Ok(result)
    }
}

impl Cacheable for MissionKPICachedInfo {
    fn name(&self) -> &str {
        "mission_kpi_raw"
    }
    fn generate_and_write(context: &CacheContext) -> Result<CacheTimeInfo, CacheGenerationError> {
        let begin = Instant::now();

        let (mut db_conn, mut redis_conn) = crate::cache::manager::get_db_redis_conn(&context.db_pool, &context.redis_client)?;

        let character_list = character::table
            .select(Character::as_select())
            .load(&mut db_conn)
            .map_err(|e| CacheGenerationError::InternalError(format!("cannot get character list from db: {}", e)))?;

        let character_id_to_game_id = character_list
            .into_iter()
            .map(|character| (character.id, character.character_game_id))
            .collect::<HashMap<_, _>>();

        let player_list = player::table
            .select(Player::as_select())
            .load(&mut db_conn)
            .map_err(|e| CacheGenerationError::InternalError(format!("cannot get player list from db: {}", e)))?;

        let player_id_to_game_id = player_list
            .into_iter()
            .map(|player| (player.id, player.player_name))
            .collect::<HashMap<_, _>>();

        let scout_special_player_set = &context.mapping.scout_special_player_set;

        let kpi_config = context.kpi_config.as_ref()
            .ok_or(CacheGenerationError::ConfigError("kpi config".to_string()))?;

        let load_from_db = begin.elapsed();

        let (cache_result, mut time_info) = MissionKPICachedInfo::from_redis_all(
            &mut db_conn,
            &mut redis_conn,
            &character_id_to_game_id,
            &player_id_to_game_id,
            scout_special_player_set,
            kpi_config,
        ).map_err(|e| CacheGenerationError::InternalError(format!("cannot update mission kpi cache: {}", e)))?;

        for cached_info in cache_result {
            let redis_key = format!("mission_kpi_raw:{}", cached_info.mission_id);
            cache_write_redis(&cached_info, &redis_key, &mut redis_conn).map_err(CacheGenerationError::InternalError)?;
        }

        let _ = redis::cmd("SAVE").exec(&mut redis_conn);

        time_info.add_load_from_db(load_from_db);

        Ok(time_info)
    }
}