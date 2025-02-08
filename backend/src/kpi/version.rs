use common::{APIResponse, KPI_VERSION};
use actix_web::{get, web::Json};
use common::kpi::KPIVersionInfo;

#[get("/version")]
async fn get_kpi_version() -> Json<APIResponse<KPIVersionInfo>> {
    Json(APIResponse::ok(KPIVersionInfo {
        version: KPI_VERSION.to_string(),
    }))
}
