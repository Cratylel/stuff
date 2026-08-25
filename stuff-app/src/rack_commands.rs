//! The only place that depends on the evolving rack-template core API.
//!
//! Keeping this boundary small lets the Types UI stay stable while the core
//! adds persistence and validation for rack templates.

use std::str::FromStr;

use serde::Deserialize;
use serde_json::Value;
use tauri::State;

use crate::{
    AppState,
    dto::{PointDto, SizeDto, SnapshotDto, core_point, core_size, snapshot_to_dto},
};

type SharedState<'a> = State<'a, std::sync::Arc<AppState>>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RackShelfInput {
    pub name: String,
    pub altitude: i64,
    pub usable_height: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RackTemplateInput {
    pub template_id: Option<String>,
    pub name: String,
    pub outer_size: SizeDto,
    pub shelves: Vec<RackShelfInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CloneRackTemplateInput {
    pub source_template_id: String,
    pub name: String,
    pub outer_size: SizeDto,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstantiateRackTemplateInput {
    pub rack_name: String,
    pub room_id: String,
    pub template_id: String,
    pub placement: PointDto,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SaveRackAsTemplateInput {
    pub rack_id: String,
    pub template_name: String,
}

#[tauri::command]
pub(crate) async fn save_rack_template(
    input: RackTemplateInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    let mut template = stuff_core::RackTemplate {
        id: input
            .template_id
            .as_deref()
            .map(rack_template_id)
            .transpose()?
            .unwrap_or_default(),
        name: input.name,
        outer_size: core_size(input.outer_size),
        shelves: input
            .shelves
            .into_iter()
            .map(|shelf| stuff_core::RackShelfTemplate {
                name: shelf.name,
                altitude: shelf.altitude,
                usable_height: shelf.usable_height,
                metadata: Value::Null,
            })
            .collect(),
        metadata: Value::Null,
    };
    if template.id.0.is_nil() {
        state
            .service
            .create_rack_template(template)
            .await
            .map_err(core_error)?;
    } else {
        let snapshot = state.service.snapshot().await;
        template.metadata = snapshot
            .rack_templates
            .iter()
            .find(|existing| existing.id == template.id)
            .map(|existing| existing.metadata.clone())
            .ok_or_else(|| "Rack type no longer exists".to_string())?;
        state
            .service
            .update_rack_template(template)
            .await
            .map_err(core_error)?;
    }
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn clone_rack_template(
    input: CloneRackTemplateInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    state
        .service
        .clone_rack_template(stuff_core::CloneRackTemplateSpec {
            source_template_id: rack_template_id(&input.source_template_id)?,
            name: input.name,
            outer_size: core_size(input.outer_size),
            metadata: Value::Null,
        })
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn instantiate_rack_template(
    input: InstantiateRackTemplateInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    state
        .service
        .create_rack_from_template(stuff_core::CreateRackFromTemplateSpec {
            rack_name: input.rack_name,
            room_id: node_id(&input.room_id)?,
            template_id: rack_template_id(&input.template_id)?,
            placement: Some(stuff_core::Placement {
                position: core_point(input.placement),
            }),
            metadata: Value::Null,
        })
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn save_rack_as_template(
    input: SaveRackAsTemplateInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    state
        .service
        .save_rack_as_template(stuff_core::SaveRackAsTemplateSpec {
            rack_id: node_id(&input.rack_id)?,
            template_name: input.template_name,
            metadata: Value::Null,
        })
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

async fn current_snapshot(state: &std::sync::Arc<AppState>) -> Result<SnapshotDto, String> {
    Ok(snapshot_to_dto(&state.service.snapshot().await, state))
}

fn rack_template_id(value: &str) -> Result<stuff_core::RackTemplateId, String> {
    stuff_core::RackTemplateId::from_str(value)
        .map_err(|error| format!("Invalid rack template id: {error}"))
}

fn node_id(value: &str) -> Result<stuff_core::NodeId, String> {
    stuff_core::NodeId::from_str(value).map_err(|error| format!("Invalid room id: {error}"))
}

fn core_error(error: stuff_core::StuffError) -> String {
    error.to_string()
}
