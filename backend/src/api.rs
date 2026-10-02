use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;

use crate::{
    domain::{Match, MatchEvent, MatchLineups, Scorer},
    service::{DataService, ServiceError, TableReport},
};

pub fn routes() -> Router<DataService> {
    Router::new()
        .route("/api/matches", get(matches))
        .route("/api/table", get(table))
        .route("/api/scorers", get(scorers))
        .route("/api/matches/{id}/lineups", get(lineups))
        .route("/api/matches/{id}/events", get(events))
}

#[derive(Serialize)]
struct MatchesBody {
    matches: Vec<Match>,
}

#[derive(Serialize)]
struct ScorersBody {
    scorers: Vec<Scorer>,
}

#[derive(Serialize)]
struct EventsBody {
    events: Vec<MatchEvent>,
}

async fn matches(State(service): State<DataService>) -> Result<Json<MatchesBody>, ApiError> {
    let matches = service.matches().await?;
    Ok(Json(MatchesBody { matches }))
}

async fn table(State(service): State<DataService>) -> Result<Json<TableReport>, ApiError> {
    Ok(Json(service.table().await?))
}

async fn scorers(State(service): State<DataService>) -> Result<Json<ScorersBody>, ApiError> {
    let scorers = service.scorers().await?;
    Ok(Json(ScorersBody { scorers }))
}

async fn lineups(
    State(service): State<DataService>,
    Path(id): Path<u64>,
) -> Result<Json<MatchLineups>, ApiError> {
    Ok(Json(service.lineups(id).await?))
}

async fn events(
    State(service): State<DataService>,
    Path(id): Path<u64>,
) -> Result<Json<EventsBody>, ApiError> {
    let events = service.events(id).await?;
    Ok(Json(EventsBody { events }))
}

pub struct ApiError(ServiceError);

impl From<ServiceError> for ApiError {
    fn from(error: ServiceError) -> Self {
        Self(error)
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    missing_variables: Vec<&'static str>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = status_for(&self.0);
        if status == StatusCode::BAD_GATEWAY {
            tracing::warn!("upstream error: {}", self.0);
        }
        let missing_variables = match &self.0 {
            ServiceError::MissingKeys(variables) => variables.clone(),
            _ => Vec::new(),
        };
        let body = ErrorBody {
            error: self.0.to_string(),
            missing_variables,
        };
        (status, Json(body)).into_response()
    }
}

fn status_for(error: &ServiceError) -> StatusCode {
    match error {
        ServiceError::MissingKeys(_) | ServiceError::BudgetExhausted(_) => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        ServiceError::Source(_) | ServiceError::NoLinkedMatch(_) => StatusCode::BAD_GATEWAY,
        ServiceError::MatchNotFound(_) => StatusCode::NOT_FOUND,
        ServiceError::MatchNotFinished(_) => StatusCode::CONFLICT,
    }
}
