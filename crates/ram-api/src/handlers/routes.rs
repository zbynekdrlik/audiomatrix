//! Route-related API handlers.

use axum::extract::{Path, State};
use axum::Json;

use crate::models::{PatchRouteRequest, RouteDefinition};
use crate::state::AppState;
use crate::websocket::{RouteUpdate, WsEvent};
use crate::Result;

/// List all routes.
pub async fn list_routes(State(state): State<AppState>) -> Result<Json<Vec<RouteDefinition>>> {
    Ok(Json(state.all_routes()))
}

/// Get a specific route.
pub async fn get_route(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RouteDefinition>> {
    state
        .get_route(&id)
        .map(Json)
        .ok_or_else(|| crate::Error::NotFound(format!("route: {id}")))
}

/// Create a new route.
pub async fn create_route(
    State(state): State<AppState>,
    Json(route): Json<RouteDefinition>,
) -> Result<Json<RouteCreatedResponse>> {
    let id = state
        .upsert_route(route)
        .map_err(crate::Error::BadRequest)?;

    state.broadcast_event(WsEvent::RouteChanged(RouteUpdate {
        action: "added".into(),
        route_id: id.clone(),
    }));

    Ok(Json(RouteCreatedResponse { id }))
}

/// Update an existing route.
pub async fn update_route(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(route): Json<RouteDefinition>,
) -> Result<Json<RouteDefinition>> {
    if state.get_route(&id).is_none() {
        return Err(crate::Error::NotFound(format!("route: {id}")));
    }

    let new_id = state
        .upsert_route(route.clone())
        .map_err(crate::Error::BadRequest)?;

    if new_id != id {
        let _ = state.remove_route(&id);
    }

    state.broadcast_event(WsEvent::RouteChanged(RouteUpdate {
        action: "modified".into(),
        route_id: new_id,
    }));

    Ok(Json(route))
}

/// Partially update a route (volume/muted only).
pub async fn patch_route(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(patch): Json<PatchRouteRequest>,
) -> Result<Json<RouteDefinition>> {
    let mut route = state
        .get_route(&id)
        .ok_or_else(|| crate::Error::NotFound(format!("route: {id}")))?;

    // Apply patches
    if let Some(volume) = patch.volume {
        route.volume = volume;
    }
    if let Some(muted) = patch.muted {
        route.muted = muted;
    }

    // Save updated route
    let new_id = state
        .upsert_route(route.clone())
        .map_err(crate::Error::BadRequest)?;

    state.broadcast_event(WsEvent::RouteChanged(RouteUpdate {
        action: "modified".into(),
        route_id: new_id,
    }));

    Ok(Json(route))
}

/// Delete a route.
pub async fn delete_route(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RouteDeletedResponse>> {
    let removed = state.remove_route(&id).map_err(crate::Error::Internal)?;

    if removed.is_none() {
        return Err(crate::Error::NotFound(format!("route: {id}")));
    }

    state.broadcast_event(WsEvent::RouteChanged(RouteUpdate {
        action: "removed".into(),
        route_id: id.clone(),
    }));

    Ok(Json(RouteDeletedResponse { id }))
}

/// Response for route creation.
#[derive(Debug, serde::Serialize)]
pub struct RouteCreatedResponse {
    pub id: String,
}

/// Response for route deletion.
#[derive(Debug, serde::Serialize)]
pub struct RouteDeletedResponse {
    pub id: String,
}
