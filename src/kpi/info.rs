use super::APIWeightTableData;
use crate::cache::kpi::CachedGlobalKPIState;
use crate::kpi::CharacterKPIType;
use crate::kpi::IndexTransformRange;
use crate::{APIResponse, DbPool};
use actix_web::{
    get,
    web::{self, Data, Json},
};
use serde::Serialize;
use std::collections::HashMap;
use log::error;
use crate::cache::manager::{get_db_redis_conn, CacheManager};

#[derive(Serialize)]
pub struct GammaInnerInfo {
    #[serde(rename = "playerIndex")]
    pub player_index: f64,
    pub value: f64,
    pub ratio: f64,
}

#[get("/gamma")]
async fn get_gamma_info(
    redis_client: Data<redis::Client>,
    db_pool: Data<DbPool>,
) -> Json<APIResponse<HashMap<String, HashMap<String, GammaInnerInfo>>>> {
    match get_db_redis_conn(&db_pool, &redis_client) {
        Ok((_, mut redis_conn)) => {
            let result = web::block(move || {
                let x = CachedGlobalKPIState::try_get_cached(&mut redis_conn).map_err(|e| e.to_string())?;

                let mut result: HashMap<String, HashMap<String, GammaInnerInfo>> = HashMap::new();
                for (character_kpi_type, character_component) in x.character_correction_factor {
                    for (kpi_component, character_data) in character_component {
                        result
                            .entry(kpi_component.to_string())
                            .or_default()
                            .entry(character_kpi_type.to_string())
                            .or_insert(GammaInnerInfo {
                                player_index: character_data.player_index,
                                value: character_data.value,
                                ratio: character_data.correction_factor,
                            });
                    }
                }

                Ok::<_, String>(result)
            })
                .await
                .unwrap();

            Json(APIResponse::from_result(result, "cannot get global kpi state"))
        }
        Err(e) => {
            error!("cannot get db connection: {}", e);
            Json(APIResponse::internal_error())
        }
    }
}

#[get("/transform_range_info")]
async fn get_transform_range_info(
    db_pool: Data<DbPool>,
    redis_client: Data<redis::Client>,
) -> Json<APIResponse<HashMap<String, HashMap<String, Vec<IndexTransformRange>>>>> {
    match get_db_redis_conn(&db_pool, &redis_client) {
        Ok((_, mut redis_conn)) => {
            let result = web::block(move || {
                let x = CachedGlobalKPIState::try_get_cached(&mut redis_conn).map_err(|e| e.to_string())?;

                let r = x.transform_range
                    .iter()
                    .map(|(character_kpi_type, character_info)| {
                        (
                            character_kpi_type.to_string(),
                            character_info
                                .iter()
                                .map(|(character_id, info)| (character_id.to_string(), info.clone()))
                                .collect(),
                        )
                    })
                    .collect::<HashMap<_, _>>();

                Ok::<_, String>(r)
            }).await.unwrap();

            Json(APIResponse::from_result(result, "cannot get global kpi state"))
        }

        Err(e) => {
            error!("cannot get db connection: {}", e);
            Json(APIResponse::internal_error())
        }
    }
}

#[get("/weight_table")]
async fn get_weight_table(cache_manager: Data<CacheManager>) -> Json<APIResponse<Vec<APIWeightTableData>>> {
    let entity_game_id_to_name = cache_manager.get_mapping().entity_mapping;

    if let Some(kpi_config) = cache_manager.get_kpi_config() {
        let mut result = Vec::new();

        for entity_game_id in entity_game_id_to_name.keys() {
            let priority = *kpi_config
                .priority_table
                .get(entity_game_id)
                .unwrap_or(&0.0);

            let driller = *kpi_config
                .character_weight_table
                .get(&CharacterKPIType::Driller)
                .unwrap_or(&HashMap::new())
                .get(entity_game_id)
                .unwrap_or(&1.0);

            let gunner = *kpi_config
                .character_weight_table
                .get(&CharacterKPIType::Gunner)
                .unwrap_or(&HashMap::new())
                .get(entity_game_id)
                .unwrap_or(&1.0);

            let engineer = *kpi_config
                .character_weight_table
                .get(&CharacterKPIType::Engineer)
                .unwrap_or(&HashMap::new())
                .get(entity_game_id)
                .unwrap_or(&1.0);

            let scout = *kpi_config
                .character_weight_table
                .get(&CharacterKPIType::Scout)
                .unwrap_or(&HashMap::new())
                .get(entity_game_id)
                .unwrap_or(&1.0);

            let scout_special = *kpi_config
                .character_weight_table
                .get(&CharacterKPIType::ScoutSpecial)
                .unwrap_or(&HashMap::new())
                .get(entity_game_id)
                .unwrap_or(&1.0);

            result.push(APIWeightTableData {
                entity_game_id: entity_game_id.clone(),
                priority,
                driller,
                gunner,
                engineer,
                scout,
                scout_special,
            });
        }

        Json(APIResponse::ok(result))
    } else {
        Json(APIResponse::config_required("kpi_config"))
    }
}
