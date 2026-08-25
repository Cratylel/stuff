//! PostgreSQL mapping for the public domain snapshot.
//!
//! `Snapshot` remains the service boundary, but every field is stored in its
//! own relational table. JSONB is used only for the explicitly schemaless
//! `metadata` fields.

use std::collections::{BTreeMap, BTreeSet};

use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    domain::*,
    drawer::{DrawerCabinetInstance, DrawerCabinetTemplate, DrawerTemplateDrawer},
    errors::{Result, StuffError},
    inventory::{CompartmentRef, Item, StockLocation, StockLot},
    occupancy::{FillEstimate, LocationRef, Occupancy, OccupancyState, Reservation},
    organizer::{
        CellId, DividerAxis, DividerEdge, DividerKind, OrganizerInstance, OrganizerTemplate,
        PrimitiveCell,
    },
    rack::{RackInstance, RackShelfTemplate, RackTemplate},
};

type LocationKey = (NodeId, Option<OrganizerId>, Option<String>);

pub(crate) async fn load_snapshot(pool: &PgPool) -> Result<Option<Snapshot>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let revision = sqlx::query_scalar::<_, i64>("SELECT revision FROM state_revision WHERE id = 1")
        .fetch_one(&mut *tx)
        .await?
        .max(0) as u64;

    let mut snapshot = Snapshot {
        revision,
        ..Snapshot::default()
    };
    snapshot.nodes = load_nodes(&mut tx).await?;
    snapshot.organizer_templates = load_organizer_templates(&mut tx).await?;
    snapshot.rack_templates = load_rack_templates(&mut tx).await?;
    snapshot.drawer_cabinet_templates = load_drawer_templates(&mut tx).await?;
    snapshot.organizers = load_organizer_instances(&mut tx).await?;
    snapshot.rack_instances = load_rack_instances(&mut tx).await?;
    snapshot.drawer_cabinet_instances = load_drawer_instances(&mut tx).await?;
    snapshot.items = load_items(&mut tx).await?;
    let locations = load_locations(&mut tx).await?;
    snapshot.stock = load_stock(&mut tx, &locations).await?;
    snapshot.reservations = load_reservations(&mut tx, &locations).await?;
    snapshot.occupancies = load_occupancies(&mut tx, &locations).await?;
    tx.commit().await?;

    let empty = snapshot.nodes.is_empty()
        && snapshot.organizer_templates.is_empty()
        && snapshot.rack_templates.is_empty()
        && snapshot.drawer_cabinet_templates.is_empty()
        && snapshot.items.is_empty();
    Ok((!empty || revision != 0).then_some(snapshot))
}

async fn load_nodes(tx: &mut Transaction<'_, Postgres>) -> Result<Vec<StorageNode>> {
    let rows = sqlx::query(
        "SELECT id, name, kind, custom_kind, parent_id, position_x, position_y, position_z, \
                width, depth, height, metadata FROM storage_nodes ORDER BY name, id",
    )
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| {
            let kind: String = row.try_get("kind")?;
            let kind = decode_storage_kind(&kind, row.try_get("custom_kind")?)?;
            let position_x: Option<i64> = row.try_get("position_x")?;
            let width: Option<i64> = row.try_get("width")?;
            let placement = match position_x {
                Some(x) => Some(Placement {
                    position: Point3 {
                        x,
                        y: row.try_get("position_y")?,
                        z: row.try_get("position_z")?,
                    },
                }),
                None => None,
            };
            let size = match width {
                Some(width) => Some(Size3 {
                    width,
                    depth: row.try_get("depth")?,
                    height: row.try_get("height")?,
                }),
                None => None,
            };
            Ok(StorageNode {
                id: NodeId(row.try_get("id")?),
                name: row.try_get("name")?,
                kind,
                parent_id: row.try_get::<Option<Uuid>, _>("parent_id")?.map(NodeId),
                placement,
                size,
                metadata: row.try_get("metadata")?,
            })
        })
        .collect()
}

async fn load_organizer_templates(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<Vec<OrganizerTemplate>> {
    let mut templates = sqlx::query(
        "SELECT id, name, width, depth, height, metadata FROM organizer_templates ORDER BY name, id",
    )
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|row| {
        Ok(OrganizerTemplate {
            id: TemplateId(row.try_get("id")?),
            name: row.try_get("name")?,
            size: Size3 {
                width: row.try_get("width")?,
                depth: row.try_get("depth")?,
                height: row.try_get("height")?,
            },
            cells: Vec::new(),
            dividers: Vec::new(),
            metadata: row.try_get("metadata")?,
        })
    })
    .collect::<Result<Vec<_>>>()?;
    let indices = templates
        .iter()
        .enumerate()
        .map(|(index, template)| (template.id, index))
        .collect::<BTreeMap<_, _>>();
    for row in sqlx::query(
        "SELECT template_id, row_no, column_no, x, y, width, depth \
         FROM organizer_template_cells ORDER BY template_id, row_no, column_no",
    )
    .fetch_all(&mut **tx)
    .await?
    {
        let id = TemplateId(row.try_get("template_id")?);
        let template = indices.get(&id).copied().ok_or_else(|| {
            StuffError::Invalid(format!("cell references missing organizer template {id}"))
        })?;
        templates[template].cells.push(PrimitiveCell {
            id: CellId {
                row: decode_cell_coordinate(row.try_get("row_no")?, "row")?,
                column: decode_cell_coordinate(row.try_get("column_no")?, "column")?,
            },
            rect: Rect {
                x: row.try_get("x")?,
                y: row.try_get("y")?,
                width: row.try_get("width")?,
                depth: row.try_get("depth")?,
            },
        });
    }
    for row in sqlx::query(
        "SELECT template_id, a_row, a_column, b_row, b_column, axis, kind \
         FROM organizer_template_dividers \
         ORDER BY template_id, a_row, a_column, b_row, b_column",
    )
    .fetch_all(&mut **tx)
    .await?
    {
        let id = TemplateId(row.try_get("template_id")?);
        let template = indices.get(&id).copied().ok_or_else(|| {
            StuffError::Invalid(format!(
                "divider references missing organizer template {id}"
            ))
        })?;
        templates[template].dividers.push(DividerEdge {
            a: CellId {
                row: decode_cell_coordinate(row.try_get("a_row")?, "divider a row")?,
                column: decode_cell_coordinate(row.try_get("a_column")?, "divider a column")?,
            },
            b: CellId {
                row: decode_cell_coordinate(row.try_get("b_row")?, "divider b row")?,
                column: decode_cell_coordinate(row.try_get("b_column")?, "divider b column")?,
            },
            axis: match row.try_get::<String, _>("axis")?.as_str() {
                "vertical" => DividerAxis::Vertical,
                "horizontal" => DividerAxis::Horizontal,
                value => return Err(StuffError::Invalid(format!("unknown divider axis {value}"))),
            },
            kind: match row.try_get::<String, _>("kind")?.as_str() {
                "fixed" => DividerKind::Fixed,
                "removable" => DividerKind::Removable,
                "impossible" => DividerKind::Impossible,
                value => return Err(StuffError::Invalid(format!("unknown divider kind {value}"))),
            },
        });
    }
    Ok(templates)
}

async fn load_rack_templates(tx: &mut Transaction<'_, Postgres>) -> Result<Vec<RackTemplate>> {
    let mut templates = sqlx::query(
        "SELECT id, name, width, depth, height, metadata FROM rack_templates ORDER BY name, id",
    )
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|row| {
        Ok(RackTemplate {
            id: RackTemplateId(row.try_get("id")?),
            name: row.try_get("name")?,
            outer_size: Size3 {
                width: row.try_get("width")?,
                depth: row.try_get("depth")?,
                height: row.try_get("height")?,
            },
            shelves: Vec::new(),
            metadata: row.try_get("metadata")?,
        })
    })
    .collect::<Result<Vec<_>>>()?;
    let indices = templates
        .iter()
        .enumerate()
        .map(|(index, template)| (template.id, index))
        .collect::<BTreeMap<_, _>>();
    for row in sqlx::query(
        "SELECT template_id, name, altitude, usable_height, metadata \
         FROM rack_template_shelves ORDER BY template_id, ordinal",
    )
    .fetch_all(&mut **tx)
    .await?
    {
        let id = RackTemplateId(row.try_get("template_id")?);
        let index = indices.get(&id).copied().ok_or_else(|| {
            StuffError::Invalid(format!("shelf references missing rack template {id}"))
        })?;
        templates[index].shelves.push(RackShelfTemplate {
            name: row.try_get("name")?,
            altitude: row.try_get("altitude")?,
            usable_height: row.try_get("usable_height")?,
            metadata: row.try_get("metadata")?,
        });
    }
    Ok(templates)
}

async fn load_drawer_templates(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<Vec<DrawerCabinetTemplate>> {
    let mut templates = sqlx::query(
        "SELECT id, name, width, depth, height, metadata \
         FROM drawer_cabinet_templates ORDER BY name, id",
    )
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|row| {
        Ok(DrawerCabinetTemplate {
            id: DrawerCabinetTemplateId(row.try_get("id")?),
            name: row.try_get("name")?,
            outer_size: Size3 {
                width: row.try_get("width")?,
                depth: row.try_get("depth")?,
                height: row.try_get("height")?,
            },
            drawers: Vec::new(),
            metadata: row.try_get("metadata")?,
        })
    })
    .collect::<Result<Vec<_>>>()?;
    let indices = templates
        .iter()
        .enumerate()
        .map(|(index, template)| (template.id, index))
        .collect::<BTreeMap<_, _>>();
    for row in sqlx::query(
        "SELECT template_id, name, altitude, usable_width, usable_depth, usable_height, metadata \
         FROM drawer_template_drawers ORDER BY template_id, ordinal",
    )
    .fetch_all(&mut **tx)
    .await?
    {
        let id = DrawerCabinetTemplateId(row.try_get("template_id")?);
        let index = indices.get(&id).copied().ok_or_else(|| {
            StuffError::Invalid(format!("drawer references missing cabinet template {id}"))
        })?;
        templates[index].drawers.push(DrawerTemplateDrawer {
            name: row.try_get("name")?,
            altitude: row.try_get("altitude")?,
            usable_size: Size3 {
                width: row.try_get("usable_width")?,
                depth: row.try_get("usable_depth")?,
                height: row.try_get("usable_height")?,
            },
            metadata: row.try_get("metadata")?,
        });
    }
    Ok(templates)
}

async fn load_organizer_instances(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<Vec<OrganizerInstance>> {
    let rows = sqlx::query(
        "SELECT instance.id, instance.node_id, instance.template_id, removed.divider_key \
         FROM organizer_instances instance \
         LEFT JOIN organizer_removed_dividers removed ON removed.organizer_id = instance.id \
         ORDER BY instance.id, removed.divider_key",
    )
    .fetch_all(&mut **tx)
    .await?;
    let mut instances: BTreeMap<OrganizerId, OrganizerInstance> = BTreeMap::new();
    for row in rows {
        let id = OrganizerId(row.try_get("id")?);
        let instance = instances.entry(id).or_insert(OrganizerInstance {
            id,
            node_id: NodeId(row.try_get("node_id")?),
            template_id: TemplateId(row.try_get("template_id")?),
            removed_dividers: BTreeSet::new(),
        });
        if let Some(key) = row.try_get::<Option<String>, _>("divider_key")? {
            instance.removed_dividers.insert(key);
        }
    }
    Ok(instances.into_values().collect())
}

async fn load_rack_instances(tx: &mut Transaction<'_, Postgres>) -> Result<Vec<RackInstance>> {
    let rows = sqlx::query(
        "SELECT instance.rack_id, instance.template_id, shelf.shelf_id \
         FROM rack_instances instance \
         LEFT JOIN rack_instance_shelves shelf ON shelf.rack_id = instance.rack_id \
         ORDER BY instance.rack_id, shelf.ordinal",
    )
    .fetch_all(&mut **tx)
    .await?;
    let mut instances: BTreeMap<NodeId, RackInstance> = BTreeMap::new();
    for row in rows {
        let rack_id = NodeId(row.try_get("rack_id")?);
        let instance = instances.entry(rack_id).or_insert(RackInstance {
            rack_id,
            template_id: RackTemplateId(row.try_get("template_id")?),
            shelf_ids: Vec::new(),
        });
        if let Some(id) = row.try_get::<Option<Uuid>, _>("shelf_id")? {
            instance.shelf_ids.push(NodeId(id));
        }
    }
    Ok(instances.into_values().collect())
}

async fn load_drawer_instances(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<Vec<DrawerCabinetInstance>> {
    let rows = sqlx::query(
        "SELECT instance.cabinet_id, instance.template_id, drawer.drawer_id \
         FROM drawer_cabinet_instances instance \
         LEFT JOIN drawer_instance_nodes drawer ON drawer.cabinet_id = instance.cabinet_id \
         ORDER BY instance.cabinet_id, drawer.ordinal",
    )
    .fetch_all(&mut **tx)
    .await?;
    let mut instances: BTreeMap<NodeId, DrawerCabinetInstance> = BTreeMap::new();
    for row in rows {
        let cabinet_id = NodeId(row.try_get("cabinet_id")?);
        let instance = instances
            .entry(cabinet_id)
            .or_insert(DrawerCabinetInstance {
                cabinet_id,
                template_id: DrawerCabinetTemplateId(row.try_get("template_id")?),
                drawer_ids: Vec::new(),
            });
        if let Some(id) = row.try_get::<Option<Uuid>, _>("drawer_id")? {
            instance.drawer_ids.push(NodeId(id));
        }
    }
    Ok(instances.into_values().collect())
}

async fn load_items(tx: &mut Transaction<'_, Postgres>) -> Result<Vec<Item>> {
    let rows = sqlx::query(
        "SELECT item.id, item.name, item.manufacturer, item.manufacturer_part_number, \
                item.category, item.notes, item.item_width, item.item_depth, \
                item.item_height, item.metadata, tag.tag \
         FROM items item LEFT JOIN item_tags tag ON tag.item_id = item.id \
         ORDER BY item.name, item.id, tag.ordinal",
    )
    .fetch_all(&mut **tx)
    .await?;
    let mut items: BTreeMap<ItemId, Item> = BTreeMap::new();
    for row in rows {
        let id = ItemId(row.try_get("id")?);
        let item = items.entry(id).or_insert(Item {
            id,
            name: row.try_get("name")?,
            manufacturer: row.try_get("manufacturer")?,
            manufacturer_part_number: row.try_get("manufacturer_part_number")?,
            category: row.try_get("category")?,
            notes: row.try_get("notes")?,
            tags: Vec::new(),
            size: row
                .try_get::<Option<i64>, _>("item_width")?
                .map(|width| {
                    Ok::<Size3, sqlx::Error>(Size3 {
                        width,
                        depth: row.try_get("item_depth")?,
                        height: row.try_get("item_height")?,
                    })
                })
                .transpose()?,
            metadata: row.try_get("metadata")?,
        });
        if let Some(tag) = row.try_get::<Option<String>, _>("tag")? {
            item.tags.push(tag);
        }
    }
    Ok(items.into_values().collect())
}

async fn load_locations(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<BTreeMap<Uuid, (StockLocation, LocationRef)>> {
    let rows = sqlx::query(
        "SELECT id, node_id, organizer_id, compartment_key FROM inventory_locations ORDER BY id",
    )
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| {
            let id: Uuid = row.try_get("id")?;
            let node_id = NodeId(row.try_get("node_id")?);
            let organizer_id = row
                .try_get::<Option<Uuid>, _>("organizer_id")?
                .map(OrganizerId);
            let compartment_key: Option<String> = row.try_get("compartment_key")?;
            let compartment = organizer_id
                .zip(compartment_key.clone())
                .map(|(organizer_id, key)| CompartmentRef { organizer_id, key });
            Ok((
                id,
                (
                    StockLocation {
                        node_id,
                        compartment,
                    },
                    LocationRef {
                        node_id,
                        organizer_id,
                        compartment_key,
                    },
                ),
            ))
        })
        .collect()
}

async fn load_stock(
    tx: &mut Transaction<'_, Postgres>,
    locations: &BTreeMap<Uuid, (StockLocation, LocationRef)>,
) -> Result<Vec<StockLot>> {
    sqlx::query(
        "SELECT id, item_id, location_id, quantity, position_x, position_y, position_z \
         FROM stock_lots ORDER BY id",
    )
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|row| {
        let location_id: Uuid = row.try_get("location_id")?;
        let location = locations
            .get(&location_id)
            .ok_or_else(|| StuffError::Invalid(format!("missing location {location_id}")))?
            .0
            .clone();
        Ok(StockLot {
            id: StockLotId(row.try_get("id")?),
            item_id: ItemId(row.try_get("item_id")?),
            location,
            quantity: row.try_get("quantity")?,
            placement: row
                .try_get::<Option<i64>, _>("position_x")?
                .map(|x| {
                    Ok::<Placement, sqlx::Error>(Placement {
                        position: Point3 {
                            x,
                            y: row.try_get("position_y")?,
                            z: row.try_get("position_z")?,
                        },
                    })
                })
                .transpose()?,
        })
    })
    .collect()
}

async fn load_reservations(
    tx: &mut Transaction<'_, Postgres>,
    locations: &BTreeMap<Uuid, (StockLocation, LocationRef)>,
) -> Result<Vec<Reservation>> {
    sqlx::query("SELECT id, location_id, label, notes FROM reservations ORDER BY id")
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|row| {
            let location_id: Uuid = row.try_get("location_id")?;
            let location = locations
                .get(&location_id)
                .ok_or_else(|| StuffError::Invalid(format!("missing location {location_id}")))?
                .1
                .clone();
            Ok(Reservation {
                id: ReservationId(row.try_get("id")?),
                location,
                label: row.try_get("label")?,
                notes: row.try_get("notes")?,
            })
        })
        .collect()
}

async fn load_occupancies(
    tx: &mut Transaction<'_, Postgres>,
    locations: &BTreeMap<Uuid, (StockLocation, LocationRef)>,
) -> Result<Vec<Occupancy>> {
    sqlx::query("SELECT location_id, state, fill_percent FROM occupancies ORDER BY location_id")
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|row| {
            let location_id: Uuid = row.try_get("location_id")?;
            let location = locations
                .get(&location_id)
                .ok_or_else(|| StuffError::Invalid(format!("missing location {location_id}")))?
                .1
                .clone();
            let state = match row.try_get::<String, _>("state")?.as_str() {
                "free" => OccupancyState::Free,
                "occupied" => OccupancyState::Occupied,
                "reserved" => OccupancyState::Reserved,
                "blocked" => OccupancyState::Blocked,
                value => {
                    return Err(StuffError::Invalid(format!(
                        "unknown occupancy state {value}"
                    )));
                }
            };
            let fill = row
                .try_get::<Option<i16>, _>("fill_percent")?
                .map(|value| {
                    FillEstimate::new(value as u8).ok_or_else(|| {
                        StuffError::Invalid(format!("invalid fill estimate {value}"))
                    })
                })
                .transpose()?;
            Ok(Occupancy {
                location,
                state,
                fill,
            })
        })
        .collect()
}

pub(crate) async fn persist_snapshot(
    pool: &PgPool,
    snapshot: &Snapshot,
    topic: &str,
    expected_revision: u64,
) -> Result<()> {
    let next_revision = i64::try_from(snapshot.revision)
        .map_err(|_| StuffError::Invalid("revision exceeds PostgreSQL bigint".into()))?;
    let expected_revision = i64::try_from(expected_revision)
        .map_err(|_| StuffError::Invalid("revision exceeds PostgreSQL bigint".into()))?;
    let mut tx = pool.begin().await?;
    let guarded = sqlx::query(
        "UPDATE state_revision SET revision = $1, updated_at = clock_timestamp() \
         WHERE id = 1 AND revision = $2",
    )
    .bind(next_revision)
    .bind(expected_revision)
    .execute(&mut *tx)
    .await?;
    if guarded.rows_affected() != 1 {
        tx.rollback().await?;
        return Err(StuffError::Conflict);
    }

    clear_canonical_tables(&mut tx).await?;
    insert_nodes(&mut tx, &snapshot.nodes).await?;
    insert_organizer_templates(&mut tx, &snapshot.organizer_templates).await?;
    insert_rack_templates(&mut tx, &snapshot.rack_templates).await?;
    insert_drawer_templates(&mut tx, &snapshot.drawer_cabinet_templates).await?;
    insert_organizer_instances(&mut tx, &snapshot.organizers).await?;
    insert_rack_instances(&mut tx, &snapshot.rack_instances).await?;
    insert_drawer_instances(&mut tx, &snapshot.drawer_cabinet_instances).await?;
    insert_items(&mut tx, &snapshot.items).await?;
    let locations = insert_locations(&mut tx, snapshot).await?;
    insert_stock(&mut tx, &snapshot.stock, &locations).await?;
    insert_reservations(&mut tx, &snapshot.reservations, &locations).await?;
    insert_occupancies(&mut tx, &snapshot.occupancies, &locations).await?;

    let payload = format!("{}:{}", topic, snapshot.revision);
    sqlx::query("SELECT pg_notify('stuff_changes', $1)")
        .bind(payload)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn clear_canonical_tables(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    for statement in [
        "DELETE FROM occupancies",
        "DELETE FROM reservations",
        "DELETE FROM stock_lots",
        "DELETE FROM inventory_locations",
        "DELETE FROM item_tags",
        "DELETE FROM items",
        "DELETE FROM organizer_removed_dividers",
        "DELETE FROM organizer_instances",
        "DELETE FROM rack_instance_shelves",
        "DELETE FROM rack_instances",
        "DELETE FROM drawer_instance_nodes",
        "DELETE FROM drawer_cabinet_instances",
        "DELETE FROM organizer_template_dividers",
        "DELETE FROM organizer_template_cells",
        "DELETE FROM organizer_templates",
        "DELETE FROM rack_template_shelves",
        "DELETE FROM rack_templates",
        "DELETE FROM drawer_template_drawers",
        "DELETE FROM drawer_cabinet_templates",
        "DELETE FROM storage_nodes",
    ] {
        sqlx::query(statement).execute(&mut **tx).await?;
    }
    Ok(())
}

async fn insert_nodes(tx: &mut Transaction<'_, Postgres>, nodes: &[StorageNode]) -> Result<()> {
    for node in nodes {
        let (kind, custom_kind) = encode_storage_kind(&node.kind);
        let position = node.placement.map(|placement| placement.position);
        sqlx::query(
            "INSERT INTO storage_nodes \
             (id, name, kind, custom_kind, parent_id, position_x, position_y, position_z, \
              width, depth, height, metadata) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(node.id.0)
        .bind(&node.name)
        .bind(kind)
        .bind(custom_kind)
        .bind(node.parent_id.map(|id| id.0))
        .bind(position.map(|point| point.x))
        .bind(position.map(|point| point.y))
        .bind(position.map(|point| point.z))
        .bind(node.size.map(|size| size.width))
        .bind(node.size.map(|size| size.depth))
        .bind(node.size.map(|size| size.height))
        .bind(&node.metadata)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn insert_organizer_templates(
    tx: &mut Transaction<'_, Postgres>,
    templates: &[OrganizerTemplate],
) -> Result<()> {
    for template in templates {
        sqlx::query(
            "INSERT INTO organizer_templates (id,name,width,depth,height,metadata) \
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(template.id.0)
        .bind(&template.name)
        .bind(template.size.width)
        .bind(template.size.depth)
        .bind(template.size.height)
        .bind(&template.metadata)
        .execute(&mut **tx)
        .await?;
        for cell in &template.cells {
            sqlx::query(
                "INSERT INTO organizer_template_cells \
                 (template_id,row_no,column_no,x,y,width,depth) VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(template.id.0)
            .bind(i64::from(cell.id.row))
            .bind(i64::from(cell.id.column))
            .bind(cell.rect.x)
            .bind(cell.rect.y)
            .bind(cell.rect.width)
            .bind(cell.rect.depth)
            .execute(&mut **tx)
            .await?;
        }
        for divider in &template.dividers {
            let (a, b) = if divider.a <= divider.b {
                (divider.a, divider.b)
            } else {
                (divider.b, divider.a)
            };
            let axis = match divider.axis {
                DividerAxis::Vertical => "vertical",
                DividerAxis::Horizontal => "horizontal",
            };
            let kind = match divider.kind {
                DividerKind::Fixed => "fixed",
                DividerKind::Removable => "removable",
                DividerKind::Impossible => "impossible",
            };
            sqlx::query(
                "INSERT INTO organizer_template_dividers \
                 (template_id,a_row,a_column,b_row,b_column,axis,kind) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(template.id.0)
            .bind(i64::from(a.row))
            .bind(i64::from(a.column))
            .bind(i64::from(b.row))
            .bind(i64::from(b.column))
            .bind(axis)
            .bind(kind)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn insert_rack_templates(
    tx: &mut Transaction<'_, Postgres>,
    templates: &[RackTemplate],
) -> Result<()> {
    for template in templates {
        sqlx::query(
            "INSERT INTO rack_templates (id,name,width,depth,height,metadata) \
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(template.id.0)
        .bind(&template.name)
        .bind(template.outer_size.width)
        .bind(template.outer_size.depth)
        .bind(template.outer_size.height)
        .bind(&template.metadata)
        .execute(&mut **tx)
        .await?;
        for (ordinal, shelf) in template.shelves.iter().enumerate() {
            let ordinal = encode_ordinal(ordinal)?;
            sqlx::query(
                "INSERT INTO rack_template_shelves \
                 (template_id,ordinal,name,altitude,usable_height,metadata) \
                 VALUES ($1,$2,$3,$4,$5,$6)",
            )
            .bind(template.id.0)
            .bind(ordinal)
            .bind(&shelf.name)
            .bind(shelf.altitude)
            .bind(shelf.usable_height)
            .bind(&shelf.metadata)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn insert_drawer_templates(
    tx: &mut Transaction<'_, Postgres>,
    templates: &[DrawerCabinetTemplate],
) -> Result<()> {
    for template in templates {
        sqlx::query(
            "INSERT INTO drawer_cabinet_templates (id,name,width,depth,height,metadata) \
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(template.id.0)
        .bind(&template.name)
        .bind(template.outer_size.width)
        .bind(template.outer_size.depth)
        .bind(template.outer_size.height)
        .bind(&template.metadata)
        .execute(&mut **tx)
        .await?;
        for (ordinal, drawer) in template.drawers.iter().enumerate() {
            let ordinal = encode_ordinal(ordinal)?;
            sqlx::query(
                "INSERT INTO drawer_template_drawers \
                 (template_id,ordinal,name,altitude,usable_width,usable_depth,usable_height,metadata) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(template.id.0)
            .bind(ordinal)
            .bind(&drawer.name)
            .bind(drawer.altitude)
            .bind(drawer.usable_size.width)
            .bind(drawer.usable_size.depth)
            .bind(drawer.usable_size.height)
            .bind(&drawer.metadata)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn insert_organizer_instances(
    tx: &mut Transaction<'_, Postgres>,
    instances: &[OrganizerInstance],
) -> Result<()> {
    for instance in instances {
        sqlx::query("INSERT INTO organizer_instances (id,node_id,template_id) VALUES ($1,$2,$3)")
            .bind(instance.id.0)
            .bind(instance.node_id.0)
            .bind(instance.template_id.0)
            .execute(&mut **tx)
            .await?;
        for key in &instance.removed_dividers {
            sqlx::query(
                "INSERT INTO organizer_removed_dividers (organizer_id,divider_key) VALUES ($1,$2)",
            )
            .bind(instance.id.0)
            .bind(key)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn insert_rack_instances(
    tx: &mut Transaction<'_, Postgres>,
    instances: &[RackInstance],
) -> Result<()> {
    for instance in instances {
        sqlx::query("INSERT INTO rack_instances (rack_id,template_id) VALUES ($1,$2)")
            .bind(instance.rack_id.0)
            .bind(instance.template_id.0)
            .execute(&mut **tx)
            .await?;
        for (ordinal, shelf_id) in instance.shelf_ids.iter().enumerate() {
            let ordinal = encode_ordinal(ordinal)?;
            sqlx::query(
                "INSERT INTO rack_instance_shelves (rack_id,ordinal,shelf_id) VALUES ($1,$2,$3)",
            )
            .bind(instance.rack_id.0)
            .bind(ordinal)
            .bind(shelf_id.0)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn insert_drawer_instances(
    tx: &mut Transaction<'_, Postgres>,
    instances: &[DrawerCabinetInstance],
) -> Result<()> {
    for instance in instances {
        sqlx::query("INSERT INTO drawer_cabinet_instances (cabinet_id,template_id) VALUES ($1,$2)")
            .bind(instance.cabinet_id.0)
            .bind(instance.template_id.0)
            .execute(&mut **tx)
            .await?;
        for (ordinal, drawer_id) in instance.drawer_ids.iter().enumerate() {
            let ordinal = encode_ordinal(ordinal)?;
            sqlx::query(
                "INSERT INTO drawer_instance_nodes (cabinet_id,ordinal,drawer_id) VALUES ($1,$2,$3)",
            )
            .bind(instance.cabinet_id.0)
            .bind(ordinal)
            .bind(drawer_id.0)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn insert_items(tx: &mut Transaction<'_, Postgres>, items: &[Item]) -> Result<()> {
    for item in items {
        sqlx::query(
            "INSERT INTO items \
             (id,name,manufacturer,manufacturer_part_number,category,notes, \
              item_width,item_depth,item_height,metadata) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(item.id.0)
        .bind(&item.name)
        .bind(&item.manufacturer)
        .bind(&item.manufacturer_part_number)
        .bind(&item.category)
        .bind(&item.notes)
        .bind(item.size.map(|size| size.width))
        .bind(item.size.map(|size| size.depth))
        .bind(item.size.map(|size| size.height))
        .bind(&item.metadata)
        .execute(&mut **tx)
        .await?;
        for (ordinal, tag) in item.tags.iter().enumerate() {
            let ordinal = encode_ordinal(ordinal)?;
            sqlx::query("INSERT INTO item_tags (item_id,ordinal,tag) VALUES ($1,$2,$3)")
                .bind(item.id.0)
                .bind(ordinal)
                .bind(tag)
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}

async fn insert_locations(
    tx: &mut Transaction<'_, Postgres>,
    snapshot: &Snapshot,
) -> Result<BTreeMap<LocationKey, Uuid>> {
    let stock = snapshot
        .stock
        .iter()
        .map(|lot| stock_location_key(&lot.location));
    let reservations = snapshot
        .reservations
        .iter()
        .map(|reservation| location_ref_key(&reservation.location));
    let occupancies = snapshot
        .occupancies
        .iter()
        .map(|occupancy| location_ref_key(&occupancy.location));
    let keys = stock
        .chain(reservations)
        .chain(occupancies)
        .collect::<BTreeSet<_>>();
    let mut locations = BTreeMap::new();
    for (node_id, organizer_id, compartment_key) in keys {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO inventory_locations (id,node_id,organizer_id,compartment_key) \
             VALUES ($1,$2,$3,$4)",
        )
        .bind(id)
        .bind(node_id.0)
        .bind(organizer_id.map(|value| value.0))
        .bind(&compartment_key)
        .execute(&mut **tx)
        .await?;
        locations.insert((node_id, organizer_id, compartment_key), id);
    }
    Ok(locations)
}

async fn insert_stock(
    tx: &mut Transaction<'_, Postgres>,
    lots: &[StockLot],
    locations: &BTreeMap<LocationKey, Uuid>,
) -> Result<()> {
    for lot in lots {
        let key = stock_location_key(&lot.location);
        let location_id = locations
            .get(&key)
            .ok_or_else(|| StuffError::Invalid("stock location was not materialized".into()))?;
        let position = lot.placement.map(|placement| placement.position);
        sqlx::query(
            "INSERT INTO stock_lots \
             (id,item_id,location_id,quantity,position_x,position_y,position_z) \
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(lot.id.0)
        .bind(lot.item_id.0)
        .bind(location_id)
        .bind(lot.quantity)
        .bind(position.map(|point| point.x))
        .bind(position.map(|point| point.y))
        .bind(position.map(|point| point.z))
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn insert_reservations(
    tx: &mut Transaction<'_, Postgres>,
    reservations: &[Reservation],
    locations: &BTreeMap<LocationKey, Uuid>,
) -> Result<()> {
    for reservation in reservations {
        let location_id = locations
            .get(&location_ref_key(&reservation.location))
            .ok_or_else(|| {
                StuffError::Invalid("reservation location was not materialized".into())
            })?;
        sqlx::query("INSERT INTO reservations (id,location_id,label,notes) VALUES ($1,$2,$3,$4)")
            .bind(reservation.id.0)
            .bind(location_id)
            .bind(&reservation.label)
            .bind(&reservation.notes)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn insert_occupancies(
    tx: &mut Transaction<'_, Postgres>,
    occupancies: &[Occupancy],
    locations: &BTreeMap<LocationKey, Uuid>,
) -> Result<()> {
    for occupancy in occupancies {
        let location_id = locations
            .get(&location_ref_key(&occupancy.location))
            .ok_or_else(|| StuffError::Invalid("occupancy location was not materialized".into()))?;
        let state = match occupancy.state {
            OccupancyState::Free => "free",
            OccupancyState::Occupied => "occupied",
            OccupancyState::Reserved => "reserved",
            OccupancyState::Blocked => "blocked",
        };
        sqlx::query("INSERT INTO occupancies (location_id,state,fill_percent) VALUES ($1,$2,$3)")
            .bind(location_id)
            .bind(state)
            .bind(occupancy.fill.map(|fill| i16::from(fill.percent())))
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

fn stock_location_key(location: &StockLocation) -> LocationKey {
    (
        location.node_id,
        location
            .compartment
            .as_ref()
            .map(|compartment| compartment.organizer_id),
        location
            .compartment
            .as_ref()
            .map(|compartment| compartment.key.clone()),
    )
}

fn location_ref_key(location: &LocationRef) -> LocationKey {
    (
        location.node_id,
        location.organizer_id,
        location.compartment_key.clone(),
    )
}

fn decode_cell_coordinate(value: i64, field: &str) -> Result<u32> {
    u32::try_from(value)
        .map_err(|_| StuffError::Invalid(format!("{field} is outside the supported u32 range")))
}

fn encode_ordinal(value: usize) -> Result<i32> {
    i32::try_from(value)
        .map_err(|_| StuffError::Invalid("ordered collection exceeds PostgreSQL integer".into()))
}

fn encode_storage_kind(kind: &StorageKind) -> (&str, Option<&str>) {
    match kind {
        StorageKind::Room => ("room", None),
        StorageKind::Rack => ("rack", None),
        StorageKind::Shelf => ("shelf", None),
        StorageKind::Drawer => ("drawer", None),
        StorageKind::Fixture => ("fixture", None),
        StorageKind::CellLessOrganizer => ("cell_less_organizer", None),
        StorageKind::Organizer => ("organizer", None),
        StorageKind::Box => ("box", None),
        StorageKind::Bin => ("bin", None),
        StorageKind::Loose => ("loose", None),
        StorageKind::Custom(value) => ("custom", Some(value)),
    }
}

fn decode_storage_kind(kind: &str, custom: Option<String>) -> Result<StorageKind> {
    Ok(match kind {
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
        "custom" => StorageKind::Custom(
            custom.ok_or_else(|| StuffError::Invalid("custom storage kind has no value".into()))?,
        ),
        value => return Err(StuffError::Invalid(format!("unknown storage kind {value}"))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::{ChangeTopic, StuffService, demo_snapshot};

    fn sort_snapshot(snapshot: &mut Snapshot) {
        snapshot.nodes.sort_by_key(|node| node.id);
        for template in &mut snapshot.organizer_templates {
            template.cells.sort_by_key(|cell| cell.id);
            for divider in &mut template.dividers {
                if divider.a > divider.b {
                    std::mem::swap(&mut divider.a, &mut divider.b);
                }
            }
            template.dividers.sort_by_key(|divider| {
                (
                    divider.a.row,
                    divider.a.column,
                    divider.b.row,
                    divider.b.column,
                )
            });
        }
        snapshot
            .organizer_templates
            .sort_by_key(|template| template.id);
        snapshot.rack_templates.sort_by_key(|template| template.id);
        snapshot
            .rack_instances
            .sort_by_key(|instance| instance.rack_id);
        snapshot
            .drawer_cabinet_templates
            .sort_by_key(|template| template.id);
        snapshot
            .drawer_cabinet_instances
            .sort_by_key(|instance| instance.cabinet_id);
        snapshot.organizers.sort_by_key(|organizer| organizer.id);
        snapshot.items.sort_by_key(|item| item.id);
        snapshot.stock.sort_by_key(|lot| lot.id);
        snapshot
            .reservations
            .sort_by_key(|reservation| reservation.id);
        snapshot.occupancies.sort_by_key(|occupancy| {
            (
                occupancy.location.node_id,
                occupancy.location.organizer_id,
                occupancy.location.compartment_key.clone(),
            )
        });
    }

    #[test]
    fn every_storage_kind_has_a_lossless_database_encoding() {
        let kinds = [
            StorageKind::Room,
            StorageKind::Rack,
            StorageKind::Shelf,
            StorageKind::Drawer,
            StorageKind::Fixture,
            StorageKind::CellLessOrganizer,
            StorageKind::Organizer,
            StorageKind::Box,
            StorageKind::Bin,
            StorageKind::Loose,
            StorageKind::Custom("future_kind".into()),
        ];
        for kind in kinds {
            let (name, custom) = encode_storage_kind(&kind);
            assert_eq!(
                decode_storage_kind(name, custom.map(str::to_owned)).unwrap(),
                kind
            );
        }
    }

    #[test]
    fn initial_migration_is_normalized_and_has_no_snapshot_blob() {
        let migration = include_str!("../migrations/0001_initial.sql");
        assert!(migration.contains("CREATE TABLE storage_nodes"));
        assert!(migration.contains("CREATE TABLE inventory_locations"));
        assert!(migration.contains("CREATE TABLE drawer_cabinet_templates"));
        assert!(!migration.contains("state jsonb NOT NULL"));
        assert!(!migration.contains("snapshot_state"));
    }

    /// Set this only to a disposable, empty PostgreSQL database. The test is
    /// opt-in so normal unit tests never mutate a developer database.
    #[tokio::test]
    async fn postgres_round_trip_and_stale_writer_guard() {
        let Ok(database_url) = std::env::var("STUFF_CORE_TEST_DATABASE_URL") else {
            return;
        };
        StuffService::migrate(&database_url).await.unwrap();
        let first = StuffService::connect(database_url.clone()).await.unwrap();
        let second = StuffService::connect(database_url.clone()).await.unwrap();
        let mut listener = first.listen().await.unwrap();
        let node_id = first
            .create_node(StorageNode {
                id: NodeId::new(),
                name: format!("Database test {}", Uuid::new_v4()),
                kind: StorageKind::Room,
                parent_id: None,
                placement: None,
                size: Some(Size3 {
                    width: 1000,
                    depth: 1000,
                    height: 1000,
                }),
                metadata: serde_json::json!({"test": true}),
            })
            .await
            .unwrap();
        let event = listener.next().await.unwrap();
        assert_eq!(event.topic, ChangeTopic::Storage);
        assert_eq!(event.revision, first.snapshot().await.revision);
        let drawer_template_id = first
            .create_drawer_cabinet_template(DrawerCabinetTemplate {
                id: DrawerCabinetTemplateId::default(),
                name: "Database drawer type".into(),
                outer_size: Size3 {
                    width: 300,
                    depth: 200,
                    height: 100,
                },
                drawers: vec![DrawerTemplateDrawer {
                    name: "Drawer".into(),
                    altitude: 0,
                    usable_size: Size3 {
                        width: 290,
                        depth: 190,
                        height: 90,
                    },
                    metadata: serde_json::Value::Null,
                }],
                metadata: serde_json::json!({"source": "database-test"}),
            })
            .await
            .unwrap();
        let cabinet_id = first
            .create_drawer_cabinet_from_template(
                crate::service::CreateDrawerCabinetFromTemplateSpec {
                    cabinet_name: "Database cabinet".into(),
                    parent_id: node_id,
                    template_id: drawer_template_id,
                    placement: None,
                    metadata: serde_json::Value::Null,
                },
            )
            .await
            .unwrap();
        let drawer_id = first
            .snapshot()
            .await
            .drawer_cabinet_instances
            .iter()
            .find(|instance| instance.cabinet_id == cabinet_id)
            .unwrap()
            .drawer_ids[0];
        let (item_id, stock_id) = first
            .create_item_with_stock(
                Item {
                    id: ItemId::default(),
                    name: "Database item".into(),
                    manufacturer: Some("Stuff".into()),
                    manufacturer_part_number: None,
                    category: Some("test".into()),
                    notes: None,
                    tags: vec!["normalized".into()],
                    size: None,
                    metadata: serde_json::Value::Null,
                },
                StockLocation {
                    node_id: drawer_id,
                    compartment: None,
                },
                7,
            )
            .await
            .unwrap();
        let stale = second
            .create_node(StorageNode {
                id: NodeId::new(),
                name: "Stale write".into(),
                kind: StorageKind::Room,
                parent_id: None,
                placement: None,
                size: Some(Size3 {
                    width: 1000,
                    depth: 1000,
                    height: 1000,
                }),
                metadata: serde_json::Value::Null,
            })
            .await;
        assert!(matches!(stale, Err(StuffError::Conflict)));
        let refreshed = second.refresh().await.unwrap();
        assert!(refreshed.nodes.iter().any(|node| node.id == node_id));
        assert!(
            refreshed
                .drawer_cabinet_templates
                .iter()
                .any(|template| template.id == drawer_template_id)
        );
        assert!(refreshed.stock.iter().any(|lot| {
            lot.item_id == item_id && lot.location.node_id == drawer_id && lot.quantity == 7
        }));

        first.adjust_stock(stock_id, -7).await.unwrap();
        let refreshed = second.refresh().await.unwrap();
        assert!(!refreshed.stock.iter().any(|lot| lot.id == stock_id));

        let pool = PgPool::connect(&database_url).await.unwrap();
        let current = load_snapshot(&pool).await.unwrap().unwrap_or_default();
        let mut expected = demo_snapshot();
        let room_id = expected
            .nodes
            .iter()
            .find(|node| node.kind == StorageKind::Room)
            .unwrap()
            .id;
        let shelf_id = expected
            .nodes
            .iter()
            .find(|node| node.kind == StorageKind::Shelf)
            .unwrap()
            .id;
        let cell_less_id = NodeId::new();
        expected.nodes.push(StorageNode {
            id: cell_less_id,
            name: "Cell-less organizer".into(),
            kind: StorageKind::CellLessOrganizer,
            parent_id: Some(shelf_id),
            placement: Some(Placement {
                position: Point3 { x: 0, y: 0, z: 0 },
            }),
            size: Some(Size3 {
                width: 200,
                depth: 120,
                height: 40,
            }),
            metadata: serde_json::Value::Null,
        });
        expected.stock.push(StockLot {
            id: StockLotId::new(),
            item_id: expected.items[0].id,
            location: StockLocation {
                node_id: room_id,
                compartment: None,
            },
            quantity: 1,
            placement: Some(Placement {
                position: Point3 {
                    x: 100,
                    y: 100,
                    z: 0,
                },
            }),
        });
        expected.stock.push(StockLot {
            id: StockLotId::new(),
            item_id: expected.items[1].id,
            location: StockLocation {
                node_id: cell_less_id,
                compartment: None,
            },
            quantity: 2,
            placement: None,
        });
        expected.organizer_templates.push(OrganizerTemplate {
            id: TemplateId::new(),
            name: "Maximum cell coordinates".into(),
            size: Size3 {
                width: 10,
                depth: 10,
                height: 10,
            },
            cells: vec![PrimitiveCell {
                id: CellId {
                    row: u32::MAX,
                    column: u32::MAX,
                },
                rect: Rect {
                    x: 0,
                    y: 0,
                    width: 10,
                    depth: 10,
                },
            }],
            dividers: Vec::new(),
            metadata: serde_json::Value::Null,
        });
        expected.revision = current.revision.checked_add(1).unwrap();
        persist_snapshot(&pool, &expected, "all", current.revision)
            .await
            .unwrap();
        let mut actual = load_snapshot(&pool).await.unwrap().unwrap();
        sort_snapshot(&mut expected);
        sort_snapshot(&mut actual);
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
}
