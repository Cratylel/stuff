use std::str::FromStr;

use serde::Deserialize;
use serde_json::Value;
use tauri::State;

use crate::{
    AppState,
    dto::{
        PointDto, SizeDto, SnapshotDto, core_point, core_size, snapshot_to_dto,
        storage_kind_from_input,
    },
};

type SharedState<'a> = State<'a, std::sync::Arc<AppState>>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NodeInput {
    pub name: String,
    pub kind: String,
    pub parent_id: Option<String>,
    pub placement: Option<PointDto>,
    pub size: Option<SizeDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NodeUpdateInput {
    pub node_id: String,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub placement: Option<PointDto>,
    pub size: Option<SizeDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MoveNodeInput {
    pub node_id: String,
    pub parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlacementPreviewInput {
    pub parent_id: String,
    pub node_id: Option<String>,
    pub proposed_position: PointDto,
    pub size: SizeDto,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlacementPreviewDto {
    pub position: PointDto,
    pub snapped_axes: [bool; 3],
    pub snapped: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ShelfInput {
    pub name: String,
    pub parent_id: String,
    /// `rack` creates an elevation shelf; `room` creates a wall/room shelf.
    pub placement_kind: String,
    pub altitude: Option<i64>,
    pub usable_height: Option<i64>,
    pub placement: Option<PointDto>,
    pub usable_size: Option<SizeDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AddOrCreateItemInput {
    pub name: String,
    pub manufacturer: Option<String>,
    pub manufacturer_part_number: Option<String>,
    pub category: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub quantity: i64,
    pub node_id: String,
    pub organizer_id: Option<String>,
    pub compartment_key: Option<String>,
    pub size: Option<SizeDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StockAdjustmentInput {
    pub lot_id: String,
    pub delta: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MoveStockInput {
    pub from_lot_id: String,
    pub to_node_id: String,
    pub to_organizer_id: Option<String>,
    pub to_compartment_key: Option<String>,
    pub quantity: i64,
    pub item_size: Option<SizeDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StockPlacementInput {
    pub lot_id: String,
    pub placement: PointDto,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CellInput {
    pub row: u32,
    pub column: u32,
    pub rect: RectInput,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RectInput {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub depth: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrganizerTemplateInput {
    pub template_id: Option<String>,
    pub name: String,
    pub outer_size: SizeDto,
    pub cells: Vec<CellInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CloneOrganizerTemplateInput {
    pub source_template_id: String,
    pub name: String,
    pub outer_size: SizeDto,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SaveOrganizerAsTemplateInput {
    pub organizer_node_id: String,
    pub template_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrganizerFromTemplateInput {
    pub organizer_name: String,
    pub parent_id: String,
    pub template_id: String,
    pub placement: Option<PointDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssignOrganizerTemplateInput {
    pub organizer_node_id: String,
    pub template_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FreshOrganizerInput {
    pub template_name: String,
    pub organizer_name: String,
    pub parent_id: String,
    pub outer_size: SizeDto,
    pub cells: Vec<CellInput>,
    pub placement: Option<PointDto>,
}

#[tauri::command]
pub(crate) async fn snapshot(state: SharedState<'_>) -> Result<SnapshotDto, String> {
    Ok(snapshot_to_dto(&state.service.snapshot().await, &state))
}

#[tauri::command]
pub(crate) async fn create_node(
    input: NodeInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    let parent_id = input.parent_id.as_deref().map(node_id).transpose()?;
    state
        .service
        .create_node(stuff_core::StorageNode {
            id: stuff_core::NodeId::default(),
            name: input.name,
            kind: storage_kind_from_input(&input.kind),
            parent_id,
            placement: input.placement.map(|position| stuff_core::Placement {
                position: core_point(position),
            }),
            size: input.size.map(core_size),
            metadata: Value::Null,
        })
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn update_node(
    input: NodeUpdateInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    let id = node_id(&input.node_id)?;
    let snapshot = state.service.snapshot().await;
    let mut node = snapshot
        .nodes
        .into_iter()
        .find(|node| node.id == id)
        .ok_or_else(|| format!("Storage object {} no longer exists", input.node_id))?;
    if let Some(name) = input.name {
        node.name = name;
    }
    if let Some(kind) = input.kind {
        node.kind = storage_kind_from_input(&kind);
    }
    if let Some(position) = input.placement {
        node.placement = Some(stuff_core::Placement {
            position: core_point(position),
        });
    }
    if let Some(size) = input.size {
        node.size = Some(core_size(size));
    }
    state.service.update_node(node).await.map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn move_node(
    input: MoveNodeInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    state
        .service
        .move_node(
            node_id(&input.node_id)?,
            input.parent_id.as_deref().map(node_id).transpose()?,
            None,
        )
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn preview_placement(
    input: PlacementPreviewInput,
    state: SharedState<'_>,
) -> Result<PlacementPreviewDto, String> {
    let parent_id = node_id(&input.parent_id)?;
    let snapshot = state.service.snapshot().await;
    let parent = snapshot
        .nodes
        .iter()
        .find(|node| node.id == parent_id)
        .ok_or_else(|| "Placement parent no longer exists".to_string())?;
    let parent_size = parent
        .size
        .ok_or_else(|| "Placement parent needs dimensions".to_string())?;
    let node_to_skip = input.node_id.as_deref().map(node_id).transpose()?;
    let siblings = snapshot
        .nodes
        .iter()
        .filter(|node| node.parent_id == Some(parent_id) && Some(node.id) != node_to_skip)
        .filter_map(|node| {
            Some(stuff_core::Aabb {
                origin: node.placement?.position,
                size: node.size?,
            })
        })
        .collect::<Vec<_>>();
    let proposal = stuff_core::snap_position_with_neighbors(
        core_point(input.proposed_position),
        core_size(input.size),
        parent_size,
        &siblings,
        10,
        35,
    );
    Ok(PlacementPreviewDto {
        position: proposal.position.into(),
        snapped_axes: proposal.snapped_axes,
        snapped: proposal.snapped_axes.into_iter().any(|axis| axis),
    })
}

#[tauri::command]
pub(crate) async fn create_shelf(
    input: ShelfInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    let parent_id = node_id(&input.parent_id)?;
    let spec = match input.placement_kind.as_str() {
        "rack" => stuff_core::CreateShelfSpec::InRack {
            name: input.name,
            rack_id: parent_id,
            altitude: input.altitude.ok_or("Rack shelves require an altitude")?,
            usable_height: input
                .usable_height
                .ok_or("Rack shelves require a usable height")?,
            metadata: Value::Null,
        },
        "room" => stuff_core::CreateShelfSpec::InRoom {
            name: input.name,
            room_id: parent_id,
            usable_size: core_size(input.usable_size.ok_or("Room shelves need dimensions")?),
            position: core_point(input.placement.ok_or("Room shelves need a position")?),
            metadata: Value::Null,
        },
        _ => return Err("Shelf placement kind must be rack or room".into()),
    };
    state.service.create_shelf(spec).await.map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn clone_storage_subtree(
    node_id_value: String,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    state
        .service
        .clone_storage_subtree(node_id(&node_id_value)?, None)
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn delete_storage_subtree(
    node_id_value: String,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    state
        .service
        .delete_storage_subtree(node_id(&node_id_value)?)
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn delete_storage_orphan_children(
    node_id_value: String,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    state
        .service
        .delete_storage_orphan_children(node_id(&node_id_value)?)
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn add_or_create_item(
    input: AddOrCreateItemInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    let compartment = match (input.organizer_id, input.compartment_key) {
        (Some(organizer_id), Some(key)) => Some(stuff_core::inventory::CompartmentRef {
            organizer_id: organizer_id
                .parse()
                .map_err(|error| format!("Invalid organizer id: {error}"))?,
            key,
        }),
        (None, None) => None,
        _ => return Err("Choose a complete organizer compartment location".into()),
    };
    state
        .service
        .add_or_create_item_with_stock(stuff_core::AddOrCreateItemSpec {
            item: stuff_core::Item {
                id: stuff_core::ItemId::default(),
                name: input.name,
                manufacturer: optional_text(input.manufacturer),
                manufacturer_part_number: optional_text(input.manufacturer_part_number),
                category: optional_text(input.category),
                notes: optional_text(input.notes),
                tags: input
                    .tags
                    .into_iter()
                    .map(|tag| tag.trim().to_owned())
                    .filter(|tag| !tag.is_empty())
                    .collect(),
                size: input.size.map(core_size),
                metadata: Value::Null,
            },
            location: stuff_core::StockLocation {
                node_id: node_id(&input.node_id)?,
                compartment,
            },
            quantity: input.quantity,
        })
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn adjust_stock(
    input: StockAdjustmentInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    let lot_id = stuff_core::StockLotId::from_str(&input.lot_id)
        .map_err(|error| format!("Invalid stock lot id: {error}"))?;
    state
        .service
        .adjust_stock(lot_id, input.delta)
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn move_stock(
    input: MoveStockInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    let compartment = match (input.to_organizer_id, input.to_compartment_key) {
        (Some(organizer_id), Some(key)) => Some(stuff_core::inventory::CompartmentRef {
            organizer_id: organizer_id
                .parse()
                .map_err(|error| format!("Invalid organizer id: {error}"))?,
            key,
        }),
        (None, None) => None,
        _ => return Err("Choose a complete organizer compartment destination".into()),
    };
    state
        .service
        .move_stock_with_item_size(
            stuff_core::StockLotId::from_str(&input.from_lot_id)
                .map_err(|error| format!("Invalid stock lot id: {error}"))?,
            stuff_core::StockLocation {
                node_id: node_id(&input.to_node_id)?,
                compartment,
            },
            input.quantity,
            input.item_size.map(core_size),
        )
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn update_stock_placement(
    input: StockPlacementInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    state
        .service
        .update_stock_placement(
            stuff_core::StockLotId::from_str(&input.lot_id)
                .map_err(|error| format!("Invalid stock lot id: {error}"))?,
            stuff_core::Placement {
                position: core_point(input.placement),
            },
        )
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn save_organizer_template(
    input: OrganizerTemplateInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    let mut template = stuff_core::OrganizerTemplate {
        id: input
            .template_id
            .as_deref()
            .map(template_id)
            .transpose()?
            .unwrap_or_default(),
        name: input.name,
        size: core_size(input.outer_size),
        cells: core_cells(input.cells),
        dividers: Vec::new(),
        metadata: Value::Null,
    };
    if template.id.0.is_nil() {
        state
            .service
            .create_organizer_template(template)
            .await
            .map_err(core_error)?;
    } else {
        let snapshot = state.service.snapshot().await;
        template.metadata = snapshot
            .organizer_templates
            .iter()
            .find(|existing| existing.id == template.id)
            .map(|existing| existing.metadata.clone())
            .ok_or_else(|| "Organizer type no longer exists".to_string())?;
        state
            .service
            .update_organizer_template(template)
            .await
            .map_err(core_error)?;
    }
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn clone_organizer_template(
    input: CloneOrganizerTemplateInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    state
        .service
        .clone_organizer_template(stuff_core::CloneOrganizerTemplateSpec {
            source_template_id: template_id(&input.source_template_id)?,
            name: input.name,
            outer_size: core_size(input.outer_size),
            metadata: Value::Null,
        })
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn save_organizer_as_template(
    input: SaveOrganizerAsTemplateInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    state
        .service
        .save_organizer_as_template(stuff_core::SaveOrganizerAsTemplateSpec {
            organizer_node_id: node_id(&input.organizer_node_id)?,
            template_name: input.template_name,
            metadata: Value::Null,
        })
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn create_organizer_from_template(
    input: OrganizerFromTemplateInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    state
        .service
        .create_organizer_from_template(stuff_core::CreateOrganizerFromTemplateSpec {
            organizer_name: input.organizer_name,
            parent_id: node_id(&input.parent_id)?,
            template_id: template_id(&input.template_id)?,
            placement: input.placement.map(|position| stuff_core::Placement {
                position: core_point(position),
            }),
            metadata: Value::Null,
        })
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn assign_organizer_template(
    input: AssignOrganizerTemplateInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    state
        .service
        .assign_organizer_template(
            node_id(&input.organizer_node_id)?,
            template_id(&input.template_id)?,
        )
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

#[tauri::command]
pub(crate) async fn create_fresh_organizer(
    input: FreshOrganizerInput,
    state: SharedState<'_>,
) -> Result<SnapshotDto, String> {
    state
        .service
        .create_fresh_organizer(stuff_core::FreshOrganizerSpec {
            template_name: input.template_name,
            organizer_name: input.organizer_name,
            parent_id: node_id(&input.parent_id)?,
            outer_size: core_size(input.outer_size),
            cells: core_cells(input.cells),
            placement: input.placement.map(|position| stuff_core::Placement {
                position: core_point(position),
            }),
            template_metadata: Value::Null,
            organizer_metadata: Value::Null,
        })
        .await
        .map_err(core_error)?;
    current_snapshot(&state).await
}

async fn current_snapshot(state: &std::sync::Arc<AppState>) -> Result<SnapshotDto, String> {
    Ok(snapshot_to_dto(&state.service.snapshot().await, state))
}

fn node_id(value: &str) -> Result<stuff_core::NodeId, String> {
    value
        .parse()
        .map_err(|error| format!("Invalid storage id: {error}"))
}

fn template_id(value: &str) -> Result<stuff_core::TemplateId, String> {
    value
        .parse()
        .map_err(|error| format!("Invalid organizer template id: {error}"))
}

fn core_cells(cells: Vec<CellInput>) -> Vec<stuff_core::PrimitiveCell> {
    cells
        .into_iter()
        .map(|cell| stuff_core::PrimitiveCell {
            id: stuff_core::CellId {
                row: cell.row,
                column: cell.column,
            },
            rect: stuff_core::Rect {
                x: cell.rect.x,
                y: cell.rect.y,
                width: cell.rect.width,
                depth: cell.rect.depth,
            },
        })
        .collect()
}

fn optional_text(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn core_error(error: stuff_core::StuffError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_are_passed_to_core_in_integer_millimetres() {
        let cells = core_cells(vec![CellInput {
            row: 2,
            column: 3,
            rect: RectInput {
                x: 45,
                y: 80,
                width: 100,
                depth: 120,
            },
        }]);
        assert_eq!(cells[0].rect.width, 100);
        assert_eq!(cells[0].id.column, 3);
    }

    #[test]
    fn cell_less_organizer_input_is_not_a_custom_or_typed_organizer() {
        assert_eq!(
            storage_kind_from_input("cell_less_organizer"),
            stuff_core::StorageKind::CellLessOrganizer
        );
    }
}
