use std::collections::BTreeMap;

use serde::Serialize;
use stuff_core::{
    DimensionSemantics, OrganizerInstance, OrganizerTemplate, Point3, RackTemplate, Size3,
    Snapshot, StorageKind, StorageNode,
};

use crate::{AppState, BackendMode};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SnapshotDto {
    pub revision: u64,
    pub backend: BackendDto,
    pub nodes: Vec<NodeDto>,
    pub organizer_templates: Vec<OrganizerTemplateDto>,
    pub organizers: Vec<OrganizerInstanceDto>,
    pub rack_templates: Vec<RackTemplateDto>,
    pub items: Vec<ItemDto>,
    pub stock_lots: Vec<StockLotDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackendDto {
    pub mode: BackendMode,
    pub connection_note: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NodeDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub kind: String,
    pub parent_id: Option<String>,
    pub placement: Option<PointDto>,
    pub size: Option<SizeDto>,
    pub dimension_semantics: String,
    pub accepts_inventory: bool,
    pub spatial_stock: bool,
    pub allowed_child_kinds: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, serde::Deserialize)]
pub(crate) struct PointDto {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

#[derive(Clone, Copy, Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SizeDto {
    pub width: i64,
    pub depth: i64,
    pub height: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrganizerTemplateDto {
    pub id: String,
    pub name: String,
    pub outer_size: SizeDto,
    pub cells: Vec<CellDto>,
    pub instance_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CellDto {
    pub row: u32,
    pub column: u32,
    pub rect: RectDto,
}

#[derive(Clone, Copy, Debug, Serialize, serde::Deserialize)]
pub(crate) struct RectDto {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub depth: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrganizerInstanceDto {
    pub id: String,
    pub node_id: String,
    pub template_id: String,
    pub compartments: Vec<CompartmentDto>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CompartmentDto {
    pub key: String,
    pub rect: RectDto,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RackTemplateDto {
    pub id: String,
    pub name: String,
    pub outer_size: SizeDto,
    pub shelves: Vec<RackShelfDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RackShelfDto {
    pub name: String,
    pub altitude: i64,
    pub usable_height: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ItemDto {
    pub id: String,
    pub name: String,
    pub manufacturer: Option<String>,
    pub manufacturer_part_number: Option<String>,
    pub category: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub size: Option<SizeDto>,
    pub total_quantity: i64,
    pub locations: Vec<ItemLocationDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ItemLocationDto {
    pub lot_id: String,
    pub node_id: String,
    pub organizer_id: Option<String>,
    pub compartment_key: Option<String>,
    pub quantity: i64,
    pub placement: Option<PointDto>,
    pub path: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StockLotDto {
    pub id: String,
    pub item_id: String,
    pub node_id: String,
    pub organizer_id: Option<String>,
    pub compartment_key: Option<String>,
    pub quantity: i64,
    pub placement: Option<PointDto>,
    pub size: Option<SizeDto>,
    pub path: String,
}

pub(crate) fn snapshot_to_dto(snapshot: &Snapshot, state: &AppState) -> SnapshotDto {
    let templates: BTreeMap<_, _> = snapshot
        .organizer_templates
        .iter()
        .map(|template| (template.id, template))
        .collect();
    let stock_lots = snapshot
        .stock
        .iter()
        .map(|lot| {
            let (organizer_id, compartment_key) = lot
                .location
                .compartment
                .as_ref()
                .map(|cell| (Some(cell.organizer_id.to_string()), Some(cell.key.clone())))
                .unwrap_or((None, None));
            StockLotDto {
                id: lot.id.to_string(),
                item_id: lot.item_id.to_string(),
                node_id: lot.location.node_id.to_string(),
                organizer_id,
                compartment_key,
                quantity: lot.quantity,
                placement: lot
                    .placement
                    .map(|placement| PointDto::from(placement.position)),
                size: snapshot
                    .items
                    .iter()
                    .find(|item| item.id == lot.item_id)
                    .and_then(|item| item.size)
                    .map(SizeDto::from),
                path: stuff_core::display_location(snapshot, &lot.location)
                    .unwrap_or_else(|_| "Unknown location".into()),
            }
        })
        .collect::<Vec<_>>();

    let items = snapshot
        .items
        .iter()
        .map(|item| {
            let locations = stock_lots
                .iter()
                .filter(|lot| lot.item_id == item.id.to_string())
                .map(|lot| ItemLocationDto {
                    lot_id: lot.id.clone(),
                    node_id: lot.node_id.clone(),
                    organizer_id: lot.organizer_id.clone(),
                    compartment_key: lot.compartment_key.clone(),
                    quantity: lot.quantity,
                    placement: lot.placement,
                    path: lot.path.clone(),
                })
                .collect::<Vec<_>>();
            ItemDto {
                id: item.id.to_string(),
                name: item.name.clone(),
                manufacturer: item.manufacturer.clone(),
                manufacturer_part_number: item.manufacturer_part_number.clone(),
                category: item.category.clone(),
                notes: item.notes.clone(),
                tags: item.tags.clone(),
                size: item.size.map(SizeDto::from),
                total_quantity: locations.iter().map(|location| location.quantity).sum(),
                locations,
            }
        })
        .collect();

    SnapshotDto {
        revision: snapshot.revision,
        backend: BackendDto {
            mode: state.mode,
            connection_note: state.connection_note.clone(),
        },
        nodes: snapshot
            .nodes
            .iter()
            .map(|node| node_to_dto(snapshot, node))
            .collect(),
        organizer_templates: snapshot
            .organizer_templates
            .iter()
            .map(|template| organizer_template_to_dto(template, snapshot))
            .collect(),
        organizers: snapshot
            .organizers
            .iter()
            .filter_map(|organizer| organizer_to_dto(organizer, &templates))
            .collect(),
        rack_templates: snapshot
            .rack_templates
            .iter()
            .map(rack_template_to_dto)
            .collect(),
        items,
        stock_lots,
    }
}

pub(crate) fn node_to_dto(snapshot: &Snapshot, node: &StorageNode) -> NodeDto {
    NodeDto {
        id: node.id.to_string(),
        name: node.name.clone(),
        path: stuff_core::path_for(&snapshot.nodes, node.id).unwrap_or_else(|_| node.name.clone()),
        kind: node.kind.as_str().to_owned(),
        parent_id: node.parent_id.map(|id| id.to_string()),
        placement: node
            .placement
            .map(|placement| PointDto::from(placement.position)),
        size: node.size.map(SizeDto::from),
        dimension_semantics: match node.dimension_semantics() {
            DimensionSemantics::RoomBounds => "roomBounds",
            DimensionSemantics::Outer => "outer",
            DimensionSemantics::Usable => "usable",
        }
        .into(),
        accepts_inventory: node.kind.accepts_inventory(),
        spatial_stock: node.kind.has_spatial_stock(),
        allowed_child_kinds: stuff_core::allowed_child_kinds(Some(&node.kind))
            .into_iter()
            .map(|kind| kind.as_str().to_owned())
            .collect(),
    }
}

pub(crate) fn organizer_template_to_dto(
    template: &OrganizerTemplate,
    snapshot: &Snapshot,
) -> OrganizerTemplateDto {
    OrganizerTemplateDto {
        id: template.id.to_string(),
        name: template.name.clone(),
        outer_size: template.size.into(),
        cells: template
            .cells
            .iter()
            .map(|cell| CellDto {
                row: cell.id.row,
                column: cell.id.column,
                rect: RectDto {
                    x: cell.rect.x,
                    y: cell.rect.y,
                    width: cell.rect.width,
                    depth: cell.rect.depth,
                },
            })
            .collect(),
        instance_count: snapshot
            .organizers
            .iter()
            .filter(|organizer| organizer.template_id == template.id)
            .count(),
    }
}

fn organizer_to_dto(
    organizer: &OrganizerInstance,
    templates: &BTreeMap<stuff_core::TemplateId, &OrganizerTemplate>,
) -> Option<OrganizerInstanceDto> {
    let template = templates.get(&organizer.template_id)?;
    Some(OrganizerInstanceDto {
        id: organizer.id.to_string(),
        node_id: organizer.node_id.to_string(),
        template_id: organizer.template_id.to_string(),
        compartments: template
            .compartments(&organizer.removed_dividers)
            .into_iter()
            .map(|compartment| CompartmentDto {
                key: compartment.key,
                rect: RectDto {
                    x: compartment.rect.x,
                    y: compartment.rect.y,
                    width: compartment.rect.width,
                    depth: compartment.rect.depth,
                },
            })
            .collect(),
    })
}

fn rack_template_to_dto(template: &RackTemplate) -> RackTemplateDto {
    RackTemplateDto {
        id: template.id.to_string(),
        name: template.name.clone(),
        outer_size: template.outer_size.into(),
        shelves: template
            .shelves
            .iter()
            .map(|shelf| RackShelfDto {
                name: shelf.name.clone(),
                altitude: shelf.altitude,
                usable_height: shelf.usable_height,
            })
            .collect(),
    }
}

impl From<Point3> for PointDto {
    fn from(point: Point3) -> Self {
        Self {
            x: point.x,
            y: point.y,
            z: point.z,
        }
    }
}

impl From<Size3> for SizeDto {
    fn from(size: Size3) -> Self {
        Self {
            width: size.width,
            depth: size.depth,
            height: size.height,
        }
    }
}

pub(crate) fn storage_kind_from_input(kind: &str) -> StorageKind {
    match kind {
        "room" => StorageKind::Room,
        "rack" => StorageKind::Rack,
        "shelf" => StorageKind::Shelf,
        "drawer" => StorageKind::Drawer,
        "fixture" => StorageKind::Fixture,
        "cell_less_organizer" => StorageKind::CellLessOrganizer,
        "organizer" => StorageKind::Organizer,
        "box" => StorageKind::Box,
        "bin" => StorageKind::Bin,
        "loose" => StorageKind::Loose,
        custom => StorageKind::Custom(custom.to_owned()),
    }
}

pub(crate) fn core_size(size: SizeDto) -> Size3 {
    Size3 {
        width: size.width,
        depth: size.depth,
        height: size.height,
    }
}

pub(crate) fn core_point(point: PointDto) -> Point3 {
    Point3 {
        x: point.x,
        y: point.y,
        z: point.z,
    }
}
