//! Async application facade. The in-memory constructor is intentionally useful
//! for previews and tests; connected instances use PostgreSQL as the shared store.

use crate::{domain::*, drawer::*, errors::*, inventory::*, occupancy::*, organizer::*, rack::*};
use serde::{Deserialize, Serialize};
use sqlx::{
    PgPool,
    postgres::{PgListener, PgPoolOptions},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tokio::sync::{Mutex, RwLock, broadcast};

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChangeTopic {
    Storage,
    Inventory,
    Organizer,
    Occupancy,
    All,
}
impl ChangeTopic {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Storage => "storage",
            Self::Inventory => "inventory",
            Self::Organizer => "organizer",
            Self::Occupancy => "occupancy",
            Self::All => "all",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangeEvent {
    pub topic: ChangeTopic,
    pub revision: u64,
}

pub struct ChangeListener {
    listener: PgListener,
}
impl ChangeListener {
    pub async fn next(&mut self) -> Result<ChangeEvent> {
        let notification = self.listener.recv().await.map_err(StuffError::Database)?;
        let mut fields = notification.payload().splitn(2, ':');
        let topic = match fields.next().unwrap_or("all") {
            "storage" => ChangeTopic::Storage,
            "inventory" => ChangeTopic::Inventory,
            "organizer" => ChangeTopic::Organizer,
            "occupancy" => ChangeTopic::Occupancy,
            _ => ChangeTopic::All,
        };
        let revision = fields
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or_default();
        Ok(ChangeEvent { topic, revision })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TreeNode {
    pub node: StorageNode,
    pub children: Vec<TreeNode>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Contents {
    pub node: StorageNode,
    pub children: Vec<StorageNode>,
    pub stock: Vec<StockLot>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentsEntry {
    pub item_id: ItemId,
    pub item_name: String,
    pub quantity: i64,
    pub location: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocationPath {
    pub node_id: NodeId,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectorMatch {
    pub id: NodeId,
    pub path: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocationSelectorMatch {
    pub location: StockLocation,
    pub path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemSelectorMatch {
    pub id: ItemId,
    pub name: String,
    pub manufacturer_part_number: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemSelectorCandidate {
    pub id: ItemId,
    pub name: String,
    pub manufacturer_part_number: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct FreeLocationFilter {
    pub completely_free: bool,
    pub max_fill: Option<u8>,
    pub reserved_empty: bool,
}
impl FreeLocationFilter {
    pub fn validate(self) -> Result<Self> {
        if self.max_fill.is_some_and(|fill| fill > 100) {
            Err(StuffError::Validation(
                "max fill must be between 0 and 100".into(),
            ))
        } else {
            Ok(self)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FreeLocation {
    pub location: LocationRef,
    pub path: String,
    pub state: OccupancyState,
    pub fill_percent: Option<u8>,
    pub reserved_for: Option<String>,
}

/// Typed shelf construction. Rack shelves inherit their usable footprint from
/// the rack; room shelves use the caller's explicit footprint and placement.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CreateShelfSpec {
    InRack {
        name: String,
        rack_id: NodeId,
        altitude: Mm,
        usable_height: Mm,
        metadata: serde_json::Value,
    },
    InRoom {
        name: String,
        room_id: NodeId,
        usable_size: Size3,
        position: Point3,
        metadata: serde_json::Value,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FreshOrganizerSpec {
    pub template_name: String,
    pub organizer_name: String,
    pub parent_id: NodeId,
    pub outer_size: Size3,
    pub cells: Vec<PrimitiveCell>,
    pub placement: Option<Placement>,
    pub template_metadata: serde_json::Value,
    pub organizer_metadata: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CloneOrganizerSpec {
    pub source_template_id: TemplateId,
    pub template_name: String,
    pub organizer_name: String,
    pub parent_id: NodeId,
    pub outer_size: Size3,
    pub placement: Option<Placement>,
    pub template_metadata: serde_json::Value,
    pub organizer_metadata: serde_json::Value,
}

/// Creates a new organizer instance from an existing reusable template type.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateOrganizerFromTemplateSpec {
    pub organizer_name: String,
    pub parent_id: NodeId,
    pub template_id: TemplateId,
    pub placement: Option<Placement>,
    pub metadata: serde_json::Value,
}

/// Copies a reusable template type without creating an organizer instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CloneOrganizerTemplateSpec {
    pub source_template_id: TemplateId,
    pub name: String,
    pub outer_size: Size3,
    pub metadata: serde_json::Value,
}

/// Lightweight data needed to populate an organizer "clone previous" picker.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrganizerLayoutSummary {
    pub template_id: TemplateId,
    pub name: String,
    pub outer_size: Size3,
    pub cell_count: usize,
    pub cells: Vec<PrimitiveCell>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddOrCreateItemSpec {
    pub item: Item,
    pub location: StockLocation,
    pub quantity: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddOrCreateItemResult {
    pub item_id: ItemId,
    pub stock_lot_id: StockLotId,
    pub created_item: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CloneRackTemplateSpec {
    pub source_template_id: RackTemplateId,
    pub name: String,
    pub outer_size: Size3,
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateRackFromTemplateSpec {
    pub rack_name: String,
    pub room_id: NodeId,
    pub template_id: RackTemplateId,
    pub placement: Option<Placement>,
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CloneDrawerCabinetTemplateSpec {
    pub source_template_id: DrawerCabinetTemplateId,
    pub name: String,
    pub outer_size: Size3,
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateDrawerCabinetFromTemplateSpec {
    pub cabinet_name: String,
    pub parent_id: NodeId,
    pub template_id: DrawerCabinetTemplateId,
    pub placement: Option<Placement>,
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveRackAsTemplateSpec {
    pub rack_id: NodeId,
    pub template_name: String,
    pub metadata: serde_json::Value,
}

/// Captures an existing physical organizer as a reusable type and makes that
/// organizer use the newly-created type immediately.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveOrganizerAsTemplateSpec {
    pub organizer_node_id: NodeId,
    pub template_name: String,
    pub metadata: serde_json::Value,
}

pub fn display_location(snapshot: &Snapshot, location: &StockLocation) -> Result<String> {
    let mut path = path_for(&snapshot.nodes, location.node_id)?;
    if let Some(compartment) = &location.compartment {
        let organizer = snapshot
            .organizers
            .iter()
            .find(|o| o.id == compartment.organizer_id)
            .ok_or_else(|| StuffError::NotFound(compartment.organizer_id.to_string()))?;
        if organizer.node_id != location.node_id {
            path = path_for(&snapshot.nodes, organizer.node_id)?;
        }
        path.push_str(" / ");
        path.push_str(&compartment.key);
    }
    Ok(path)
}

#[derive(Clone)]
pub struct StuffService {
    pool: Option<PgPool>,
    database_url: Option<String>,
    state: Arc<RwLock<Snapshot>>,
    changes: broadcast::Sender<ChangeEvent>,
    write_lock: Arc<Mutex<()>>,
}

impl StuffService {
    pub fn demo() -> Self {
        let (changes, _) = broadcast::channel(64);
        let snapshot = demo_snapshot();
        Self {
            pool: None,
            database_url: None,
            state: Arc::new(RwLock::new(snapshot)),
            changes,
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    pub async fn connect(database_url: impl Into<String>) -> Result<Self> {
        let database_url = database_url.into();
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(&database_url)
            .await?;
        MIGRATOR.run(&pool).await?;
        let snapshot = crate::persistence::load_snapshot(&pool)
            .await?
            .unwrap_or_default();
        let (changes, _) = broadcast::channel(128);
        Ok(Self {
            pool: Some(pool),
            database_url: Some(database_url),
            state: Arc::new(RwLock::new(snapshot)),
            changes,
            write_lock: Arc::new(Mutex::new(())),
        })
    }

    pub async fn migrate(database_url: &str) -> Result<()> {
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(database_url)
            .await?;
        MIGRATOR.run(&pool).await?;
        Ok(())
    }
    pub fn subscribe(&self) -> broadcast::Receiver<ChangeEvent> {
        self.changes.subscribe()
    }
    pub async fn listen(&self) -> Result<ChangeListener> {
        let url = self
            .database_url
            .as_ref()
            .ok_or_else(|| StuffError::Invalid("demo service has no database listener".into()))?;
        let mut listener = PgListener::connect(url).await?;
        listener.listen("stuff_changes").await?;
        Ok(ChangeListener { listener })
    }
    pub async fn snapshot(&self) -> Snapshot {
        self.state.read().await.clone()
    }
    /// Refetches the canonical snapshot after a notification from another client.
    pub async fn refresh(&self) -> Result<Snapshot> {
        let _guard = self.write_lock.lock().await;
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| StuffError::Invalid("demo service has no database to refresh".into()))?;
        let snapshot = crate::persistence::load_snapshot(pool)
            .await?
            .unwrap_or_default();
        *self.state.write().await = snapshot.clone();
        Ok(snapshot)
    }

    async fn commit(&self, mut next: Snapshot, topic: ChangeTopic) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        let old = self.state.read().await.clone();
        if next.revision != old.revision {
            return Err(StuffError::Conflict);
        }
        let revision = old.revision + 1;
        next.revision = revision;
        if let Some(pool) = &self.pool {
            crate::persistence::persist_snapshot(pool, &next, topic.as_str(), old.revision).await?;
        }
        *self.state.write().await = next;
        let event = ChangeEvent { topic, revision };
        let _ = self.changes.send(event.clone());
        Ok(())
    }

    pub async fn tree(&self, root: Option<NodeId>) -> Vec<TreeNode> {
        let snapshot = self.state.read().await;
        fn build(nodes: &[StorageNode], parent: Option<NodeId>) -> Vec<TreeNode> {
            nodes
                .iter()
                .filter(|n| n.parent_id == parent)
                .map(|node| TreeNode {
                    node: node.clone(),
                    children: build(nodes, Some(node.id)),
                })
                .collect()
        }
        if let Some(root_id) = root {
            snapshot
                .nodes
                .iter()
                .find(|n| n.id == root_id)
                .map(|node| {
                    vec![TreeNode {
                        node: node.clone(),
                        children: build(&snapshot.nodes, Some(node.id)),
                    }]
                })
                .unwrap_or_default()
        } else {
            build(&snapshot.nodes, None)
        }
    }
    pub async fn path(&self, node_id: NodeId) -> Result<String> {
        let snapshot = self.state.read().await;
        path_for(&snapshot.nodes, node_id)
    }
    pub async fn resolve_selector(&self, selector: &str) -> Result<SelectorMatch> {
        let snapshot = self.state.read().await;
        resolve_selector(&snapshot.nodes, selector)
    }
    pub async fn resolve_item_selector(&self, selector: &str) -> Result<ItemSelectorMatch> {
        let snapshot = self.state.read().await;
        resolve_item_selector(&snapshot.items, selector)
    }
    pub async fn resolve_location_selector(&self, selector: &str) -> Result<LocationSelectorMatch> {
        let snapshot = self.state.read().await;
        resolve_location_selector(&snapshot, selector)
    }
    pub async fn contents(&self, node_id: NodeId) -> Result<Contents> {
        let snapshot = self.state.read().await;
        let node = snapshot
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .cloned()
            .ok_or_else(|| StuffError::NotFound(node_id.to_string()))?;
        Ok(Contents {
            node,
            children: snapshot
                .nodes
                .iter()
                .filter(|n| n.parent_id == Some(node_id))
                .cloned()
                .collect(),
            stock: snapshot
                .stock
                .iter()
                .filter(|s| s.location.node_id == node_id)
                .cloned()
                .collect(),
        })
    }
    pub async fn contents_by_selector(&self, selector: &str) -> Result<Vec<ContentsEntry>> {
        let snapshot = self.state.read().await;
        let selected = resolve_location_selector(&snapshot, selector)?;
        let item_by_id: BTreeMap<_, _> = snapshot.items.iter().map(|i| (i.id, i)).collect();
        Ok(snapshot
            .stock
            .iter()
            .filter(|lot| {
                lot.quantity > 0
                    && if selected.location.compartment.is_some() {
                        lot.location == selected.location
                    } else {
                        lot.location.node_id == selected.location.node_id
                    }
            })
            .filter_map(|lot| {
                item_by_id.get(&lot.item_id).map(|item| ContentsEntry {
                    item_id: item.id,
                    item_name: item.name.clone(),
                    quantity: lot.quantity,
                    location: display_location(&snapshot, &lot.location)
                        .unwrap_or_else(|_| selected.path.clone()),
                })
            })
            .collect())
    }
    pub async fn free_locations(&self, include_reserved_empty: bool) -> Vec<LocationRef> {
        self.free_locations_filtered(FreeLocationFilter {
            reserved_empty: include_reserved_empty,
            ..FreeLocationFilter::default()
        })
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|l| l.location)
        .collect()
    }
    pub async fn free_locations_filtered(
        &self,
        filter: FreeLocationFilter,
    ) -> Result<Vec<FreeLocation>> {
        let filter = filter.validate()?;
        let s = self.state.read().await;
        let mut locations = Vec::new();
        for node in &s.nodes {
            if !node.kind.accepts_inventory()
                || s.organizers
                    .iter()
                    .any(|organizer| organizer.node_id == node.id)
            {
                continue;
            }
            locations.push(LocationRef {
                node_id: node.id,
                organizer_id: None,
                compartment_key: None,
            });
        }
        for organizer in &s.organizers {
            if let Some(template) = s
                .organizer_templates
                .iter()
                .find(|t| t.id == organizer.template_id)
            {
                for compartment in organizer.compartments(template) {
                    locations.push(LocationRef {
                        node_id: organizer.node_id,
                        organizer_id: Some(organizer.id),
                        compartment_key: Some(compartment.key),
                    });
                }
            }
        }
        Ok(locations
            .into_iter()
            .filter_map(|location| {
                let occupied = s
                    .stock
                    .iter()
                    .any(|lot| lot.quantity > 0 && stock_matches_ref(lot, &location));
                let reservation = s
                    .reservations
                    .iter()
                    .find(|r| location_matches_ref(&r.location, &location));
                let explicit = s
                    .occupancies
                    .iter()
                    .find(|o| location_matches_ref(&o.location, &location));
                let fill_percent = explicit.and_then(|o| o.fill.map(fill_percent));
                let explicit_state = explicit.map(|o| o.state);
                let state = if occupied && explicit_state == Some(OccupancyState::Free) {
                    OccupancyState::Occupied
                } else {
                    explicit_state.unwrap_or(if reservation.is_some() {
                        OccupancyState::Reserved
                    } else if occupied {
                        OccupancyState::Occupied
                    } else {
                        OccupancyState::Free
                    })
                };
                if state == OccupancyState::Blocked || fill_percent == Some(100) {
                    return None;
                }
                if filter.completely_free
                    && (occupied || reservation.is_some() || fill_percent.unwrap_or(0) != 0)
                {
                    return None;
                }
                if filter.reserved_empty && (reservation.is_none() || occupied) {
                    return None;
                }
                if let Some(max) = filter.max_fill
                    && fill_percent.unwrap_or(if occupied { 100 } else { 0 }) > max
                {
                    return None;
                }
                Some(FreeLocation {
                    path: display_location(
                        &s,
                        &StockLocation {
                            node_id: location.node_id,
                            compartment: location
                                .organizer_id
                                .zip(location.compartment_key.clone())
                                .map(|(organizer_id, key)| CompartmentRef { organizer_id, key }),
                        },
                    )
                    .unwrap_or_else(|_| "Unknown location".into()),
                    location,
                    state,
                    fill_percent,
                    reserved_for: reservation.map(|r| r.label.clone()),
                })
            })
            .collect())
    }

    pub async fn search(&self, query: &str) -> SearchResults {
        let s = self.state.read().await;
        let q = query.to_ascii_lowercase().replace(' ', "");
        let mut items = Vec::new();
        let mut locations = Vec::new();
        for item in &s.items {
            let hay = format!(
                "{} {} {} {} {:?}",
                item.name,
                item.manufacturer.as_deref().unwrap_or(""),
                item.manufacturer_part_number.as_deref().unwrap_or(""),
                item.category.as_deref().unwrap_or(""),
                item.tags
            )
            .to_ascii_lowercase()
            .replace(' ', "");
            if hay.contains(&q) {
                let lots: Vec<_> = s
                    .stock
                    .iter()
                    .filter(|lot| lot.item_id == item.id && lot.quantity > 0)
                    .cloned()
                    .collect();
                for lot in &lots {
                    locations.push(StockLocationResult {
                        item_id: item.id,
                        item_name: item.name.clone(),
                        quantity: lot.quantity,
                        location: display_location(&s, &lot.location)
                            .unwrap_or_else(|_| "Unknown location".into()),
                    });
                }
                items.push(ItemMatch {
                    item: item.clone(),
                    total_quantity: lots.iter().map(|l| l.quantity).sum(),
                    lots,
                });
            }
        }
        SearchResults { items, locations }
    }

    pub async fn create_node(&self, mut node: StorageNode) -> Result<NodeId> {
        validate_name(&node.name, "storage node")?;
        validate_storage_kind(&node.kind)?;
        if node.kind == StorageKind::Organizer {
            return Err(StuffError::Validation(
                "typed organizers must be created from an organizer type".into(),
            ));
        }
        node.id = if node.id.0.is_nil() {
            NodeId::new()
        } else {
            node.id
        };
        let mut next = self.snapshot().await;
        if next.nodes.iter().any(|existing| existing.id == node.id) {
            return Err(StuffError::Invalid("storage node id already exists".into()));
        }
        validate_containment(&next.nodes, &node)?;
        validate_size(node.size)?;
        let id = node.id;
        next.nodes.push(node);
        self.commit(next, ChangeTopic::Storage).await?;
        Ok(id)
    }
    pub async fn update_node(&self, node: StorageNode) -> Result<()> {
        validate_name(&node.name, "storage node")?;
        validate_storage_kind(&node.kind)?;
        let mut next = self.snapshot().await;
        if !next.nodes.iter().any(|existing| existing.id == node.id) {
            return Err(StuffError::NotFound(node.id.to_string()));
        }
        if node.kind == StorageKind::Organizer
            && next
                .nodes
                .iter()
                .find(|existing| existing.id == node.id)
                .is_some_and(|existing| existing.kind != StorageKind::Organizer)
        {
            return Err(StuffError::Validation(
                "use an organizer type to convert this storage object into an organizer".into(),
            ));
        }
        if next
            .organizers
            .iter()
            .any(|organizer| organizer.node_id == node.id)
            && node.kind != StorageKind::Organizer
        {
            if node.kind != StorageKind::CellLessOrganizer {
                return Err(StuffError::Validation(
                    "a typed organizer can only be converted to a cell-less organizer".into(),
                ));
            }
            if has_location_state(&next, node.id) {
                return Err(StuffError::Validation(
                    "clear organizer stock, reservations, and occupancy before removing its type"
                        .into(),
                ));
            }
            next.organizers
                .retain(|organizer| organizer.node_id != node.id);
        }
        if next
            .rack_instances
            .iter()
            .any(|instance| instance.rack_id == node.id)
            && node.kind != StorageKind::Rack
        {
            return Err(StuffError::Validation(
                "a rack instance must keep a rack storage node".into(),
            ));
        }
        if next
            .drawer_cabinet_instances
            .iter()
            .any(|instance| instance.cabinet_id == node.id)
            && node.kind != StorageKind::Custom(DRAWER_CABINET_KIND.into())
        {
            return Err(StuffError::Validation(
                "a drawer cabinet instance must keep a drawer cabinet storage node".into(),
            ));
        }
        if let Some((parent_id, kind)) = generated_node_parent(&next, node.id)
            && (node.parent_id != Some(parent_id) || node.kind != kind)
        {
            return Err(StuffError::Validation(
                "generated rack shelves and cabinet drawers must keep their parent and kind".into(),
            ));
        }
        if let Some(parent) = node.parent_id
            && (parent == node.id || is_descendant(&next.nodes, parent, node.id))
        {
            return Err(StuffError::Invalid(
                "cannot move a node into itself or its descendant".into(),
            ));
        }
        validate_containment(&next.nodes, &node)?;
        if next
            .nodes
            .iter()
            .filter(|child| child.parent_id == Some(node.id))
            .any(|child| !is_allowed_child_kind(Some(&node.kind), &child.kind))
        {
            return Err(StuffError::Validation(
                "updated storage kind cannot contain one or more existing children".into(),
            ));
        }
        let was_spatial = next
            .nodes
            .iter()
            .find(|existing| existing.id == node.id)
            .is_some_and(|existing| existing.kind.has_spatial_stock());
        if !was_spatial
            && node.kind.has_spatial_stock()
            && next
                .stock
                .iter()
                .any(|lot| lot.quantity > 0 && lot.location.node_id == node.id)
        {
            return Err(StuffError::Validation(
                "clear direct stock before changing this object into a spatial stock workspace"
                    .into(),
            ));
        }
        if was_spatial && !node.kind.has_spatial_stock() {
            for lot in next
                .stock
                .iter_mut()
                .filter(|lot| lot.location.node_id == node.id)
            {
                lot.placement = None;
            }
        }
        validate_size(node.size)?;
        let slot = next
            .nodes
            .iter_mut()
            .find(|n| n.id == node.id)
            .ok_or_else(|| StuffError::NotFound(node.id.to_string()))?;
        *slot = node;
        self.commit(next, ChangeTopic::Storage).await
    }
    pub async fn move_node(
        &self,
        id: NodeId,
        parent_id: Option<NodeId>,
        placement: Option<Placement>,
    ) -> Result<()> {
        let mut next = self.snapshot().await;
        if let Some(parent) = parent_id
            && (parent == id || is_descendant(&next.nodes, parent, id))
        {
            return Err(StuffError::Invalid(
                "cannot move a node into itself or its descendant".into(),
            ));
        }
        let index = next
            .nodes
            .iter()
            .position(|n| n.id == id)
            .ok_or_else(|| StuffError::NotFound(id.to_string()))?;
        if let Some((generated_parent, _)) = generated_node_parent(&next, id)
            && parent_id != Some(generated_parent)
        {
            return Err(StuffError::Validation(
                "generated rack shelves and cabinet drawers cannot be moved out of their instance"
                    .into(),
            ));
        }
        let mut proposed = next.nodes[index].clone();
        proposed.parent_id = parent_id;
        proposed.placement = placement
            .or_else(|| first_available_placement(&next.nodes, parent_id, id, proposed.size));
        validate_containment(&next.nodes, &proposed)?;
        let node = &mut next.nodes[index];
        node.parent_id = parent_id;
        node.placement = proposed.placement;
        self.commit(next, ChangeTopic::Storage).await
    }

    pub async fn create_shelf(&self, spec: CreateShelfSpec) -> Result<NodeId> {
        let (name, parent_id, size, position, metadata) = match spec {
            CreateShelfSpec::InRack {
                name,
                rack_id,
                altitude,
                usable_height,
                metadata,
            } => {
                let snapshot = self.snapshot().await;
                let rack = snapshot
                    .nodes
                    .iter()
                    .find(|node| node.id == rack_id)
                    .ok_or_else(|| StuffError::NotFound(rack_id.to_string()))?;
                if rack.kind != StorageKind::Rack {
                    return Err(StuffError::Validation(
                        "rack shelf parent must be a rack".into(),
                    ));
                }
                let outer = rack.outer_dimensions().ok_or_else(|| {
                    StuffError::Validation("rack needs positive outer dimensions".into())
                })?;
                let width = outer.width - 10;
                let depth = outer.depth - 10;
                if width <= 0 || depth <= 0 || usable_height <= 0 || altitude < 0 {
                    return Err(StuffError::Validation(
                        "rack shelf dimensions and altitude must be positive".into(),
                    ));
                }
                (
                    name,
                    rack_id,
                    Size3 {
                        width,
                        depth,
                        height: usable_height,
                    },
                    Point3 {
                        x: 5,
                        y: 5,
                        z: altitude,
                    },
                    metadata,
                )
            }
            CreateShelfSpec::InRoom {
                name,
                room_id,
                usable_size,
                position,
                metadata,
            } => {
                let snapshot = self.snapshot().await;
                let room = snapshot
                    .nodes
                    .iter()
                    .find(|node| node.id == room_id)
                    .ok_or_else(|| StuffError::NotFound(room_id.to_string()))?;
                if room.kind != StorageKind::Room {
                    return Err(StuffError::Validation(
                        "wall shelf parent must be a room".into(),
                    ));
                }
                if position.x < 0 || position.y < 0 || position.z < 0 {
                    return Err(StuffError::Validation(
                        "room shelf position cannot be negative".into(),
                    ));
                }
                (name, room_id, usable_size, position, metadata)
            }
        };
        self.create_node(StorageNode {
            id: NodeId::default(),
            name,
            kind: StorageKind::Shelf,
            parent_id: Some(parent_id),
            placement: Some(Placement { position }),
            size: Some(size),
            metadata,
        })
        .await
    }

    pub async fn create_rack_template(&self, mut template: RackTemplate) -> Result<RackTemplateId> {
        template.validate().map_err(StuffError::Validation)?;
        let mut next = self.snapshot().await;
        if template.id.0.is_nil() {
            template.id = RackTemplateId::new();
        } else if next
            .rack_templates
            .iter()
            .any(|existing| existing.id == template.id)
        {
            return Err(StuffError::Invalid("rack type id already exists".into()));
        }
        let id = template.id;
        next.rack_templates.push(template);
        self.commit(next, ChangeTopic::Storage).await?;
        Ok(id)
    }

    pub async fn update_rack_template(&self, template: RackTemplate) -> Result<()> {
        template.validate().map_err(StuffError::Validation)?;
        if template.id.0.is_nil() {
            return Err(StuffError::Invalid(
                "rack type id is required for update".into(),
            ));
        }
        let mut next = self.snapshot().await;
        let index = next
            .rack_templates
            .iter()
            .position(|existing| existing.id == template.id)
            .ok_or_else(|| StuffError::NotFound(template.id.to_string()))?;
        let instance_indices = next
            .rack_instances
            .iter()
            .enumerate()
            .filter_map(|(index, instance)| (instance.template_id == template.id).then_some(index))
            .collect::<Vec<_>>();
        for instance_index in instance_indices {
            let instance = next.rack_instances[instance_index].clone();
            let obsolete_shelf_ids = instance
                .shelf_ids
                .iter()
                .skip(template.shelves.len())
                .copied()
                .collect::<Vec<_>>();
            for shelf_id in &obsolete_shelf_ids {
                let has_children = next
                    .nodes
                    .iter()
                    .any(|node| node.parent_id == Some(*shelf_id));
                let has_state = next
                    .stock
                    .iter()
                    .any(|lot| lot.quantity > 0 && lot.location.node_id == *shelf_id)
                    || next
                        .reservations
                        .iter()
                        .any(|reservation| reservation.location.node_id == *shelf_id)
                    || next
                        .occupancies
                        .iter()
                        .any(|occupancy| occupancy.location.node_id == *shelf_id);
                if has_children || has_state {
                    return Err(StuffError::Validation(
                        "cannot remove a rack shelf from a type while one of its instances is in use"
                            .into(),
                    ));
                }
            }
            {
                let rack = next
                    .nodes
                    .iter_mut()
                    .find(|node| node.id == instance.rack_id)
                    .ok_or_else(|| StuffError::NotFound(instance.rack_id.to_string()))?;
                if rack.kind != StorageKind::Rack {
                    return Err(StuffError::Validation(
                        "rack type instance no longer points to a rack".into(),
                    ));
                }
                rack.size = Some(template.outer_size);
            }
            let mut shelf_ids = Vec::with_capacity(template.shelves.len());
            for (shelf_index, shelf) in template.shelves.iter().enumerate() {
                let shelf_id = instance
                    .shelf_ids
                    .get(shelf_index)
                    .copied()
                    .unwrap_or_else(NodeId::new);
                let shelf_node = StorageNode {
                    id: shelf_id,
                    name: shelf.name.clone(),
                    kind: StorageKind::Shelf,
                    parent_id: Some(instance.rack_id),
                    placement: Some(Placement {
                        position: Point3 {
                            x: 5,
                            y: 5,
                            z: shelf.altitude,
                        },
                    }),
                    size: Some(Size3 {
                        width: template.outer_size.width - 10,
                        depth: template.outer_size.depth - 10,
                        height: shelf.usable_height,
                    }),
                    metadata: shelf.metadata.clone(),
                };
                if let Some(existing_shelf) = next.nodes.iter_mut().find(|node| node.id == shelf_id)
                {
                    *existing_shelf = shelf_node;
                } else {
                    next.nodes.push(shelf_node);
                }
                shelf_ids.push(shelf_id);
            }
            next.nodes
                .retain(|node| !obsolete_shelf_ids.contains(&node.id));
            next.rack_instances[instance_index].shelf_ids = shelf_ids;
        }
        next.rack_templates[index] = template;
        self.commit(next, ChangeTopic::Storage).await
    }

    pub async fn clone_rack_template(&self, spec: CloneRackTemplateSpec) -> Result<RackTemplateId> {
        validate_name(&spec.name, "rack type")?;
        let snapshot = self.snapshot().await;
        let source = snapshot
            .rack_templates
            .iter()
            .find(|template| template.id == spec.source_template_id)
            .ok_or_else(|| StuffError::NotFound(spec.source_template_id.to_string()))?;
        if source.outer_size.height <= 0 {
            return Err(StuffError::Validation(
                "source rack type height must be positive".into(),
            ));
        }
        let scale_height = |value: Mm| {
            value
                .checked_mul(spec.outer_size.height)
                .map(|scaled| scaled / source.outer_size.height)
                .ok_or_else(|| StuffError::Validation("rack type scaling overflow".into()))
        };
        let shelves = source
            .shelves
            .iter()
            .map(|shelf| {
                Ok(RackShelfTemplate {
                    name: shelf.name.clone(),
                    altitude: scale_height(shelf.altitude)?,
                    usable_height: scale_height(shelf.usable_height)?.max(1),
                    metadata: shelf.metadata.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        drop(snapshot);
        self.create_rack_template(RackTemplate {
            id: RackTemplateId::default(),
            name: spec.name,
            outer_size: spec.outer_size,
            shelves,
            metadata: spec.metadata,
        })
        .await
    }

    pub async fn save_rack_as_template(
        &self,
        spec: SaveRackAsTemplateSpec,
    ) -> Result<RackTemplateId> {
        validate_name(&spec.template_name, "rack type")?;
        let mut next = self.snapshot().await;
        let rack = next
            .nodes
            .iter()
            .find(|node| node.id == spec.rack_id)
            .ok_or_else(|| StuffError::NotFound(spec.rack_id.to_string()))?;
        if rack.kind != StorageKind::Rack {
            return Err(StuffError::Validation(
                "only a rack can be saved as a rack type".into(),
            ));
        }
        let outer_size = rack.outer_dimensions().ok_or_else(|| {
            StuffError::Validation("rack needs outer dimensions before saving a type".into())
        })?;
        let mut shelves = next
            .nodes
            .iter()
            .filter(|node| node.parent_id == Some(rack.id) && node.kind == StorageKind::Shelf)
            .map(|node| {
                let placement = node.placement.ok_or_else(|| {
                    StuffError::Validation(format!(
                        "shelf '{}' needs an altitude before saving the rack type",
                        node.name
                    ))
                })?;
                let size = node.size.ok_or_else(|| {
                    StuffError::Validation(format!(
                        "shelf '{}' needs usable dimensions before saving the rack type",
                        node.name
                    ))
                })?;
                Ok((
                    node.id,
                    RackShelfTemplate {
                        name: node.name.clone(),
                        altitude: placement.position.z,
                        usable_height: size.height,
                        metadata: node.metadata.clone(),
                    },
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        shelves.sort_by_key(|shelf| shelf.1.altitude);
        let template = RackTemplate {
            id: RackTemplateId::new(),
            name: spec.template_name,
            outer_size,
            shelves: shelves.iter().map(|(_, shelf)| shelf.clone()).collect(),
            metadata: spec.metadata,
        };
        template.validate().map_err(StuffError::Validation)?;
        let template_id = template.id;
        next.rack_templates.push(template);
        next.rack_instances
            .retain(|instance| instance.rack_id != spec.rack_id);
        next.rack_instances.push(RackInstance {
            rack_id: spec.rack_id,
            template_id,
            shelf_ids: shelves.into_iter().map(|(id, _)| id).collect(),
        });
        self.commit(next, ChangeTopic::Storage).await?;
        Ok(template_id)
    }

    pub async fn create_rack_from_template(
        &self,
        spec: CreateRackFromTemplateSpec,
    ) -> Result<NodeId> {
        validate_name(&spec.rack_name, "rack")?;
        let mut next = self.snapshot().await;
        let room = next
            .nodes
            .iter()
            .find(|node| node.id == spec.room_id)
            .ok_or_else(|| StuffError::NotFound(spec.room_id.to_string()))?;
        if room.kind != StorageKind::Room {
            return Err(StuffError::Validation(
                "rack type instances must be created in a room".into(),
            ));
        }
        let template = next
            .rack_templates
            .iter()
            .find(|template| template.id == spec.template_id)
            .cloned()
            .ok_or_else(|| StuffError::NotFound(spec.template_id.to_string()))?;
        template.validate().map_err(StuffError::Validation)?;
        let rack_id = NodeId::new();
        let rack = StorageNode {
            id: rack_id,
            name: spec.rack_name,
            kind: StorageKind::Rack,
            parent_id: Some(spec.room_id),
            placement: spec.placement,
            size: Some(template.outer_size),
            metadata: spec.metadata,
        };
        validate_containment(&next.nodes, &rack)?;
        next.nodes.push(rack);
        let mut shelf_ids = Vec::with_capacity(template.shelves.len());
        for shelf in template.shelves {
            let shelf_id = NodeId::new();
            next.nodes.push(StorageNode {
                id: shelf_id,
                name: shelf.name,
                kind: StorageKind::Shelf,
                parent_id: Some(rack_id),
                placement: Some(Placement {
                    position: Point3 {
                        x: 5,
                        y: 5,
                        z: shelf.altitude,
                    },
                }),
                size: Some(Size3 {
                    width: template.outer_size.width - 10,
                    depth: template.outer_size.depth - 10,
                    height: shelf.usable_height,
                }),
                metadata: shelf.metadata,
            });
            shelf_ids.push(shelf_id);
        }
        next.rack_instances.push(RackInstance {
            rack_id,
            template_id: spec.template_id,
            shelf_ids,
        });
        self.commit(next, ChangeTopic::Storage).await?;
        Ok(rack_id)
    }

    pub async fn create_drawer_cabinet_template(
        &self,
        mut template: DrawerCabinetTemplate,
    ) -> Result<DrawerCabinetTemplateId> {
        template.validate().map_err(StuffError::Validation)?;
        let mut next = self.snapshot().await;
        if template.id.0.is_nil() {
            template.id = DrawerCabinetTemplateId::new();
        } else if next
            .drawer_cabinet_templates
            .iter()
            .any(|existing| existing.id == template.id)
        {
            return Err(StuffError::Invalid(
                "drawer cabinet template id already exists".into(),
            ));
        }
        let id = template.id;
        next.drawer_cabinet_templates.push(template);
        self.commit(next, ChangeTopic::Storage).await?;
        Ok(id)
    }

    pub async fn clone_drawer_cabinet_template(
        &self,
        spec: CloneDrawerCabinetTemplateSpec,
    ) -> Result<DrawerCabinetTemplateId> {
        let mut next = self.snapshot().await;
        let source = next
            .drawer_cabinet_templates
            .iter()
            .find(|template| template.id == spec.source_template_id)
            .cloned()
            .ok_or_else(|| StuffError::NotFound(spec.source_template_id.to_string()))?;
        let template = DrawerCabinetTemplate {
            id: DrawerCabinetTemplateId::new(),
            name: spec.name,
            outer_size: spec.outer_size,
            drawers: source.drawers,
            metadata: spec.metadata,
        };
        template.validate().map_err(StuffError::Validation)?;
        let id = template.id;
        next.drawer_cabinet_templates.push(template);
        self.commit(next, ChangeTopic::Storage).await?;
        Ok(id)
    }

    /// Updates a reusable drawer type and all physical cabinets that use it.
    /// Generated drawer IDs remain stable so inventory locations do too.
    pub async fn update_drawer_cabinet_template(
        &self,
        template: DrawerCabinetTemplate,
    ) -> Result<()> {
        template.validate().map_err(StuffError::Validation)?;
        let mut next = self.snapshot().await;
        let template_index = next
            .drawer_cabinet_templates
            .iter()
            .position(|existing| existing.id == template.id)
            .ok_or_else(|| StuffError::NotFound(template.id.to_string()))?;
        let instance_indices = next
            .drawer_cabinet_instances
            .iter()
            .enumerate()
            .filter_map(|(index, instance)| (instance.template_id == template.id).then_some(index))
            .collect::<Vec<_>>();
        for instance_index in instance_indices {
            let instance = next.drawer_cabinet_instances[instance_index].clone();
            let obsolete_drawer_ids = instance
                .drawer_ids
                .iter()
                .skip(template.drawers.len())
                .copied()
                .collect::<Vec<_>>();
            for drawer_id in &obsolete_drawer_ids {
                let has_children = next
                    .nodes
                    .iter()
                    .any(|node| node.parent_id == Some(*drawer_id));
                let has_state = next
                    .stock
                    .iter()
                    .any(|lot| lot.quantity > 0 && lot.location.node_id == *drawer_id)
                    || next
                        .reservations
                        .iter()
                        .any(|reservation| reservation.location.node_id == *drawer_id)
                    || next
                        .occupancies
                        .iter()
                        .any(|occupancy| occupancy.location.node_id == *drawer_id);
                if has_children || has_state {
                    return Err(StuffError::Validation(
                        "cannot remove a drawer from a type while one of its instances is in use"
                            .into(),
                    ));
                }
            }
            let cabinet = next
                .nodes
                .iter_mut()
                .find(|node| node.id == instance.cabinet_id)
                .ok_or_else(|| StuffError::NotFound(instance.cabinet_id.to_string()))?;
            if cabinet.kind != StorageKind::Custom(DRAWER_CABINET_KIND.into()) {
                return Err(StuffError::Validation(
                    "drawer cabinet type instance no longer points to a cabinet".into(),
                ));
            }
            cabinet.size = Some(template.outer_size);
            let mut drawer_ids = Vec::with_capacity(template.drawers.len());
            for (drawer_index, drawer) in template.drawers.iter().enumerate() {
                let drawer_id = instance
                    .drawer_ids
                    .get(drawer_index)
                    .copied()
                    .unwrap_or_else(NodeId::new);
                let drawer_node = StorageNode {
                    id: drawer_id,
                    name: drawer.name.clone(),
                    kind: StorageKind::Drawer,
                    parent_id: Some(instance.cabinet_id),
                    placement: Some(Placement {
                        position: Point3 {
                            x: 0,
                            y: 0,
                            z: drawer.altitude,
                        },
                    }),
                    size: Some(drawer.usable_size),
                    metadata: drawer.metadata.clone(),
                };
                if let Some(existing) = next.nodes.iter_mut().find(|node| node.id == drawer_id) {
                    *existing = drawer_node;
                } else {
                    next.nodes.push(drawer_node);
                }
                drawer_ids.push(drawer_id);
            }
            next.nodes
                .retain(|node| !obsolete_drawer_ids.contains(&node.id));
            next.drawer_cabinet_instances[instance_index].drawer_ids = drawer_ids;
        }
        next.drawer_cabinet_templates[template_index] = template;
        self.commit(next, ChangeTopic::Storage).await
    }

    /// Creates the physical cabinet and every generated Drawer child in one
    /// transaction. Drawers are ordinary inventory-capable storage nodes.
    pub async fn create_drawer_cabinet_from_template(
        &self,
        spec: CreateDrawerCabinetFromTemplateSpec,
    ) -> Result<NodeId> {
        validate_name(&spec.cabinet_name, "drawer cabinet")?;
        let mut next = self.snapshot().await;
        let template = next
            .drawer_cabinet_templates
            .iter()
            .find(|template| template.id == spec.template_id)
            .cloned()
            .ok_or_else(|| StuffError::NotFound(spec.template_id.to_string()))?;
        template.validate().map_err(StuffError::Validation)?;
        if !next.nodes.iter().any(|node| node.id == spec.parent_id) {
            return Err(StuffError::NotFound(spec.parent_id.to_string()));
        }
        let cabinet_id = NodeId::new();
        let cabinet = StorageNode {
            id: cabinet_id,
            name: spec.cabinet_name,
            kind: StorageKind::Custom(DRAWER_CABINET_KIND.into()),
            parent_id: Some(spec.parent_id),
            placement: spec.placement,
            size: Some(template.outer_size),
            metadata: spec.metadata,
        };
        validate_containment(&next.nodes, &cabinet)?;
        next.nodes.push(cabinet);
        let mut drawer_ids = Vec::with_capacity(template.drawers.len());
        for drawer in template.drawers {
            let drawer_id = NodeId::new();
            next.nodes.push(StorageNode {
                id: drawer_id,
                name: drawer.name,
                kind: StorageKind::Drawer,
                parent_id: Some(cabinet_id),
                placement: Some(Placement {
                    position: Point3 {
                        x: 0,
                        y: 0,
                        z: drawer.altitude,
                    },
                }),
                size: Some(drawer.usable_size),
                metadata: drawer.metadata,
            });
            drawer_ids.push(drawer_id);
        }
        next.drawer_cabinet_instances.push(DrawerCabinetInstance {
            cabinet_id,
            template_id: spec.template_id,
            drawer_ids,
        });
        self.commit(next, ChangeTopic::Storage).await?;
        Ok(cabinet_id)
    }

    /// Deep-copies a storage subtree and any organizer instances inside it.
    /// Templates stay shared; stock, occupancy, and reservations are never
    /// copied. The cloned root is deliberately unplaced to avoid overlap.
    pub async fn clone_storage_subtree(
        &self,
        root_id: NodeId,
        new_parent: Option<NodeId>,
    ) -> Result<NodeId> {
        let mut next = self.snapshot().await;
        let root = next
            .nodes
            .iter()
            .find(|node| node.id == root_id)
            .cloned()
            .ok_or_else(|| StuffError::NotFound(root_id.to_string()))?;
        let destination_parent = new_parent.or(root.parent_id);
        if let Some(parent_id) = destination_parent
            && !next.nodes.iter().any(|node| node.id == parent_id)
        {
            return Err(StuffError::NotFound(parent_id.to_string()));
        }
        let mut source_nodes = Vec::new();
        let mut pending = vec![root_id];
        while let Some(id) = pending.pop() {
            let node = next
                .nodes
                .iter()
                .find(|node| node.id == id)
                .cloned()
                .ok_or_else(|| StuffError::NotFound(id.to_string()))?;
            pending.extend(
                next.nodes
                    .iter()
                    .filter(|candidate| candidate.parent_id == Some(id))
                    .map(|candidate| candidate.id),
            );
            source_nodes.push(node);
        }
        let id_map: BTreeMap<_, _> = source_nodes
            .iter()
            .map(|node| (node.id, NodeId::new()))
            .collect();
        let mut names: std::collections::BTreeSet<_> =
            next.nodes.iter().map(|node| node.name.clone()).collect();
        let mut cloned_nodes = Vec::with_capacity(source_nodes.len());
        for source in &source_nodes {
            let is_root = source.id == root_id;
            let name = unique_copy_name(&mut names, &source.name);
            let node = StorageNode {
                id: id_map[&source.id],
                name,
                kind: source.kind.clone(),
                parent_id: if is_root {
                    destination_parent
                } else {
                    source.parent_id.map(|parent| id_map[&parent])
                },
                placement: if is_root { None } else { source.placement },
                size: source.size,
                metadata: source.metadata.clone(),
            };
            let mut containment_nodes = next.nodes.clone();
            containment_nodes.extend(cloned_nodes.iter().cloned());
            validate_containment(&containment_nodes, &node)?;
            cloned_nodes.push(node);
        }
        let cloned_root_id = id_map[&root_id];
        if let Some(cloned_root) = cloned_nodes
            .iter_mut()
            .find(|node| node.id == cloned_root_id)
        {
            cloned_root.placement = first_available_placement(
                &next.nodes,
                destination_parent,
                cloned_root.id,
                cloned_root.size,
            );
        }
        let cloned_organizers: Vec<_> = next
            .organizers
            .iter()
            .filter_map(|organizer| {
                id_map
                    .get(&organizer.node_id)
                    .map(|node_id| OrganizerInstance {
                        id: OrganizerId::new(),
                        node_id: *node_id,
                        template_id: organizer.template_id,
                        removed_dividers: organizer.removed_dividers.clone(),
                    })
            })
            .collect();
        let cloned_rack_instances: Vec<_> = next
            .rack_instances
            .iter()
            .filter_map(|instance| {
                id_map.get(&instance.rack_id).map(|rack_id| RackInstance {
                    rack_id: *rack_id,
                    template_id: instance.template_id,
                    shelf_ids: instance
                        .shelf_ids
                        .iter()
                        .filter_map(|shelf_id| id_map.get(shelf_id).copied())
                        .collect(),
                })
            })
            .collect();
        let cloned_drawer_instances: Vec<_> = next
            .drawer_cabinet_instances
            .iter()
            .filter_map(|instance| {
                id_map
                    .get(&instance.cabinet_id)
                    .map(|cabinet_id| DrawerCabinetInstance {
                        cabinet_id: *cabinet_id,
                        template_id: instance.template_id,
                        drawer_ids: instance
                            .drawer_ids
                            .iter()
                            .filter_map(|drawer_id| id_map.get(drawer_id).copied())
                            .collect(),
                    })
            })
            .collect();
        next.nodes.extend(cloned_nodes);
        next.organizers.extend(cloned_organizers);
        next.rack_instances.extend(cloned_rack_instances);
        next.drawer_cabinet_instances
            .extend(cloned_drawer_instances);
        self.commit(next, ChangeTopic::Storage).await?;
        Ok(cloned_root_id)
    }

    /// Removes an empty physical-storage subtree. Inventory, reservations, and
    /// explicit occupancy are never silently discarded with a container.
    pub async fn delete_storage_subtree(&self, root_id: NodeId) -> Result<usize> {
        let mut next = self.snapshot().await;
        if generated_node_parent(&next, root_id).is_some() {
            return Err(StuffError::Validation(
                "generated rack shelves and cabinet drawers must be removed through their type"
                    .into(),
            ));
        }
        let root = next
            .nodes
            .iter()
            .find(|node| node.id == root_id)
            .cloned()
            .ok_or_else(|| StuffError::NotFound(root_id.to_string()))?;
        let mut removed_ids = BTreeSet::new();
        let mut pending = vec![root_id];
        while let Some(id) = pending.pop() {
            if !removed_ids.insert(id) {
                continue;
            }
            pending.extend(
                next.nodes
                    .iter()
                    .filter(|node| node.parent_id == Some(id))
                    .map(|node| node.id),
            );
        }
        if next
            .stock
            .iter()
            .any(|lot| lot.quantity > 0 && removed_ids.contains(&lot.location.node_id))
        {
            return Err(StuffError::Validation(format!(
                "cannot delete '{}' because it contains stock; move or consume the stock first",
                root.name
            )));
        }
        if next
            .reservations
            .iter()
            .any(|reservation| removed_ids.contains(&reservation.location.node_id))
            || next
                .occupancies
                .iter()
                .any(|occupancy| removed_ids.contains(&occupancy.location.node_id))
        {
            return Err(StuffError::Validation(format!(
                "cannot delete '{}' because it has reservation or occupancy data; clear that state first",
                root.name
            )));
        }
        let removed_count = removed_ids.len();
        next.nodes.retain(|node| !removed_ids.contains(&node.id));
        next.organizers
            .retain(|organizer| !removed_ids.contains(&organizer.node_id));
        next.rack_instances
            .retain(|instance| !removed_ids.contains(&instance.rack_id));
        next.drawer_cabinet_instances
            .retain(|instance| !removed_ids.contains(&instance.cabinet_id));
        self.commit(next, ChangeTopic::Storage).await?;
        Ok(removed_count)
    }

    /// Removes one storage object while keeping its direct children. Children
    /// move to the deleted object's parent and become unplaced because their
    /// old coordinates were relative to the deleted volume.
    pub async fn delete_storage_orphan_children(&self, node_id: NodeId) -> Result<usize> {
        let mut next = self.snapshot().await;
        if generated_node_parent(&next, node_id).is_some() {
            return Err(StuffError::Validation(
                "generated rack shelves and cabinet drawers must be removed through their type"
                    .into(),
            ));
        }
        let node = next
            .nodes
            .iter()
            .find(|candidate| candidate.id == node_id)
            .cloned()
            .ok_or_else(|| StuffError::NotFound(node_id.to_string()))?;
        let mut generated_children = next
            .rack_instances
            .iter()
            .find(|instance| instance.rack_id == node_id)
            .map(|instance| instance.shelf_ids.iter().copied().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        if let Some(instance) = next
            .drawer_cabinet_instances
            .iter()
            .find(|instance| instance.cabinet_id == node_id)
        {
            generated_children.extend(instance.drawer_ids.iter().copied());
        }
        let removed_ids = std::iter::once(node_id)
            .chain(generated_children.iter().copied())
            .collect::<BTreeSet<_>>();
        let children = next
            .nodes
            .iter()
            .filter(|candidate| {
                candidate.parent_id == Some(node_id)
                    || generated_children.contains(&candidate.parent_id.unwrap_or_default())
            })
            .filter(|candidate| !generated_children.contains(&candidate.id))
            .cloned()
            .collect::<Vec<_>>();
        for child in &children {
            let mut moved = child.clone();
            moved.parent_id = node.parent_id;
            moved.placement = None;
            validate_containment(&next.nodes, &moved)?;
        }
        if next
            .stock
            .iter()
            .any(|lot| lot.quantity > 0 && removed_ids.contains(&lot.location.node_id))
        {
            return Err(StuffError::Validation(format!(
                "cannot delete '{}' because it contains stock; move or consume the stock first",
                node.name
            )));
        }
        if next
            .reservations
            .iter()
            .any(|reservation| removed_ids.contains(&reservation.location.node_id))
            || next
                .occupancies
                .iter()
                .any(|occupancy| removed_ids.contains(&occupancy.location.node_id))
        {
            return Err(StuffError::Validation(format!(
                "cannot delete '{}' because it has reservation or occupancy data; clear that state first",
                node.name
            )));
        }
        for child in next
            .nodes
            .iter_mut()
            .filter(|child| {
                child.parent_id == Some(node_id)
                    || generated_children.contains(&child.parent_id.unwrap_or_default())
            })
            .filter(|child| !generated_children.contains(&child.id))
        {
            child.parent_id = node.parent_id;
            child.placement = None;
        }
        next.nodes
            .retain(|candidate| !removed_ids.contains(&candidate.id));
        next.organizers
            .retain(|organizer| !removed_ids.contains(&organizer.node_id));
        next.rack_instances
            .retain(|instance| instance.rack_id != node_id);
        next.drawer_cabinet_instances
            .retain(|instance| instance.cabinet_id != node_id);
        self.commit(next, ChangeTopic::Storage).await?;
        Ok(children.len())
    }

    pub async fn create_item(&self, item: Item) -> Result<ItemId> {
        validate_name(&item.name, "item")?;
        validate_size(item.size)?;
        let mut item = item;
        let mut next = self.snapshot().await;
        if item.id.0.is_nil() {
            item.id = ItemId::new();
        } else if next.items.iter().any(|existing| existing.id == item.id) {
            return Err(StuffError::Invalid("item id already exists".into()));
        }
        let id = item.id;
        next.items.push(item);
        self.commit(next, ChangeTopic::Inventory).await?;
        Ok(id)
    }
    pub async fn create_item_with_stock(
        &self,
        mut item: Item,
        location: StockLocation,
        quantity: i64,
    ) -> Result<(ItemId, StockLotId)> {
        validate_name(&item.name, "item")?;
        validate_size(item.size)?;
        if quantity <= 0 {
            return Err(StuffError::Validation(
                "initial stock quantity must be positive".into(),
            ));
        }
        let mut next = self.snapshot().await;
        validate_stock_location(&next, &location)?;
        if item.id.0.is_nil() {
            item.id = ItemId::new();
        } else if next.items.iter().any(|existing| existing.id == item.id) {
            return Err(StuffError::Invalid("item id already exists".into()));
        }
        let item_id = item.id;
        let stock_id = StockLotId::new();
        next.items.push(item);
        let mut lot = StockLot {
            id: stock_id,
            item_id,
            location,
            quantity,
            placement: None,
        };
        lot.placement = placement_for_new_stock_lot(&next, &lot)?;
        validate_stock_lot_geometry(&next, &lot)?;
        next.stock.push(lot);
        self.commit(next, ChangeTopic::Inventory).await?;
        Ok((item_id, stock_id))
    }
    /// Finds an existing item by exact normalized name or manufacturer part
    /// number, otherwise creates the supplied full item and adds stock in the
    /// same commit. Multiple identity matches are deliberately reported as an
    /// ambiguity instead of picking one.
    pub async fn add_or_create_item_with_stock(
        &self,
        mut spec: AddOrCreateItemSpec,
    ) -> Result<AddOrCreateItemResult> {
        validate_name(&spec.item.name, "item")?;
        validate_size(spec.item.size)?;
        if spec.quantity <= 0 {
            return Err(StuffError::Validation(
                "initial stock quantity must be positive".into(),
            ));
        }
        let mut next = self.snapshot().await;
        validate_stock_location(&next, &spec.location)?;
        let matches = exact_item_identity_matches(&next.items, &spec.item);
        let destination_is_spatial = stock_location_is_spatial(&next, &spec.location)?;
        let requested_size = spec.item.size;
        let (item_id, created_item) = match matches.as_slice() {
            [] => {
                if spec.item.id.0.is_nil() {
                    spec.item.id = ItemId::new();
                } else if next.items.iter().any(|item| item.id == spec.item.id) {
                    return Err(StuffError::Invalid("item id already exists".into()));
                }
                let id = spec.item.id;
                next.items.push(spec.item);
                (id, true)
            }
            [item] => {
                let item_id = item.id;
                if destination_is_spatial {
                    let requested_size = requested_size.ok_or_else(|| {
                        StuffError::Validation(
                            "item dimensions are required for direct stock in a room, rack, or shelf"
                                .into(),
                        )
                    })?;
                    let existing = next
                        .items
                        .iter_mut()
                        .find(|candidate| candidate.id == item_id)
                        .expect("matched item must still exist");
                    match existing.size {
                        Some(size) if size != requested_size => {
                            return Err(StuffError::Validation(
                                "the matching item already has different dimensions".into(),
                            ));
                        }
                        None => existing.size = Some(requested_size),
                        Some(_) => {}
                    }
                }
                (item_id, false)
            }
            _ => {
                return Err(StuffError::Ambiguous {
                    selector: spec.item.name,
                    candidates: matches
                        .iter()
                        .map(|item| AmbiguousCandidate {
                            id: item.id.to_string(),
                            name: item.name.clone(),
                            path: None,
                        })
                        .collect(),
                });
            }
        };
        let stock_lot_id = if let Some(lot) = next
            .stock
            .iter_mut()
            .find(|lot| lot.item_id == item_id && lot.location == spec.location)
        {
            lot.quantity = lot
                .quantity
                .checked_add(spec.quantity)
                .ok_or_else(|| StuffError::Validation("quantity overflow".into()))?;
            lot.id
        } else {
            let id = StockLotId::new();
            let mut lot = StockLot {
                id,
                item_id,
                location: spec.location,
                quantity: spec.quantity,
                placement: None,
            };
            lot.placement = placement_for_new_stock_lot(&next, &lot)?;
            validate_stock_lot_geometry(&next, &lot)?;
            next.stock.push(lot);
            id
        };
        self.commit(next, ChangeTopic::Inventory).await?;
        Ok(AddOrCreateItemResult {
            item_id,
            stock_lot_id,
            created_item,
        })
    }
    pub async fn update_item(&self, item: Item) -> Result<()> {
        validate_name(&item.name, "item")?;
        validate_size(item.size)?;
        if item.id.0.is_nil() {
            return Err(StuffError::Invalid("item id is required for update".into()));
        }
        let mut next = self.snapshot().await;
        let index = next
            .items
            .iter()
            .position(|existing| existing.id == item.id)
            .ok_or_else(|| StuffError::NotFound(item.id.to_string()))?;
        let old = std::mem::replace(&mut next.items[index], item);
        if let Err(error) = next
            .stock
            .iter()
            .filter(|lot| lot.item_id == next.items[index].id)
            .try_for_each(|lot| validate_stock_lot_geometry(&next, lot))
        {
            next.items[index] = old;
            return Err(error);
        }
        self.commit(next, ChangeTopic::Inventory).await
    }
    pub async fn add_stock_by_selector(
        &self,
        item_selector: &str,
        location_selector: &str,
        quantity: i64,
    ) -> Result<StockLotId> {
        if quantity <= 0 {
            return Err(StuffError::Validation(
                "add quantity must be positive".into(),
            ));
        }
        let snapshot = self.snapshot().await;
        let item = resolve_item_selector(&snapshot.items, item_selector)?;
        let location = resolve_location_selector(&snapshot, location_selector)?;
        self.add_stock(StockLot {
            id: StockLotId::new(),
            item_id: item.id,
            location: location.location,
            quantity,
            placement: None,
        })
        .await
    }
    pub async fn add_stock(&self, mut lot: StockLot) -> Result<StockLotId> {
        if lot.quantity <= 0 {
            return Err(StuffError::Validation(
                "add quantity must be positive".into(),
            ));
        }
        validate_quantity(lot.quantity).map_err(StuffError::Validation)?;
        let mut next = self.snapshot().await;
        validate_stock_location(&next, &lot.location)?;
        if !next.items.iter().any(|i| i.id == lot.item_id) {
            return Err(StuffError::NotFound(lot.item_id.to_string()));
        }
        if !lot.id.0.is_nil() && next.stock.iter().any(|existing| existing.id == lot.id) {
            return Err(StuffError::Invalid("stock lot id already exists".into()));
        }
        if let Some(existing) = next
            .stock
            .iter_mut()
            .find(|existing| existing.item_id == lot.item_id && existing.location == lot.location)
        {
            existing.quantity = existing
                .quantity
                .checked_add(lot.quantity)
                .ok_or_else(|| StuffError::Validation("quantity overflow".into()))?;
            let id = existing.id;
            self.commit(next, ChangeTopic::Inventory).await?;
            return Ok(id);
        }
        lot.id = if lot.id.0.is_nil() {
            StockLotId::new()
        } else {
            lot.id
        };
        let id = lot.id;
        lot.placement = placement_for_new_stock_lot(&next, &lot)?;
        validate_stock_lot_geometry(&next, &lot)?;
        next.stock.push(lot);
        self.commit(next, ChangeTopic::Inventory).await?;
        Ok(id)
    }
    pub async fn adjust_stock(&self, id: StockLotId, delta: i64) -> Result<()> {
        let mut next = self.snapshot().await;
        let remove = {
            let lot = next
                .stock
                .iter_mut()
                .find(|l| l.id == id)
                .ok_or_else(|| StuffError::NotFound(id.to_string()))?;
            lot.adjust(delta).map_err(StuffError::Validation)?;
            lot.quantity == 0
        };
        if remove {
            next.stock.retain(|lot| lot.id != id);
        }
        self.commit(next, ChangeTopic::Inventory).await
    }
    pub async fn adjust_stock_by_selector(
        &self,
        item_selector: &str,
        location_selector: &str,
        delta: i64,
    ) -> Result<()> {
        let snapshot = self.snapshot().await;
        let item = resolve_item_selector(&snapshot.items, item_selector)?;
        let location = resolve_location_selector(&snapshot, location_selector)?.location;
        let lots: Vec<_> = snapshot
            .stock
            .iter()
            .filter(|lot| lot.item_id == item.id && lot.location == location)
            .collect();
        let lot = match lots.as_slice() {
            [] => return Err(StuffError::NotFound("stock at selected location".into())),
            [lot] => *lot,
            _ => {
                return Err(StuffError::Ambiguous {
                    selector: location_selector.into(),
                    candidates: lots
                        .iter()
                        .map(|lot| AmbiguousCandidate {
                            id: lot.id.to_string(),
                            name: item.name.clone(),
                            path: display_location(&snapshot, &lot.location).ok(),
                        })
                        .collect(),
                });
            }
        };
        self.adjust_stock(lot.id, delta).await
    }
    pub async fn move_stock(
        &self,
        id: StockLotId,
        destination: StockLocation,
        quantity: i64,
    ) -> Result<StockLotId> {
        self.move_stock_with_item_size(id, destination, quantity, None)
            .await
    }

    /// Moves stock and, when entering a spatial workspace for the first time,
    /// records the physical dimensions of the item in the same commit.
    pub async fn move_stock_with_item_size(
        &self,
        id: StockLotId,
        destination: StockLocation,
        quantity: i64,
        item_size: Option<Size3>,
    ) -> Result<StockLotId> {
        if quantity <= 0 {
            return Err(StuffError::Validation(
                "move quantity must be positive".into(),
            ));
        }
        validate_size(item_size)?;
        let mut next = self.snapshot().await;
        validate_stock_location(&next, &destination)?;
        let source_index = next
            .stock
            .iter()
            .position(|l| l.id == id)
            .ok_or_else(|| StuffError::NotFound(id.to_string()))?;
        if next.stock[source_index].quantity < quantity {
            return Err(StuffError::Validation("not enough stock".into()));
        }
        let source_id = next.stock[source_index].id;
        let item_id = next.stock[source_index].item_id;
        if stock_location_is_spatial(&next, &destination)? {
            let item = next
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or_else(|| StuffError::NotFound(item_id.to_string()))?;
            match (item.size, item_size) {
                (Some(existing), Some(requested)) if existing != requested => {
                    return Err(StuffError::Validation(
                        "the item already has different dimensions".into(),
                    ));
                }
                (None, Some(requested)) => item.size = Some(requested),
                (None, None) => {
                    return Err(StuffError::Validation(
                        "item dimensions are required when moving stock directly into a room, rack, or shelf"
                            .into(),
                    ));
                }
                _ => {}
            }
        }
        let target_index = next
            .stock
            .iter()
            .position(|lot| lot.item_id == item_id && lot.location == destination);
        let new_placement = if target_index.is_none() {
            let proposed = StockLot {
                id: StockLotId::new(),
                item_id,
                location: destination.clone(),
                quantity,
                placement: None,
            };
            placement_for_new_stock_lot(&next, &proposed)?
        } else {
            None
        };
        next.stock[source_index].quantity -= quantity;
        let target_id = if let Some(target_index) = target_index {
            let target = &mut next.stock[target_index];
            target.quantity = target
                .quantity
                .checked_add(quantity)
                .ok_or_else(|| StuffError::Validation("quantity overflow".into()))?;
            target.id
        } else {
            let lot = StockLot {
                id: StockLotId::new(),
                item_id,
                location: destination,
                quantity,
                placement: new_placement,
            };
            validate_stock_lot_geometry(&next, &lot)?;
            let id = lot.id;
            next.stock.push(lot);
            id
        };
        if target_id != source_id
            && next
                .stock
                .iter()
                .any(|lot| lot.id == source_id && lot.quantity == 0)
        {
            next.stock.retain(|lot| lot.id != source_id);
        }
        self.commit(next, ChangeTopic::Inventory).await?;
        Ok(target_id)
    }

    pub async fn update_stock_placement(&self, id: StockLotId, placement: Placement) -> Result<()> {
        let mut next = self.snapshot().await;
        let index = next
            .stock
            .iter()
            .position(|lot| lot.id == id)
            .ok_or_else(|| StuffError::NotFound(id.to_string()))?;
        next.stock[index].placement = Some(placement);
        validate_stock_lot_geometry(&next, &next.stock[index])?;
        self.commit(next, ChangeTopic::Inventory).await
    }
    pub async fn move_stock_by_selector(
        &self,
        item_selector: &str,
        from_selector: &str,
        to_selector: &str,
        quantity: Option<i64>,
    ) -> Result<StockLotId> {
        let snapshot = self.snapshot().await;
        let item = resolve_item_selector(&snapshot.items, item_selector)?;
        let from = resolve_location_selector(&snapshot, from_selector)?.location;
        let to = resolve_location_selector(&snapshot, to_selector)?.location;
        let lots: Vec<_> = snapshot
            .stock
            .iter()
            .filter(|lot| lot.item_id == item.id && lot.location == from)
            .collect();
        let lot = match lots.as_slice() {
            [] => return Err(StuffError::NotFound("stock at source location".into())),
            [lot] => *lot,
            _ => {
                return Err(StuffError::Ambiguous {
                    selector: from_selector.into(),
                    candidates: lots
                        .iter()
                        .map(|lot| AmbiguousCandidate {
                            id: lot.id.to_string(),
                            name: item.name.clone(),
                            path: display_location(&snapshot, &lot.location).ok(),
                        })
                        .collect(),
                });
            }
        };
        let amount = quantity.unwrap_or(lot.quantity);
        self.move_stock(lot.id, to, amount).await
    }
    pub async fn set_occupancy(&self, occupancy: Occupancy) -> Result<()> {
        let mut next = self.snapshot().await;
        validate_location_ref(&next, &occupancy.location)?;
        if occupancy.state == OccupancyState::Free
            && next
                .stock
                .iter()
                .any(|lot| lot.quantity > 0 && stock_matches_ref(lot, &occupancy.location))
        {
            return Err(StuffError::Validation(
                "a location with positive stock cannot be marked free".into(),
            ));
        }
        let key = occupancy_key(&occupancy.location);
        if let Some(existing) = next
            .occupancies
            .iter_mut()
            .find(|o| occupancy_key(&o.location) == key)
        {
            *existing = occupancy;
        } else {
            next.occupancies.push(occupancy);
        }
        self.commit(next, ChangeTopic::Occupancy).await
    }
    pub async fn reserve(&self, mut reservation: Reservation) -> Result<ReservationId> {
        validate_name(&reservation.label, "reservation")?;
        let mut next = self.snapshot().await;
        validate_location_ref(&next, &reservation.location)?;
        if reservation.id.0.is_nil() {
            reservation.id = ReservationId::new();
        } else if next
            .reservations
            .iter()
            .any(|existing| existing.id == reservation.id)
        {
            return Err(StuffError::Invalid("reservation id already exists".into()));
        }
        let id = reservation.id;
        next.reservations.push(reservation);
        self.commit(next, ChangeTopic::Occupancy).await?;
        Ok(id)
    }
    pub async fn remove_divider(
        &self,
        organizer_id: OrganizerId,
        divider: &DividerEdge,
    ) -> Result<()> {
        self.set_divider(organizer_id, divider, true).await
    }
    pub async fn set_divider(
        &self,
        organizer_id: OrganizerId,
        divider: &DividerEdge,
        removed: bool,
    ) -> Result<()> {
        let mut next = self.snapshot().await;
        let org_index = next
            .organizers
            .iter()
            .position(|o| o.id == organizer_id)
            .ok_or_else(|| StuffError::NotFound(organizer_id.to_string()))?;
        let divider_key = divider.key();
        let template = next
            .organizer_templates
            .iter()
            .find(|t| t.id == next.organizers[org_index].template_id)
            .ok_or_else(|| StuffError::NotFound("organizer template".into()))?;
        let stored_divider = template
            .dividers
            .iter()
            .find(|edge| edge.key() == divider_key)
            .ok_or_else(|| {
                StuffError::Invalid("divider does not belong to organizer template".into())
            })?;
        if stored_divider.kind != DividerKind::Removable {
            return Err(StuffError::Invalid(
                "fixed or impossible dividers cannot be changed".into(),
            ));
        }
        let old_removed = next.organizers[org_index].removed_dividers.clone();
        let mut new_removed = old_removed.clone();
        if removed {
            new_removed.insert(divider_key.clone());
        } else {
            new_removed.remove(&divider_key);
        }
        if old_removed == new_removed {
            return Ok(());
        }
        let old_compartments = template.compartments(&old_removed);
        let new_compartments = template.compartments(&new_removed);
        if !removed {
            for old in &old_compartments {
                let candidates: Vec<_> = new_compartments
                    .iter()
                    .filter(|candidate| compartments_overlap(old, candidate))
                    .map(|candidate| candidate.key.clone())
                    .collect();
                if candidates.len() <= 1 {
                    continue;
                }
                let attached = next.stock.iter().any(|lot| {
                    lot.quantity > 0 && stock_in_compartment(lot, organizer_id, &old.key)
                }) || next.occupancies.iter().any(|occupancy| {
                    location_in_compartment(&occupancy.location, organizer_id, &old.key)
                }) || next.reservations.iter().any(|reservation| {
                    location_in_compartment(&reservation.location, organizer_id, &old.key)
                });
                if attached {
                    return Err(StuffError::CompartmentSplitAmbiguous {
                        old_location: format!("{} / {}", organizer_id, old.key),
                        candidates,
                    });
                }
            }
        }
        let remaps: BTreeMap<_, _> = old_compartments
            .iter()
            .filter_map(|old| {
                new_compartments
                    .iter()
                    .find(|candidate| compartments_overlap(old, candidate))
                    .map(|candidate| (old.key.clone(), candidate.key.clone()))
            })
            .collect();
        let org = &mut next.organizers[org_index];
        org.removed_dividers = new_removed;
        if removed {
            for lot in &mut next.stock {
                remap_stock_compartment(lot, organizer_id, &remaps);
            }
            for occupancy in &mut next.occupancies {
                remap_location_compartment(&mut occupancy.location, organizer_id, &remaps);
            }
            for reservation in &mut next.reservations {
                remap_location_compartment(&mut reservation.location, organizer_id, &remaps);
            }
            coalesce_stock(&mut next.stock)?;
        }
        self.commit(next, ChangeTopic::Organizer).await
    }
}

fn validate_size(size: Option<Size3>) -> Result<()> {
    if let Some(s) = size
        && (s.width <= 0 || s.depth <= 0 || s.height <= 0)
    {
        return Err(StuffError::Validation(
            "dimensions must be positive when supplied".into(),
        ));
    }
    Ok(())
}

fn stock_location_is_spatial(snapshot: &Snapshot, location: &StockLocation) -> Result<bool> {
    let node = snapshot
        .nodes
        .iter()
        .find(|node| node.id == location.node_id)
        .ok_or_else(|| StuffError::NotFound(location.node_id.to_string()))?;
    Ok(location.compartment.is_none() && node.kind.has_spatial_stock())
}

fn validate_stock_lot_geometry(snapshot: &Snapshot, lot: &StockLot) -> Result<()> {
    validate_stock_location(snapshot, &lot.location)?;
    let spatial = stock_location_is_spatial(snapshot, &lot.location)?;
    if !spatial {
        if lot.placement.is_some() {
            return Err(StuffError::Validation(
                "stock inside a container or organizer compartment cannot have a workspace position"
                    .into(),
            ));
        }
        return Ok(());
    }

    let item = snapshot
        .items
        .iter()
        .find(|item| item.id == lot.item_id)
        .ok_or_else(|| StuffError::NotFound(lot.item_id.to_string()))?;
    let item_size = item.size.ok_or_else(|| {
        StuffError::Validation(
            "item dimensions are required for direct stock in a room, rack, or shelf".into(),
        )
    })?;
    validate_size(Some(item_size))?;
    let placement = lot.placement.ok_or_else(|| {
        StuffError::Validation(
            "direct stock in a room, rack, or shelf needs a workspace position".into(),
        )
    })?;
    let node = snapshot
        .nodes
        .iter()
        .find(|node| node.id == lot.location.node_id)
        .expect("stock location was validated above");
    let bounds = node
        .size
        .ok_or_else(|| StuffError::Validation("spatial stock location needs dimensions".into()))?;
    let parent = Aabb {
        origin: Point3::default(),
        size: bounds,
    };
    let object = Aabb {
        origin: placement.position,
        size: item_size,
    };
    if !parent.contains(object) {
        return Err(StuffError::Validation(
            "item placement must fit inside the room, rack, or shelf dimensions".into(),
        ));
    }
    Ok(())
}

fn placement_for_new_stock_lot(snapshot: &Snapshot, lot: &StockLot) -> Result<Option<Placement>> {
    if !stock_location_is_spatial(snapshot, &lot.location)? {
        return Ok(None);
    }
    let node = snapshot
        .nodes
        .iter()
        .find(|node| node.id == lot.location.node_id)
        .expect("stock location was resolved above");
    let bounds = node
        .size
        .ok_or_else(|| StuffError::Validation("spatial stock location needs dimensions".into()))?;
    let size = snapshot
        .items
        .iter()
        .find(|item| item.id == lot.item_id)
        .and_then(|item| item.size)
        .ok_or_else(|| {
            StuffError::Validation(
                "item dimensions are required for direct stock in a room, rack, or shelf".into(),
            )
        })?;
    validate_size(Some(size))?;

    let obstacles = snapshot
        .stock
        .iter()
        .filter(|existing| existing.location.node_id == lot.location.node_id)
        .filter_map(|existing| {
            Some(Aabb {
                origin: existing.placement?.position,
                size: snapshot
                    .items
                    .iter()
                    .find(|item| item.id == existing.item_id)?
                    .size?,
            })
        })
        .collect::<Vec<_>>();
    let mut candidates = vec![Point3::default()];
    for obstacle in &obstacles {
        if node.kind == StorageKind::Rack {
            candidates.extend([
                Point3 {
                    x: obstacle.max_x(),
                    y: 0,
                    z: obstacle.origin.z,
                },
                Point3 {
                    x: obstacle.origin.x,
                    y: 0,
                    z: obstacle.max_z(),
                },
            ]);
        } else {
            candidates.extend([
                Point3 {
                    x: obstacle.max_x(),
                    y: obstacle.origin.y,
                    z: 0,
                },
                Point3 {
                    x: obstacle.origin.x,
                    y: obstacle.max_y(),
                    z: 0,
                },
            ]);
        }
    }
    let parent = Aabb {
        origin: Point3::default(),
        size: bounds,
    };
    candidates
        .into_iter()
        .find_map(|position| {
            let candidate = Aabb {
                origin: position,
                size,
            };
            (parent.contains(candidate)
                && !obstacles
                    .iter()
                    .any(|obstacle| candidate.intersects(*obstacle)))
            .then_some(Placement { position })
        })
        .map(Some)
        .ok_or_else(|| {
            StuffError::Validation(
                "the item does not fit in a free position in this workspace".into(),
            )
        })
}

fn validate_storage_kind(kind: &StorageKind) -> Result<()> {
    if matches!(kind, StorageKind::Custom(value) if value.trim().is_empty()) {
        return Err(StuffError::Validation(
            "custom storage kind cannot be blank".into(),
        ));
    }
    Ok(())
}
fn generated_node_parent(snapshot: &Snapshot, node_id: NodeId) -> Option<(NodeId, StorageKind)> {
    if let Some(instance) = snapshot
        .rack_instances
        .iter()
        .find(|instance| instance.shelf_ids.contains(&node_id))
    {
        return Some((instance.rack_id, StorageKind::Shelf));
    }
    snapshot
        .drawer_cabinet_instances
        .iter()
        .find(|instance| instance.drawer_ids.contains(&node_id))
        .map(|instance| (instance.cabinet_id, StorageKind::Drawer))
}
fn has_node_only_location_state(snapshot: &Snapshot, node_id: NodeId) -> bool {
    snapshot.stock.iter().any(|lot| {
        lot.quantity > 0 && lot.location.node_id == node_id && lot.location.compartment.is_none()
    }) || snapshot.reservations.iter().any(|reservation| {
        reservation.location.node_id == node_id && reservation.location.organizer_id.is_none()
    }) || snapshot.occupancies.iter().any(|occupancy| {
        occupancy.location.node_id == node_id && occupancy.location.organizer_id.is_none()
    })
}
fn has_location_state(snapshot: &Snapshot, node_id: NodeId) -> bool {
    snapshot
        .stock
        .iter()
        .any(|lot| lot.quantity > 0 && lot.location.node_id == node_id)
        || snapshot
            .reservations
            .iter()
            .any(|reservation| reservation.location.node_id == node_id)
        || snapshot
            .occupancies
            .iter()
            .any(|occupancy| occupancy.location.node_id == node_id)
}
fn first_available_placement(
    nodes: &[StorageNode],
    parent_id: Option<NodeId>,
    node_id: NodeId,
    size: Option<Size3>,
) -> Option<Placement> {
    let parent_id = parent_id?;
    let parent_size = nodes.iter().find(|node| node.id == parent_id)?.size?;
    let size = size?;
    let parent = Aabb {
        origin: Point3::default(),
        size: parent_size,
    };
    let siblings = nodes
        .iter()
        .filter(|node| node.parent_id == Some(parent_id) && node.id != node_id)
        .filter_map(|node| {
            Some(Aabb {
                origin: node.placement?.position,
                size: node.size?,
            })
        })
        .collect::<Vec<_>>();
    let mut candidates = vec![Point3::default()];
    for sibling in &siblings {
        candidates.extend([
            Point3 {
                x: sibling.max_x(),
                y: sibling.origin.y,
                z: sibling.origin.z,
            },
            Point3 {
                x: sibling.origin.x,
                y: sibling.max_y(),
                z: sibling.origin.z,
            },
            Point3 {
                x: sibling.origin.x,
                y: sibling.origin.y,
                z: sibling.max_z(),
            },
        ]);
    }
    candidates.into_iter().find_map(|position| {
        let candidate = Aabb {
            origin: position,
            size,
        };
        (parent.contains(candidate)
            && !siblings
                .iter()
                .any(|sibling| candidate.intersects(*sibling)))
        .then_some(Placement { position })
    })
}
fn validate_containment(nodes: &[StorageNode], node: &StorageNode) -> Result<()> {
    let parent = match node.parent_id {
        Some(parent_id) => Some(
            nodes
                .iter()
                .find(|candidate| candidate.id == parent_id)
                .ok_or_else(|| StuffError::NotFound(parent_id.to_string()))?,
        ),
        None => None,
    };
    if !is_allowed_child_kind(parent.map(|node| &node.kind), &node.kind) {
        let parent_name = parent.map(|node| node.kind.as_str()).unwrap_or("root");
        return Err(StuffError::Validation(format!(
            "{} cannot be placed under {parent_name}",
            node.kind.as_str()
        )));
    }
    Ok(())
}
fn validate_name(name: &str, kind: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(StuffError::Validation(format!(
            "{kind} name cannot be blank"
        )));
    }
    Ok(())
}
fn unique_copy_name(names: &mut std::collections::BTreeSet<String>, source: &str) -> String {
    let base = format!("{source} (copy)");
    if names.insert(base.clone()) {
        return base;
    }
    let mut number = 2_u32;
    loop {
        let candidate = format!("{source} (copy {number})");
        if names.insert(candidate.clone()) {
            return candidate;
        }
        number = number.saturating_add(1);
    }
}
fn compartments_overlap(left: &Compartment, right: &Compartment) -> bool {
    left.cells.iter().any(|cell| right.cells.contains(cell))
}
fn location_in_compartment(location: &LocationRef, organizer_id: OrganizerId, key: &str) -> bool {
    location.organizer_id == Some(organizer_id) && location.compartment_key.as_deref() == Some(key)
}
fn stock_in_compartment(lot: &StockLot, organizer_id: OrganizerId, key: &str) -> bool {
    lot.location
        .compartment
        .as_ref()
        .is_some_and(|compartment| {
            compartment.organizer_id == organizer_id && compartment.key == key
        })
}

impl StuffService {
    pub async fn organizer_layout_summaries(&self) -> Vec<OrganizerLayoutSummary> {
        self.state
            .read()
            .await
            .organizer_templates
            .iter()
            .map(|template| OrganizerLayoutSummary {
                template_id: template.id,
                name: template.name.clone(),
                outer_size: template.size,
                cell_count: template.cells.len(),
                cells: template.cells.clone(),
            })
            .collect()
    }

    pub async fn create_organizer_template(
        &self,
        mut template: OrganizerTemplate,
    ) -> Result<TemplateId> {
        template.validate().map_err(StuffError::Validation)?;
        let mut next = self.snapshot().await;
        if template.id.0.is_nil() {
            template.id = TemplateId::new();
        } else if next
            .organizer_templates
            .iter()
            .any(|existing| existing.id == template.id)
        {
            return Err(StuffError::Invalid(
                "organizer template id already exists".into(),
            ));
        }
        let id = template.id;
        next.organizer_templates.push(template);
        self.commit(next, ChangeTopic::Organizer).await?;
        Ok(id)
    }

    pub async fn clone_organizer_template(
        &self,
        spec: CloneOrganizerTemplateSpec,
    ) -> Result<TemplateId> {
        validate_name(&spec.name, "organizer template")?;
        let mut next = self.snapshot().await;
        let source = next
            .organizer_templates
            .iter()
            .find(|template| template.id == spec.source_template_id)
            .cloned()
            .ok_or_else(|| StuffError::NotFound(spec.source_template_id.to_string()))?;
        let template = OrganizerTemplate {
            id: TemplateId::new(),
            name: spec.name,
            size: spec.outer_size,
            cells: scale_cells(&source, spec.outer_size).map_err(StuffError::Validation)?,
            dividers: source.dividers,
            metadata: spec.metadata,
        };
        template.validate().map_err(StuffError::Validation)?;
        let id = template.id;
        next.organizer_templates.push(template);
        self.commit(next, ChangeTopic::Organizer).await?;
        Ok(id)
    }

    /// Creates an organizer node and an instance referring to an existing
    /// reusable template type. It never duplicates the template.
    pub async fn create_organizer_from_template(
        &self,
        spec: CreateOrganizerFromTemplateSpec,
    ) -> Result<(NodeId, OrganizerId)> {
        validate_name(&spec.organizer_name, "organizer")?;
        let mut next = self.snapshot().await;
        let template = next
            .organizer_templates
            .iter()
            .find(|template| template.id == spec.template_id)
            .ok_or_else(|| StuffError::NotFound(spec.template_id.to_string()))?;
        template.validate().map_err(StuffError::Validation)?;
        let node_id = NodeId::new();
        let node = StorageNode {
            id: node_id,
            name: spec.organizer_name,
            kind: StorageKind::Organizer,
            parent_id: Some(spec.parent_id),
            placement: spec.placement,
            size: Some(template.size),
            metadata: spec.metadata,
        };
        validate_containment(&next.nodes, &node)?;
        let organizer_id = OrganizerId::new();
        next.nodes.push(node);
        next.organizers.push(OrganizerInstance {
            id: organizer_id,
            node_id,
            template_id: spec.template_id,
            removed_dividers: std::collections::BTreeSet::new(),
        });
        self.commit(next, ChangeTopic::Organizer).await?;
        Ok((node_id, organizer_id))
    }

    /// Converts a regular storage node to an organizer, or switches an existing
    /// organizer to another reusable type, without guessing where contents go.
    pub async fn assign_organizer_template(
        &self,
        node_id: NodeId,
        template_id: TemplateId,
    ) -> Result<()> {
        let mut next = self.snapshot().await;
        let template = next
            .organizer_templates
            .iter()
            .find(|template| template.id == template_id)
            .cloned()
            .ok_or_else(|| StuffError::NotFound(template_id.to_string()))?;
        template.validate().map_err(StuffError::Validation)?;
        let node_index = next
            .nodes
            .iter()
            .position(|node| node.id == node_id)
            .ok_or_else(|| StuffError::NotFound(node_id.to_string()))?;
        let mut proposed = next.nodes[node_index].clone();
        if proposed.kind != StorageKind::Organizer {
            let managed_instance = next
                .rack_instances
                .iter()
                .any(|instance| instance.rack_id == node_id)
                || next
                    .drawer_cabinet_instances
                    .iter()
                    .any(|instance| instance.cabinet_id == node_id)
                || generated_node_parent(&next, node_id).is_some();
            if managed_instance {
                return Err(StuffError::Validation(
                    "a generated or typed storage node cannot be converted to an organizer".into(),
                ));
            }
            proposed.kind = StorageKind::Organizer;
            validate_containment(&next.nodes, &proposed)?;
            if next
                .nodes
                .iter()
                .any(|child| child.parent_id == Some(node_id))
            {
                return Err(StuffError::Validation(
                    "an organizer cannot contain existing storage children".into(),
                ));
            }
        }
        if !template.cells.is_empty() && has_node_only_location_state(&next, node_id) {
            return Err(StuffError::Validation(
                "node-only stock, reservations, or occupancy must be cleared before assigning a celled organizer type"
                    .into(),
            ));
        }
        if let Some(instance_index) = next
            .organizers
            .iter()
            .position(|instance| instance.node_id == node_id)
        {
            let valid_removed = template
                .dividers
                .iter()
                .filter(|divider| divider.kind == DividerKind::Removable)
                .map(DividerEdge::key)
                .collect::<BTreeSet<_>>();
            let removed_dividers = next.organizers[instance_index]
                .removed_dividers
                .intersection(&valid_removed)
                .cloned()
                .collect::<BTreeSet<_>>();
            let keys = template
                .compartments(&removed_dividers)
                .into_iter()
                .map(|compartment| compartment.key)
                .collect::<BTreeSet<_>>();
            let organizer_id = next.organizers[instance_index].id;
            let referenced = next.stock.iter().any(|lot| {
                lot.quantity > 0
                    && lot
                        .location
                        .compartment
                        .as_ref()
                        .is_some_and(|compartment| {
                            compartment.organizer_id == organizer_id
                                && !keys.contains(&compartment.key)
                        })
            }) || next.occupancies.iter().any(|occupancy| {
                occupancy.location.organizer_id == Some(organizer_id)
                    && occupancy
                        .location
                        .compartment_key
                        .as_ref()
                        .is_some_and(|key| !keys.contains(key))
            }) || next.reservations.iter().any(|reservation| {
                reservation.location.organizer_id == Some(organizer_id)
                    && reservation
                        .location
                        .compartment_key
                        .as_ref()
                        .is_some_and(|key| !keys.contains(key))
            });
            if referenced {
                return Err(StuffError::Validation(
                    "the selected organizer type does not contain one or more referenced compartments"
                        .into(),
                ));
            }
            let instance = &mut next.organizers[instance_index];
            instance.template_id = template_id;
            instance.removed_dividers = removed_dividers;
        } else {
            next.organizers.push(OrganizerInstance {
                id: OrganizerId::new(),
                node_id,
                template_id,
                removed_dividers: BTreeSet::new(),
            });
        }
        proposed.size = Some(template.size);
        next.nodes[node_index] = proposed;
        self.commit(next, ChangeTopic::Organizer).await
    }

    /// Replaces a template's cells without asking callers to construct divider
    /// edges. Existing stock, occupancy, and reservations keep their stable
    /// compartment keys; deleting one of those keys is rejected.
    pub async fn update_organizer_template(&self, template: OrganizerTemplate) -> Result<()> {
        template.validate().map_err(StuffError::Validation)?;
        if template.id.0.is_nil() {
            return Err(StuffError::Invalid(
                "organizer template id is required for update".into(),
            ));
        }
        let mut next = self.snapshot().await;
        let index = next
            .organizer_templates
            .iter()
            .position(|existing| existing.id == template.id)
            .ok_or_else(|| StuffError::NotFound(template.id.to_string()))?;
        let old = next.organizer_templates[index].clone();
        let mut referenced = Vec::new();
        for organizer in next
            .organizers
            .iter()
            .filter(|organizer| organizer.template_id == template.id)
        {
            if !template.cells.is_empty() && has_node_only_location_state(&next, organizer.node_id)
            {
                return Err(StuffError::Validation(
                    "node-only stock, reservations, or occupancy must be cleared before adding organizer cells"
                        .into(),
                ));
            }
            let old_compartments = old.compartments(&organizer.removed_dividers);
            let new_keys: std::collections::BTreeSet<_> = template
                .compartments(&organizer.removed_dividers)
                .into_iter()
                .map(|compartment| compartment.key)
                .collect();
            for old_compartment in old_compartments {
                let key = &old_compartment.key;
                let is_referenced =
                    next.stock.iter().any(|lot| {
                        lot.quantity > 0 && stock_in_compartment(lot, organizer.id, key)
                    }) || next.occupancies.iter().any(|occupancy| {
                        location_in_compartment(&occupancy.location, organizer.id, key)
                    }) || next.reservations.iter().any(|reservation| {
                        location_in_compartment(&reservation.location, organizer.id, key)
                    });
                if is_referenced && !new_keys.contains(key) {
                    let cells = old_compartment
                        .cells
                        .iter()
                        .map(|cell| format!("{}:{}", cell.row, cell.column))
                        .collect::<Vec<_>>()
                        .join(", ");
                    referenced.push(format!("{key} (cells {cells})"));
                }
            }
        }
        if !referenced.is_empty() {
            return Err(StuffError::Validation(format!(
                "cannot delete stock-referenced organizer cells or compartments: {}",
                referenced.join("; ")
            )));
        }
        let valid_removed_dividers = template
            .dividers
            .iter()
            .filter(|divider| divider.kind == DividerKind::Removable)
            .map(DividerEdge::key)
            .collect::<BTreeSet<_>>();
        for organizer in next
            .organizers
            .iter_mut()
            .filter(|organizer| organizer.template_id == template.id)
        {
            organizer
                .removed_dividers
                .retain(|key| valid_removed_dividers.contains(key));
        }
        for organizer in next
            .organizers
            .iter()
            .filter(|organizer| organizer.template_id == template.id)
        {
            let node = next
                .nodes
                .iter_mut()
                .find(|node| node.id == organizer.node_id)
                .ok_or_else(|| StuffError::NotFound(organizer.node_id.to_string()))?;
            if node.kind != StorageKind::Organizer {
                return Err(StuffError::Validation(
                    "organizer type instance no longer points to an organizer".into(),
                ));
            }
            node.size = Some(template.size);
        }
        next.organizer_templates[index] = template;
        self.commit(next, ChangeTopic::Organizer).await
    }

    pub async fn save_organizer_as_template(
        &self,
        spec: SaveOrganizerAsTemplateSpec,
    ) -> Result<TemplateId> {
        validate_name(&spec.template_name, "organizer type")?;
        let mut next = self.snapshot().await;
        let node = next
            .nodes
            .iter()
            .find(|node| node.id == spec.organizer_node_id)
            .cloned()
            .ok_or_else(|| StuffError::NotFound(spec.organizer_node_id.to_string()))?;
        if node.kind != StorageKind::Organizer {
            return Err(StuffError::Validation(
                "only an organizer can be saved as an organizer type".into(),
            ));
        }
        let size = node.outer_dimensions().ok_or_else(|| {
            StuffError::Validation("organizer needs outer dimensions before saving a type".into())
        })?;
        let existing_instance = next
            .organizers
            .iter()
            .position(|organizer| organizer.node_id == node.id);
        let template = if let Some(instance_index) = existing_instance {
            let instance = &next.organizers[instance_index];
            let source = next
                .organizer_templates
                .iter()
                .find(|template| template.id == instance.template_id)
                .ok_or_else(|| StuffError::NotFound(instance.template_id.to_string()))?;
            OrganizerTemplate {
                id: TemplateId::new(),
                name: spec.template_name,
                size,
                cells: scale_cells(source, size).map_err(StuffError::Validation)?,
                dividers: source.dividers.clone(),
                metadata: spec.metadata,
            }
        } else {
            OrganizerTemplate {
                id: TemplateId::new(),
                name: spec.template_name,
                size,
                cells: vec![PrimitiveCell {
                    id: CellId { row: 0, column: 0 },
                    rect: Rect {
                        x: 0,
                        y: 0,
                        width: size.width,
                        depth: size.depth,
                    },
                }],
                dividers: Vec::new(),
                metadata: spec.metadata,
            }
        };
        template.validate().map_err(StuffError::Validation)?;
        let template_id = template.id;
        next.organizer_templates.push(template);
        if let Some(instance_index) = existing_instance {
            next.organizers[instance_index].template_id = template_id;
        } else {
            next.organizers.push(OrganizerInstance {
                id: OrganizerId::new(),
                node_id: node.id,
                template_id,
                removed_dividers: std::collections::BTreeSet::new(),
            });
        }
        self.commit(next, ChangeTopic::Organizer).await?;
        Ok(template_id)
    }

    /// Atomically creates a new organizer node, its independent template, and
    /// its instance. Empty dividers are deliberate: new editor flows only need
    /// explicit rectangular cells.
    pub async fn create_fresh_organizer(
        &self,
        spec: FreshOrganizerSpec,
    ) -> Result<(TemplateId, NodeId, OrganizerId)> {
        validate_name(&spec.template_name, "organizer template")?;
        validate_name(&spec.organizer_name, "organizer")?;
        let template = OrganizerTemplate {
            id: TemplateId::new(),
            name: spec.template_name,
            size: spec.outer_size,
            cells: spec.cells,
            dividers: Vec::new(),
            metadata: spec.template_metadata,
        };
        template.validate().map_err(StuffError::Validation)?;
        let mut next = self.snapshot().await;
        let node_id = NodeId::new();
        let node = StorageNode {
            id: node_id,
            name: spec.organizer_name,
            kind: StorageKind::Organizer,
            parent_id: Some(spec.parent_id),
            placement: spec.placement,
            size: Some(spec.outer_size),
            metadata: spec.organizer_metadata,
        };
        validate_containment(&next.nodes, &node)?;
        let organizer_id = OrganizerId::new();
        let template_id = template.id;
        next.organizer_templates.push(template);
        next.nodes.push(node);
        next.organizers.push(OrganizerInstance {
            id: organizer_id,
            node_id,
            template_id,
            removed_dividers: std::collections::BTreeSet::new(),
        });
        self.commit(next, ChangeTopic::Organizer).await?;
        Ok((template_id, node_id, organizer_id))
    }

    /// Clones a previous layout into a fresh template and instance. Width and
    /// depth changes scale every cell proportionally; height is outer-only.
    pub async fn clone_organizer_layout(
        &self,
        spec: CloneOrganizerSpec,
    ) -> Result<(TemplateId, NodeId, OrganizerId)> {
        let snapshot = self.snapshot().await;
        let source = snapshot
            .organizer_templates
            .iter()
            .find(|template| template.id == spec.source_template_id)
            .cloned()
            .ok_or_else(|| StuffError::NotFound(spec.source_template_id.to_string()))?;
        let cells = scale_cells(&source, spec.outer_size).map_err(StuffError::Validation)?;
        self.create_fresh_organizer(FreshOrganizerSpec {
            template_name: spec.template_name,
            organizer_name: spec.organizer_name,
            parent_id: spec.parent_id,
            outer_size: spec.outer_size,
            cells,
            placement: spec.placement,
            template_metadata: spec.template_metadata,
            organizer_metadata: spec.organizer_metadata,
        })
        .await
    }

    pub async fn create_organizer_instance(
        &self,
        mut instance: OrganizerInstance,
    ) -> Result<OrganizerId> {
        let mut next = self.snapshot().await;
        let node = next
            .nodes
            .iter()
            .find(|node| node.id == instance.node_id)
            .ok_or_else(|| StuffError::NotFound(instance.node_id.to_string()))?;
        if node.kind != StorageKind::Organizer {
            return Err(StuffError::Validation(
                "organizer instance must reference an organizer node".into(),
            ));
        }
        if next
            .organizers
            .iter()
            .any(|existing| existing.node_id == instance.node_id)
        {
            return Err(StuffError::Invalid(
                "storage node already has an organizer instance".into(),
            ));
        }
        let template = next
            .organizer_templates
            .iter()
            .find(|template| template.id == instance.template_id)
            .ok_or_else(|| StuffError::NotFound(instance.template_id.to_string()))?;
        instance
            .validate(template)
            .map_err(StuffError::Validation)?;
        if instance.id.0.is_nil() {
            instance.id = OrganizerId::new();
        } else if next
            .organizers
            .iter()
            .any(|existing| existing.id == instance.id)
        {
            return Err(StuffError::Invalid(
                "organizer instance id already exists".into(),
            ));
        }
        let id = instance.id;
        next.organizers.push(instance);
        self.commit(next, ChangeTopic::Organizer).await?;
        Ok(id)
    }

    pub async fn create_unplaced_organizer(
        &self,
        name: impl Into<String>,
        parent_id: Option<NodeId>,
        template_id: TemplateId,
    ) -> Result<(NodeId, OrganizerId)> {
        let name = name.into();
        validate_name(&name, "organizer")?;
        let mut next = self.snapshot().await;
        let template = next
            .organizer_templates
            .iter()
            .find(|template| template.id == template_id)
            .ok_or_else(|| StuffError::NotFound(template_id.to_string()))?;
        template.validate().map_err(StuffError::Validation)?;
        let node_id = NodeId::new();
        let organizer_id = OrganizerId::new();
        let node = StorageNode {
            id: node_id,
            name,
            kind: StorageKind::Organizer,
            parent_id,
            placement: None,
            size: Some(template.size),
            metadata: serde_json::Value::Null,
        };
        validate_containment(&next.nodes, &node)?;
        next.nodes.push(node);
        next.organizers.push(OrganizerInstance {
            id: organizer_id,
            node_id,
            template_id,
            removed_dividers: std::collections::BTreeSet::new(),
        });
        self.commit(next, ChangeTopic::Organizer).await?;
        Ok((node_id, organizer_id))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_grid_organizer(
        &self,
        template_name: impl Into<String>,
        organizer_name: impl Into<String>,
        parent_id: Option<NodeId>,
        size: Size3,
        rows: u32,
        columns: u32,
        divider_kind: DividerKind,
    ) -> Result<(TemplateId, NodeId, OrganizerId)> {
        let organizer_name = organizer_name.into();
        validate_name(&organizer_name, "organizer")?;
        let template = grid_template(template_name, size, rows, columns, divider_kind)
            .map_err(StuffError::Validation)?;
        template.validate().map_err(StuffError::Validation)?;
        let mut next = self.snapshot().await;
        let template_id = template.id;
        let node_id = NodeId::new();
        let organizer_id = OrganizerId::new();
        next.organizer_templates.push(template);
        let node = StorageNode {
            id: node_id,
            name: organizer_name,
            kind: StorageKind::Organizer,
            parent_id,
            placement: None,
            size: Some(size),
            metadata: serde_json::Value::Null,
        };
        validate_containment(&next.nodes, &node)?;
        next.nodes.push(node);
        next.organizers.push(OrganizerInstance {
            id: organizer_id,
            node_id,
            template_id,
            removed_dividers: std::collections::BTreeSet::new(),
        });
        self.commit(next, ChangeTopic::Organizer).await?;
        Ok((template_id, node_id, organizer_id))
    }
}
fn remap_stock_compartment(
    lot: &mut StockLot,
    organizer_id: OrganizerId,
    remaps: &BTreeMap<String, String>,
) {
    let Some(compartment) = lot.location.compartment.as_mut() else {
        return;
    };
    if compartment.organizer_id != organizer_id {
        return;
    }
    if let Some(key) = remaps.get(&compartment.key) {
        compartment.key = key.clone();
    }
}
fn scale_cells(
    source: &OrganizerTemplate,
    target_size: Size3,
) -> std::result::Result<Vec<PrimitiveCell>, String> {
    if target_size.width <= 0 || target_size.depth <= 0 || target_size.height <= 0 {
        return Err("organizer dimensions must be positive".into());
    }
    let scale = |value: Mm, source_extent: Mm, target_extent: Mm| {
        value
            .checked_mul(target_extent)
            .ok_or_else(|| "organizer cell scaling overflow".to_string())
            .map(|scaled| scaled / source_extent)
    };
    source
        .cells
        .iter()
        .map(|cell| {
            let right = cell
                .rect
                .x
                .checked_add(cell.rect.width)
                .ok_or_else(|| "organizer cell extent overflow".to_string())?;
            let bottom = cell
                .rect
                .y
                .checked_add(cell.rect.depth)
                .ok_or_else(|| "organizer cell extent overflow".to_string())?;
            let x = scale(cell.rect.x, source.size.width, target_size.width)?;
            let y = scale(cell.rect.y, source.size.depth, target_size.depth)?;
            let scaled_right = scale(right, source.size.width, target_size.width)?;
            let scaled_bottom = scale(bottom, source.size.depth, target_size.depth)?;
            Ok(PrimitiveCell {
                id: cell.id,
                rect: Rect {
                    x,
                    y,
                    width: scaled_right - x,
                    depth: scaled_bottom - y,
                },
            })
        })
        .collect()
}
fn remap_location_compartment(
    location: &mut LocationRef,
    organizer_id: OrganizerId,
    remaps: &BTreeMap<String, String>,
) {
    if location.organizer_id != Some(organizer_id) {
        return;
    }
    if let Some(current) = location.compartment_key.as_ref()
        && let Some(key) = remaps.get(current)
    {
        location.compartment_key = Some(key.clone());
    }
}
fn coalesce_stock(stock: &mut Vec<StockLot>) -> Result<()> {
    let mut coalesced = Vec::with_capacity(stock.len());
    for lot in stock.drain(..) {
        if let Some(existing) = coalesced.iter_mut().find(|existing: &&mut StockLot| {
            existing.item_id == lot.item_id && existing.location == lot.location
        }) {
            existing.quantity = existing
                .quantity
                .checked_add(lot.quantity)
                .ok_or_else(|| StuffError::Validation("quantity overflow".into()))?;
        } else {
            coalesced.push(lot);
        }
    }
    *stock = coalesced;
    Ok(())
}
fn occupancy_key(l: &LocationRef) -> (NodeId, Option<OrganizerId>, Option<&str>) {
    (l.node_id, l.organizer_id, l.compartment_key.as_deref())
}
fn location_matches_ref(a: &LocationRef, b: &LocationRef) -> bool {
    occupancy_key(a) == occupancy_key(b)
}
fn stock_matches_ref(stock: &StockLot, location: &LocationRef) -> bool {
    stock.location.node_id == location.node_id
        && stock
            .location
            .compartment
            .as_ref()
            .map(|c| (c.organizer_id, c.key.as_str()))
            == location
                .organizer_id
                .zip(location.compartment_key.as_deref())
}
fn fill_percent(fill: FillEstimate) -> u8 {
    fill.percent()
}
fn validate_stock_location(snapshot: &Snapshot, location: &StockLocation) -> Result<()> {
    let node = snapshot
        .nodes
        .iter()
        .find(|n| n.id == location.node_id)
        .ok_or_else(|| StuffError::NotFound(location.node_id.to_string()))?;
    if !node.kind.accepts_inventory() {
        return Err(StuffError::Validation(format!(
            "{} is not an inventory location",
            node.kind.as_str()
        )));
    }
    if location.compartment.is_none()
        && let Some(organizer) = snapshot
            .organizers
            .iter()
            .find(|organizer| organizer.node_id == location.node_id)
    {
        let template = snapshot
            .organizer_templates
            .iter()
            .find(|template| template.id == organizer.template_id)
            .ok_or_else(|| StuffError::NotFound(organizer.template_id.to_string()))?;
        if !template.cells.is_empty() {
            return Err(StuffError::Validation(
                "stock in a celled organizer must be assigned to a compartment".into(),
            ));
        }
    }
    if let Some(compartment) = &location.compartment {
        let organizer = snapshot
            .organizers
            .iter()
            .find(|o| o.id == compartment.organizer_id)
            .ok_or_else(|| StuffError::NotFound(compartment.organizer_id.to_string()))?;
        if organizer.node_id != location.node_id {
            return Err(StuffError::Validation(
                "organizer is not at the selected storage node".into(),
            ));
        }
        let template = snapshot
            .organizer_templates
            .iter()
            .find(|t| t.id == organizer.template_id)
            .ok_or_else(|| StuffError::NotFound(organizer.template_id.to_string()))?;
        if !organizer
            .compartments(template)
            .iter()
            .any(|c| c.key == compartment.key)
        {
            return Err(StuffError::Validation(
                "unknown organizer compartment".into(),
            ));
        }
    }
    Ok(())
}
fn validate_location_ref(snapshot: &Snapshot, location: &LocationRef) -> Result<()> {
    if location.organizer_id.is_some() != location.compartment_key.is_some() {
        return Err(StuffError::Validation(
            "organizer and compartment must be supplied together".into(),
        ));
    }
    validate_stock_location(
        snapshot,
        &StockLocation {
            node_id: location.node_id,
            compartment: location
                .organizer_id
                .zip(location.compartment_key.clone())
                .map(|(organizer_id, key)| CompartmentRef { organizer_id, key }),
        },
    )
}
fn is_descendant(nodes: &[StorageNode], candidate: NodeId, ancestor: NodeId) -> bool {
    let mut current = Some(candidate);
    while let Some(id) = current {
        if id == ancestor {
            return true;
        }
        current = nodes.iter().find(|n| n.id == id).and_then(|n| n.parent_id);
    }
    false
}
pub fn path_for(nodes: &[StorageNode], id: NodeId) -> Result<String> {
    let mut names = Vec::new();
    let mut current = Some(id);
    while let Some(node_id) = current {
        let node = nodes
            .iter()
            .find(|n| n.id == node_id)
            .ok_or_else(|| StuffError::NotFound(node_id.to_string()))?;
        names.push(node.name.clone());
        current = node.parent_id;
    }
    names.reverse();
    Ok(names.join(" / "))
}
pub fn resolve_selector(nodes: &[StorageNode], selector: &str) -> Result<SelectorMatch> {
    if let Ok(id) = selector.parse::<NodeId>() {
        let node = nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or_else(|| StuffError::NotFound(selector.into()))?;
        return Ok(SelectorMatch {
            id,
            path: path_for(nodes, id)?,
            name: node.name.clone(),
        });
    }
    let normalized = selector.trim().to_ascii_lowercase();
    let mut matches = nodes
        .iter()
        .filter(|n| n.name.to_ascii_lowercase() == normalized)
        .collect::<Vec<_>>();
    if matches.is_empty() {
        let path_matches = nodes
            .iter()
            .filter(|n| {
                path_for(nodes, n.id)
                    .map(|p| p.to_ascii_lowercase() == normalized)
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        matches = path_matches;
    }
    match matches.as_slice() {
        [] => Err(StuffError::NotFound(selector.into())),
        [node] => Ok(SelectorMatch {
            id: node.id,
            path: path_for(nodes, node.id)?,
            name: node.name.clone(),
        }),
        _ => Err(StuffError::Ambiguous {
            selector: selector.into(),
            candidates: matches
                .iter()
                .filter_map(|n| {
                    path_for(nodes, n.id).ok().map(|path| AmbiguousCandidate {
                        id: n.id.to_string(),
                        name: n.name.clone(),
                        path: Some(path),
                    })
                })
                .collect(),
        }),
    }
}
pub fn resolve_item_selector(items: &[Item], selector: &str) -> Result<ItemSelectorMatch> {
    if let Ok(id) = selector.parse::<ItemId>() {
        let item = items
            .iter()
            .find(|i| i.id == id)
            .ok_or_else(|| StuffError::NotFound(selector.into()))?;
        return Ok(ItemSelectorMatch {
            id,
            name: item.name.clone(),
            manufacturer_part_number: item.manufacturer_part_number.clone(),
        });
    }
    let q = selector.trim().to_ascii_lowercase().replace(' ', "");
    let matches = items
        .iter()
        .filter(|i| {
            format!(
                "{} {} {}",
                i.name,
                i.manufacturer_part_number.as_deref().unwrap_or(""),
                i.tags.join(" ")
            )
            .to_ascii_lowercase()
            .replace(' ', "")
            .contains(&q)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Err(StuffError::NotFound(selector.into())),
        [item] => Ok(ItemSelectorMatch {
            id: item.id,
            name: item.name.clone(),
            manufacturer_part_number: item.manufacturer_part_number.clone(),
        }),
        _ => Err(StuffError::Ambiguous {
            selector: selector.into(),
            candidates: matches
                .iter()
                .map(|i| AmbiguousCandidate {
                    id: i.id.to_string(),
                    name: i.name.clone(),
                    path: None,
                })
                .collect(),
        }),
    }
}
fn normalized_item_identity(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn exact_item_identity_matches(items: &[Item], requested: &Item) -> Vec<Item> {
    let name = normalized_item_identity(&requested.name);
    let part_number = requested
        .manufacturer_part_number
        .as_deref()
        .map(normalized_item_identity)
        .filter(|value| !value.is_empty());
    items
        .iter()
        .filter(|item| {
            normalized_item_identity(&item.name) == name
                || part_number.as_ref().is_some_and(|part_number| {
                    item.manufacturer_part_number
                        .as_deref()
                        .map(normalized_item_identity)
                        .as_ref()
                        == Some(part_number)
                })
        })
        .cloned()
        .collect()
}
pub fn resolve_location_selector(
    snapshot: &Snapshot,
    selector: &str,
) -> Result<LocationSelectorMatch> {
    match resolve_selector(&snapshot.nodes, selector) {
        Ok(node) => {
            return Ok(LocationSelectorMatch {
                location: StockLocation {
                    node_id: node.id,
                    compartment: None,
                },
                path: node.path,
            });
        }
        Err(StuffError::NotFound(_)) => {}
        Err(error) => return Err(error),
    }
    let q = selector.trim().to_ascii_lowercase();
    let mut matches = Vec::new();
    for organizer in &snapshot.organizers {
        let Some(template) = snapshot
            .organizer_templates
            .iter()
            .find(|t| t.id == organizer.template_id)
        else {
            continue;
        };
        for compartment in organizer.compartments(template) {
            let location = StockLocation {
                node_id: organizer.node_id,
                compartment: Some(CompartmentRef {
                    organizer_id: organizer.id,
                    key: compartment.key.clone(),
                }),
            };
            let path = display_location(snapshot, &location).unwrap_or_default();
            let normalized_path = path.to_ascii_lowercase();
            if normalized_path == q
                || normalized_path.ends_with(&format!(" / {q}"))
                || compartment.key.to_ascii_lowercase() == q
            {
                matches.push(LocationSelectorMatch { location, path });
            }
        }
    }
    match matches.as_slice() {
        [] => Err(StuffError::NotFound(selector.into())),
        [one] => Ok(one.clone()),
        _ => Err(StuffError::Ambiguous {
            selector: selector.into(),
            candidates: matches
                .iter()
                .map(|m| AmbiguousCandidate {
                    id: m
                        .location
                        .compartment
                        .as_ref()
                        .map(|compartment| {
                            format!("{}:{}", compartment.organizer_id, compartment.key)
                        })
                        .unwrap_or_else(|| m.location.node_id.to_string()),
                    name: m.path.clone(),
                    path: Some(m.path.clone()),
                })
                .collect(),
        }),
    }
}
pub fn item_selector_candidates(items: &[Item], selector: &str) -> Vec<ItemSelectorCandidate> {
    let q = selector.trim().to_ascii_lowercase().replace(' ', "");
    items
        .iter()
        .filter(|i| {
            format!(
                "{} {} {}",
                i.name,
                i.manufacturer_part_number.as_deref().unwrap_or(""),
                i.tags.join(" ")
            )
            .to_ascii_lowercase()
            .replace(' ', "")
            .contains(&q)
        })
        .map(|i| ItemSelectorCandidate {
            id: i.id,
            name: i.name.clone(),
            manufacturer_part_number: i.manufacturer_part_number.clone(),
        })
        .collect()
}

pub fn demo_snapshot() -> Snapshot {
    let room = StorageNode {
        id: NodeId::new(),
        name: "Bedroom".into(),
        kind: StorageKind::Room,
        parent_id: None,
        placement: None,
        size: Some(Size3 {
            width: 4000,
            depth: 3500,
            height: 2400,
        }),
        metadata: serde_json::json!({"demo":true}),
    };
    let rack = StorageNode {
        id: NodeId::new(),
        name: "Rack A".into(),
        kind: StorageKind::Rack,
        parent_id: Some(room.id),
        placement: Some(Placement {
            position: Point3 {
                x: 200,
                y: 200,
                z: 0,
            },
        }),
        size: Some(Size3 {
            width: 900,
            depth: 450,
            height: 1800,
        }),
        metadata: serde_json::Value::Null,
    };
    let shelf = StorageNode {
        id: NodeId::new(),
        name: "Shelf 3".into(),
        kind: StorageKind::Shelf,
        parent_id: Some(rack.id),
        placement: Some(Placement {
            position: Point3 { x: 5, y: 5, z: 820 },
        }),
        size: Some(Size3 {
            width: 880,
            depth: 430,
            height: 310,
        }),
        metadata: serde_json::Value::Null,
    };
    let organizer_node = StorageNode {
        id: NodeId::new(),
        name: "ICs Organizer".into(),
        kind: StorageKind::Organizer,
        parent_id: Some(shelf.id),
        placement: Some(Placement {
            position: Point3 { x: 40, y: 30, z: 0 },
        }),
        size: Some(Size3 {
            width: 340,
            depth: 220,
            height: 50,
        }),
        metadata: serde_json::json!({"color":"blue"}),
    };
    let box_node = StorageNode {
        id: NodeId::new(),
        name: "Cable Box".into(),
        kind: StorageKind::Box,
        parent_id: Some(shelf.id),
        placement: Some(Placement {
            position: Point3 {
                x: 420,
                y: 20,
                z: 0,
            },
        }),
        size: Some(Size3 {
            width: 260,
            depth: 180,
            height: 120,
        }),
        metadata: serde_json::Value::Null,
    };
    let drawer = StorageNode {
        id: NodeId::new(),
        name: "Bench Drawer".into(),
        kind: StorageKind::Drawer,
        parent_id: Some(shelf.id),
        placement: Some(Placement {
            position: Point3 {
                x: 40,
                y: 260,
                z: 0,
            },
        }),
        size: Some(Size3 {
            width: 360,
            depth: 150,
            height: 70,
        }),
        metadata: serde_json::json!({"view":"top-down"}),
    };
    let fixture = StorageNode {
        id: NodeId::new(),
        name: "Work Desk".into(),
        kind: StorageKind::Fixture,
        parent_id: Some(room.id),
        placement: Some(Placement {
            position: Point3 {
                x: 1450,
                y: 400,
                z: 0,
            },
        }),
        size: Some(Size3 {
            width: 1200,
            depth: 650,
            height: 740,
        }),
        metadata: serde_json::json!({"fixture":"desk"}),
    };
    let cells = (0..2)
        .flat_map(|row| {
            (0..3).map(move |column| PrimitiveCell {
                id: CellId { row, column },
                rect: Rect {
                    x: column as i64 * 100,
                    y: row as i64 * 100,
                    width: 100,
                    depth: 100,
                },
            })
        })
        .collect::<Vec<_>>();
    let mut dividers = Vec::new();
    for row in 0..2 {
        for column in 0..2 {
            dividers.push(DividerEdge {
                a: CellId { row, column },
                b: CellId {
                    row,
                    column: column + 1,
                },
                axis: DividerAxis::Vertical,
                kind: DividerKind::Removable,
            });
        }
    }
    for column in 0..3 {
        dividers.push(DividerEdge {
            a: CellId { row: 0, column },
            b: CellId { row: 1, column },
            axis: DividerAxis::Horizontal,
            kind: DividerKind::Fixed,
        });
    }
    let template = OrganizerTemplate {
        id: TemplateId::new(),
        name: "Blue 6-cell organizer".into(),
        size: Size3 {
            width: 340,
            depth: 220,
            height: 50,
        },
        cells,
        dividers,
        metadata: serde_json::Value::Null,
    };
    let organizer = OrganizerInstance {
        id: OrganizerId::new(),
        node_id: organizer_node.id,
        template_id: template.id,
        removed_dividers: std::collections::BTreeSet::new(),
    };
    let item = Item {
        id: ItemId::new(),
        name: "MCP6022-I/P".into(),
        manufacturer: Some("Microchip".into()),
        manufacturer_part_number: Some("MCP6022-I/P".into()),
        category: Some("op-amp".into()),
        notes: None,
        tags: vec!["mcp6022".into(), "electronics".into()],
        size: Some(Size3 {
            width: 10,
            depth: 10,
            height: 5,
        }),
        metadata: serde_json::Value::Null,
    };
    let item2 = Item {
        id: ItemId::new(),
        name: "JST XH cables".into(),
        manufacturer: None,
        manufacturer_part_number: None,
        category: Some("cables".into()),
        notes: None,
        tags: vec!["jst".into()],
        size: None,
        metadata: serde_json::Value::Null,
    };
    let compartment = template.compartments(&std::collections::BTreeSet::new())[0]
        .key
        .clone();
    let stock = vec![
        StockLot {
            id: StockLotId::new(),
            item_id: item.id,
            location: StockLocation {
                node_id: organizer_node.id,
                compartment: Some(CompartmentRef {
                    organizer_id: organizer.id,
                    key: compartment.clone(),
                }),
            },
            quantity: 18,
            placement: None,
        },
        StockLot {
            id: StockLotId::new(),
            item_id: item2.id,
            location: StockLocation {
                node_id: box_node.id,
                compartment: None,
            },
            quantity: 12,
            placement: None,
        },
    ];
    let reservation = Reservation {
        id: ReservationId::new(),
        location: LocationRef {
            node_id: organizer_node.id,
            organizer_id: Some(organizer.id),
            compartment_key: Some(
                template.compartments(&std::collections::BTreeSet::new())[1]
                    .key
                    .clone(),
            ),
        },
        label: "FluxPose".into(),
        notes: None,
    };
    let occupancy = Occupancy {
        location: LocationRef {
            node_id: organizer_node.id,
            organizer_id: Some(organizer.id),
            compartment_key: Some(compartment),
        },
        state: OccupancyState::Occupied,
        fill: FillEstimate::new(35),
    };
    let rack_template = RackTemplate {
        id: RackTemplateId::new(),
        name: "Workshop rack type".into(),
        outer_size: rack.size.unwrap(),
        shelves: vec![RackShelfTemplate {
            name: shelf.name.clone(),
            altitude: shelf.placement.unwrap().position.z,
            usable_height: shelf.size.unwrap().height,
            metadata: serde_json::Value::Null,
        }],
        metadata: serde_json::Value::Null,
    };
    let rack_template_id = rack_template.id;
    let rack_id = rack.id;
    let shelf_id = shelf.id;
    Snapshot {
        nodes: vec![room, rack, shelf, organizer_node, box_node, drawer, fixture],
        organizer_templates: vec![template],
        rack_templates: vec![rack_template],
        rack_instances: vec![RackInstance {
            rack_id,
            template_id: rack_template_id,
            shelf_ids: vec![shelf_id],
        }],
        organizers: vec![organizer],
        items: vec![item, item2],
        stock,
        reservations: vec![reservation],
        occupancies: vec![occupancy],
        ..Snapshot::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, kind: StorageKind, parent_id: Option<NodeId>) -> StorageNode {
        StorageNode {
            id: NodeId::default(),
            name: name.into(),
            kind,
            parent_id,
            placement: None,
            size: Some(Size3 {
                width: 100,
                depth: 100,
                height: 100,
            }),
            metadata: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn demo_tree_and_path() {
        let service = StuffService::demo();
        let roots = service.tree(None).await;
        assert_eq!(roots.len(), 1);
        let shelf = service.snapshot().await.nodes[2].id;
        assert_eq!(
            service.path(shelf).await.unwrap(),
            "Bedroom / Rack A / Shelf 3"
        );
    }
    #[tokio::test]
    async fn cell_less_organizer_is_node_only_and_typed_organizer_needs_a_type() {
        let service = StuffService::demo();
        let shelf = service.snapshot().await.nodes[2].id;
        let cell_less = service
            .create_node(node(
                "Undivided tray",
                StorageKind::CellLessOrganizer,
                Some(shelf),
            ))
            .await
            .unwrap();
        assert!(
            service
                .create_node(node(
                    "Untyped organizer",
                    StorageKind::Organizer,
                    Some(shelf)
                ))
                .await
                .is_err()
        );
        let (_, lot_id) = service
            .create_item_with_stock(
                Item {
                    id: ItemId::default(),
                    name: "Loose fastener".into(),
                    manufacturer: None,
                    manufacturer_part_number: None,
                    category: None,
                    notes: None,
                    tags: Vec::new(),
                    size: None,
                    metadata: serde_json::Value::Null,
                },
                StockLocation {
                    node_id: cell_less,
                    compartment: None,
                },
                5,
            )
            .await
            .unwrap();
        let snapshot = service.snapshot().await;
        assert!(
            !snapshot
                .organizers
                .iter()
                .any(|organizer| organizer.node_id == cell_less)
        );
        assert!(
            snapshot
                .stock
                .iter()
                .find(|lot| lot.id == lot_id)
                .is_some_and(|lot| lot.placement.is_none())
        );
    }

    #[tokio::test]
    async fn direct_room_stock_requires_dimensions_and_gets_a_workspace_position() {
        let service = StuffService::demo();
        let room = service.snapshot().await.nodes[0].id;
        let item = Item {
            id: ItemId::default(),
            name: "Bench supply".into(),
            manufacturer: None,
            manufacturer_part_number: None,
            category: None,
            notes: None,
            tags: Vec::new(),
            size: None,
            metadata: serde_json::Value::Null,
        };
        assert!(
            service
                .create_item_with_stock(
                    item.clone(),
                    StockLocation {
                        node_id: room,
                        compartment: None,
                    },
                    1,
                )
                .await
                .is_err()
        );

        let mut sized = item;
        sized.size = Some(Size3 {
            width: 120,
            depth: 80,
            height: 40,
        });
        let (item_id, lot_id) = service
            .create_item_with_stock(
                sized,
                StockLocation {
                    node_id: room,
                    compartment: None,
                },
                1,
            )
            .await
            .unwrap();
        let snapshot = service.snapshot().await;
        let lot = snapshot.stock.iter().find(|lot| lot.id == lot_id).unwrap();
        assert_eq!(lot.placement.unwrap().position, Point3::default());
        assert_eq!(
            snapshot
                .items
                .iter()
                .find(|item| item.id == item_id)
                .unwrap()
                .size,
            Some(Size3 {
                width: 120,
                depth: 80,
                height: 40,
            })
        );
        assert!(
            service
                .update_stock_placement(
                    lot_id,
                    Placement {
                        position: Point3 {
                            x: 10_000,
                            y: 0,
                            z: 0,
                        },
                    },
                )
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn moving_parent_rejects_cycle() {
        let s = StuffService::demo();
        let x = s.snapshot().await.nodes[1].id;
        let child = s.snapshot().await.nodes[2].id;
        assert!(s.move_node(x, Some(child), None).await.is_err());
    }
    #[tokio::test]
    async fn location_selector_accepts_a_human_path_suffix() {
        let service = StuffService::demo();
        let location = service
            .resolve_location_selector("ICs Organizer / 00")
            .await
            .unwrap();
        assert!(location.path.ends_with("ICs Organizer / 00"));
        assert!(location.location.compartment.is_some());
        let contents = service
            .contents_by_selector("ICs Organizer / 00")
            .await
            .unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].item_name, "MCP6022-I/P");
    }
    #[tokio::test]
    async fn free_location_filters_include_direct_spatial_workspaces() {
        let service = StuffService::demo();
        let all = service
            .free_locations_filtered(FreeLocationFilter::default())
            .await
            .unwrap();
        assert!(
            all.iter()
                .any(|location| location.path.ends_with("Bedroom"))
        );
        assert!(all.iter().any(|location| location.path.ends_with("Rack A")));
        assert!(
            all.iter()
                .all(|location| !location.path.ends_with("ICs Organizer"))
        );

        let reserved_empty = service
            .free_locations_filtered(FreeLocationFilter {
                reserved_empty: true,
                ..FreeLocationFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(reserved_empty.len(), 1);
        assert_eq!(reserved_empty[0].reserved_for.as_deref(), Some("FluxPose"));

        let completely_free = service
            .free_locations_filtered(FreeLocationFilter {
                completely_free: true,
                ..FreeLocationFilter::default()
            })
            .await
            .unwrap();
        assert!(completely_free.iter().all(|location| {
            location.state == OccupancyState::Free && location.reserved_for.is_none()
        }));
        assert!(
            service
                .free_locations_filtered(FreeLocationFilter {
                    max_fill: Some(101),
                    ..FreeLocationFilter::default()
                })
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn selector_stock_operations_validate_and_move_partial_quantities() {
        let service = StuffService::demo();
        assert!(
            service
                .add_stock_by_selector("mcp6022", "Shelf 3", 0)
                .await
                .is_err()
        );
        let shelf_lot = service
            .add_stock_by_selector("mcp6022", "Shelf 3", 5)
            .await
            .unwrap();
        service
            .adjust_stock_by_selector("mcp6022", "Shelf 3", 2)
            .await
            .unwrap();
        service
            .move_stock_by_selector("mcp6022", "Shelf 3", "Cable Box", Some(3))
            .await
            .unwrap();
        let snapshot = service.snapshot().await;
        assert_eq!(
            snapshot
                .stock
                .iter()
                .find(|lot| lot.id == shelf_lot)
                .unwrap()
                .quantity,
            4
        );
        assert_eq!(
            snapshot
                .stock
                .iter()
                .filter(|lot| lot.item_id == snapshot.items[0].id)
                .map(|lot| lot.quantity)
                .sum::<i64>(),
            25
        );
    }

    #[tokio::test]
    async fn moving_all_stock_prunes_the_empty_source_lot() {
        let service = StuffService::demo();
        let before = service.snapshot().await;
        let source = before
            .stock
            .iter()
            .find(|lot| lot.item_id == before.items[0].id)
            .unwrap()
            .id;
        service
            .move_stock_by_selector("mcp6022", "ICs Organizer / 00", "Cable Box", None)
            .await
            .unwrap();
        let after = service.snapshot().await;
        assert!(!after.stock.iter().any(|lot| lot.id == source));
        assert_eq!(
            after
                .stock
                .iter()
                .filter(|lot| lot.item_id == before.items[0].id)
                .map(|lot| lot.quantity)
                .sum::<i64>(),
            18
        );
    }

    #[tokio::test]
    async fn adjusting_all_stock_prunes_the_empty_lot() {
        let service = StuffService::demo();
        let before = service.snapshot().await;
        let source = before
            .stock
            .iter()
            .find(|lot| lot.item_id == before.items[0].id)
            .unwrap();

        service
            .adjust_stock(source.id, -source.quantity)
            .await
            .unwrap();

        assert!(
            !service
                .snapshot()
                .await
                .stock
                .iter()
                .any(|lot| lot.id == source.id)
        );
    }

    #[tokio::test]
    async fn divider_merge_remaps_contents_and_coalesces_locations() {
        let service = StuffService::demo();
        let before = service.snapshot().await;
        let organizer = before.organizers[0].clone();
        let template = before
            .organizer_templates
            .iter()
            .find(|template| template.id == organizer.template_id)
            .unwrap();
        let divider = template
            .dividers
            .iter()
            .find(|divider| divider.kind == DividerKind::Removable)
            .unwrap()
            .clone();
        let old = organizer.compartments(template);
        let first = old[0].key.clone();
        let second = old[1].key.clone();
        service
            .add_stock(StockLot {
                id: StockLotId::new(),
                item_id: before.items[1].id,
                location: StockLocation {
                    node_id: organizer.node_id,
                    compartment: Some(CompartmentRef {
                        organizer_id: organizer.id,
                        key: second.clone(),
                    }),
                },
                quantity: 7,
                placement: None,
            })
            .await
            .unwrap();
        service
            .set_occupancy(Occupancy {
                location: LocationRef {
                    node_id: organizer.node_id,
                    organizer_id: Some(organizer.id),
                    compartment_key: Some(second.clone()),
                },
                state: OccupancyState::Occupied,
                fill: FillEstimate::new(40),
            })
            .await
            .unwrap();
        service
            .reserve(Reservation {
                id: ReservationId::new(),
                location: LocationRef {
                    node_id: organizer.node_id,
                    organizer_id: Some(organizer.id),
                    compartment_key: Some(second.clone()),
                },
                label: "merge test".into(),
                notes: None,
            })
            .await
            .unwrap();
        service
            .remove_divider(organizer.id, &divider)
            .await
            .unwrap();
        let after = service.snapshot().await;
        let merged_key = after.organizers[0]
            .compartments(
                after
                    .organizer_templates
                    .iter()
                    .find(|template| template.id == organizer.template_id)
                    .unwrap(),
            )
            .into_iter()
            .find(|compartment| compartment.key.contains("00") && compartment.key.contains("01"))
            .unwrap()
            .key;
        assert_eq!(
            after
                .stock
                .iter()
                .filter(|lot| {
                    lot.location
                        .compartment
                        .as_ref()
                        .is_some_and(|compartment| {
                            compartment.organizer_id == organizer.id
                                && compartment.key == merged_key
                        })
                })
                .count(),
            2
        );
        assert!(
            after
                .occupancies
                .iter()
                .any(|occupancy| occupancy.location.compartment_key.as_deref()
                    == Some(merged_key.as_str()))
        );
        assert!(after.reservations.iter().any(|reservation| {
            reservation.location.compartment_key.as_deref() == Some(merged_key.as_str())
        }));
        assert!(!after.stock.iter().any(|lot| {
            lot.location
                .compartment
                .as_ref()
                .is_some_and(|compartment| {
                    compartment.organizer_id == organizer.id
                        && (compartment.key == first || compartment.key == second)
                })
        }));
    }

    #[tokio::test]
    async fn populated_split_is_rejected_without_changing_snapshot() {
        let service = StuffService::demo();
        let before = service.snapshot().await;
        let organizer = before.organizers[0].clone();
        let template = before
            .organizer_templates
            .iter()
            .find(|template| template.id == organizer.template_id)
            .unwrap();
        let divider = template
            .dividers
            .iter()
            .find(|divider| divider.kind == DividerKind::Removable)
            .unwrap()
            .clone();
        service
            .remove_divider(organizer.id, &divider)
            .await
            .unwrap();
        let merged = service.snapshot().await;
        let result = service.set_divider(organizer.id, &divider, false).await;
        assert!(matches!(
            result,
            Err(StuffError::CompartmentSplitAmbiguous { .. })
        ));
        let after = service.snapshot().await;
        assert_eq!(
            serde_json::to_value(merged).unwrap(),
            serde_json::to_value(after).unwrap()
        );
    }

    #[tokio::test]
    async fn empty_split_succeeds() {
        let service = StuffService::demo();
        let before = service.snapshot().await;
        let organizer = before.organizers[0].clone();
        let template = before
            .organizer_templates
            .iter()
            .find(|template| template.id == organizer.template_id)
            .unwrap();
        let divider = template
            .dividers
            .iter()
            .filter(|divider| divider.kind == DividerKind::Removable)
            .nth(2)
            .unwrap()
            .clone();
        service
            .remove_divider(organizer.id, &divider)
            .await
            .unwrap();
        service
            .set_divider(organizer.id, &divider, false)
            .await
            .unwrap();
        assert!(
            service.snapshot().await.organizers[0]
                .removed_dividers
                .is_empty()
        );
    }

    #[tokio::test]
    async fn organizer_instances_share_template_but_keep_contents_and_dividers_independent() {
        let service = StuffService::demo();
        let initial = service.snapshot().await;
        let template = initial.organizer_templates[0].clone();
        let shelf = initial.nodes[2].id;
        let (node_a, organizer_a) = service
            .create_unplaced_organizer("A", Some(shelf), template.id)
            .await
            .unwrap();
        let (node_b, organizer_b) = service
            .create_unplaced_organizer("B", Some(shelf), template.id)
            .await
            .unwrap();
        let divider = template
            .dividers
            .iter()
            .find(|divider| divider.kind == DividerKind::Removable)
            .unwrap()
            .clone();
        service.remove_divider(organizer_a, &divider).await.unwrap();
        let current = service.snapshot().await;
        let first_key = current
            .organizers
            .iter()
            .find(|organizer| organizer.id == organizer_a)
            .unwrap()
            .compartments(&template)
            .into_iter()
            .next()
            .unwrap()
            .key;
        service
            .create_item_with_stock(
                Item {
                    id: ItemId::new(),
                    name: "instance-only".into(),
                    manufacturer: None,
                    manufacturer_part_number: None,
                    category: None,
                    notes: None,
                    tags: Vec::new(),
                    size: None,
                    metadata: serde_json::Value::Null,
                },
                StockLocation {
                    node_id: node_a,
                    compartment: Some(CompartmentRef {
                        organizer_id: organizer_a,
                        key: first_key,
                    }),
                },
                3,
            )
            .await
            .unwrap();
        let after = service.snapshot().await;
        assert!(
            after
                .organizers
                .iter()
                .find(|organizer| organizer.id == organizer_a)
                .unwrap()
                .removed_dividers
                .contains(&divider.key())
        );
        assert!(
            !after
                .organizers
                .iter()
                .find(|organizer| organizer.id == organizer_b)
                .unwrap()
                .removed_dividers
                .contains(&divider.key())
        );
        assert!(after.stock.iter().any(|lot| {
            lot.location.node_id == node_a
                && lot
                    .location
                    .compartment
                    .as_ref()
                    .is_some_and(|compartment| compartment.organizer_id == organizer_a)
        }));
        assert!(!after.stock.iter().any(|lot| lot.location.node_id == node_b));
        assert_eq!(current.organizer_templates[0].id, template.id);
    }

    #[tokio::test]
    async fn assigning_an_organizer_type_converts_atomically_and_preserves_location_rules() {
        let service = StuffService::demo();
        let snapshot = service.snapshot().await;
        let template_id = snapshot.organizer_templates[0].id;
        let shelf_id = snapshot
            .nodes
            .iter()
            .find(|node| node.name == "Shelf 3")
            .unwrap()
            .id;
        let empty_box = service
            .create_node(node("Empty box", StorageKind::Box, Some(shelf_id)))
            .await
            .unwrap();
        let before_assignment = service.snapshot().await.revision;

        service
            .assign_organizer_template(empty_box, template_id)
            .await
            .unwrap();

        let assigned = service.snapshot().await;
        assert_eq!(assigned.revision, before_assignment + 1);
        assert_eq!(
            assigned
                .nodes
                .iter()
                .find(|node| node.id == empty_box)
                .unwrap()
                .kind,
            StorageKind::Organizer
        );
        assert!(assigned.organizers.iter().any(
            |organizer| organizer.node_id == empty_box && organizer.template_id == template_id
        ));

        let stocked_box = assigned
            .nodes
            .iter()
            .find(|node| node.name == "Cable Box")
            .unwrap()
            .id;
        assert!(
            service
                .assign_organizer_template(stocked_box, template_id)
                .await
                .is_err()
        );
        assert_eq!(
            service
                .snapshot()
                .await
                .nodes
                .iter()
                .find(|node| node.id == stocked_box)
                .unwrap()
                .kind,
            StorageKind::Box
        );
    }

    #[tokio::test]
    async fn moving_container_preserves_organizer_and_stock() {
        let service = StuffService::demo();
        let before = service.snapshot().await;
        let organizer = before.organizers[0].clone();
        let stock_before: Vec<_> = before
            .stock
            .iter()
            .filter(|lot| lot.location.node_id == organizer.node_id)
            .map(|lot| (lot.id, lot.location.clone(), lot.quantity))
            .collect();
        let rack = before.nodes[1].id;
        service
            .move_node(
                organizer.node_id,
                Some(rack),
                Some(Placement {
                    position: Point3 {
                        x: 10,
                        y: 20,
                        z: 30,
                    },
                }),
            )
            .await
            .unwrap();
        let after = service.snapshot().await;
        let instance = after
            .organizers
            .iter()
            .find(|candidate| candidate.id == organizer.id)
            .unwrap();
        assert_eq!(instance.node_id, organizer.node_id);
        assert_eq!(
            after
                .stock
                .iter()
                .filter(|lot| lot.location.node_id == organizer.node_id)
                .map(|lot| (lot.id, lot.location.clone(), lot.quantity))
                .collect::<Vec<_>>(),
            stock_before
        );
    }

    #[tokio::test]
    async fn item_with_stock_is_atomic_on_invalid_location_and_update_keeps_stock_ids() {
        let service = StuffService::demo();
        let before = service.snapshot().await;
        let invalid = service
            .create_item_with_stock(
                Item {
                    id: ItemId::default(),
                    name: "should not persist".into(),
                    manufacturer: None,
                    manufacturer_part_number: None,
                    category: None,
                    notes: None,
                    tags: Vec::new(),
                    size: None,
                    metadata: serde_json::Value::Null,
                },
                StockLocation {
                    node_id: NodeId::new(),
                    compartment: None,
                },
                1,
            )
            .await;
        assert!(invalid.is_err());
        let unchanged = service.snapshot().await;
        assert_eq!(before.items.len(), unchanged.items.len());
        assert_eq!(before.stock.len(), unchanged.stock.len());

        let item = unchanged.items[0].clone();
        let stock_ids: Vec<_> = unchanged
            .stock
            .iter()
            .filter(|lot| lot.item_id == item.id)
            .map(|lot| lot.id)
            .collect();
        let mut updated = item.clone();
        updated.name = "MCP6022 updated".into();
        service.update_item(updated).await.unwrap();
        let after = service.snapshot().await;
        assert_eq!(
            after
                .items
                .iter()
                .find(|candidate| candidate.id == item.id)
                .unwrap()
                .name,
            "MCP6022 updated"
        );
        assert_eq!(
            after
                .stock
                .iter()
                .filter(|lot| lot.item_id == item.id)
                .map(|lot| lot.id)
                .collect::<Vec<_>>(),
            stock_ids
        );
    }

    #[tokio::test]
    async fn service_boundaries_reject_blank_names_zero_dimensions_and_free_stock() {
        let service = StuffService::demo();
        let snapshot = service.snapshot().await;

        let mut blank_node = snapshot.nodes[2].clone();
        blank_node.name = "  ".into();
        assert!(service.update_node(blank_node).await.is_err());
        let mut zero_sized = snapshot.nodes[2].clone();
        zero_sized.size = Some(Size3 {
            width: 0,
            depth: 430,
            height: 310,
        });
        assert!(service.update_node(zero_sized).await.is_err());
        let mut blank_custom_kind = snapshot.nodes[2].clone();
        blank_custom_kind.kind = StorageKind::Custom("  ".into());
        assert!(service.update_node(blank_custom_kind).await.is_err());

        let mut blank_item = snapshot.items[0].clone();
        blank_item.name.clear();
        assert!(service.create_item(blank_item).await.is_err());
        let mut blank_template = snapshot.organizer_templates[0].clone();
        blank_template.name = "".into();
        assert!(
            service
                .create_organizer_template(blank_template)
                .await
                .is_err()
        );

        let organizer = snapshot.organizers[0].clone();
        let mut wrong_kind = snapshot
            .nodes
            .iter()
            .find(|node| node.id == organizer.node_id)
            .unwrap()
            .clone();
        wrong_kind.kind = StorageKind::Box;
        assert!(service.update_node(wrong_kind).await.is_err());

        let lot = snapshot
            .stock
            .iter()
            .find(|lot| lot.quantity > 0 && lot.location.compartment.is_some())
            .unwrap();
        let compartment = lot.location.compartment.as_ref().unwrap();
        let free_write = service
            .set_occupancy(Occupancy {
                location: LocationRef {
                    node_id: lot.location.node_id,
                    organizer_id: Some(compartment.organizer_id),
                    compartment_key: Some(compartment.key.clone()),
                },
                state: OccupancyState::Free,
                fill: FillEstimate::new(0),
            })
            .await;
        assert!(free_write.is_err());
    }

    #[tokio::test]
    async fn database_constrained_ids_and_reservation_labels_are_validated_in_core() {
        let service = StuffService::demo();
        let snapshot = service.snapshot().await;
        let location = snapshot.reservations[0].location.clone();

        assert!(
            service
                .reserve(Reservation {
                    id: ReservationId::default(),
                    location: location.clone(),
                    label: "  ".into(),
                    notes: None,
                })
                .await
                .is_err()
        );

        let reservation_id = service
            .reserve(Reservation {
                id: ReservationId::default(),
                location: location.clone(),
                label: "Database parity".into(),
                notes: None,
            })
            .await
            .unwrap();
        assert!(!reservation_id.0.is_nil());
        assert!(
            service
                .reserve(Reservation {
                    id: reservation_id,
                    location,
                    label: "Duplicate id".into(),
                    notes: None,
                })
                .await
                .is_err()
        );

        let duplicate_lot_id = snapshot.stock[0].id;
        let destination = snapshot
            .nodes
            .iter()
            .find(|node| node.name == "Cable Box")
            .unwrap()
            .id;
        assert!(
            service
                .add_stock(StockLot {
                    id: duplicate_lot_id,
                    item_id: snapshot.stock[0].item_id,
                    location: StockLocation {
                        node_id: destination,
                        compartment: None,
                    },
                    quantity: 1,
                    placement: None,
                })
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn containment_drawers_and_fixtures_are_core_rules() {
        let service = StuffService::demo();
        let snapshot = service.snapshot().await;
        let room = snapshot.nodes[0].id;
        let shelf = snapshot.nodes[2].id;
        assert!(
            service
                .create_node(node("root box", StorageKind::Box, None))
                .await
                .is_err()
        );
        assert!(
            service
                .create_node(node("bad rack", StorageKind::Rack, Some(shelf)))
                .await
                .is_err()
        );
        assert!(
            service
                .create_node(node("bad shelf", StorageKind::Shelf, Some(shelf)))
                .await
                .is_err()
        );
        let fixture = service
            .create_node(node("Fixture", StorageKind::Fixture, Some(room)))
            .await
            .unwrap();
        assert!(
            service
                .create_node(node("inside fixture", StorageKind::Box, Some(fixture)))
                .await
                .is_err()
        );
        let drawer = service
            .create_node(node("Drawer", StorageKind::Drawer, Some(shelf)))
            .await
            .unwrap();
        assert!(
            service
                .create_node(node("inside drawer", StorageKind::Box, Some(drawer)))
                .await
                .is_ok()
        );
        let item = service.snapshot().await.items[0].clone();
        assert!(
            service
                .add_stock(StockLot {
                    id: StockLotId::new(),
                    item_id: item.id,
                    location: StockLocation {
                        node_id: fixture,
                        compartment: None
                    },
                    quantity: 1,
                    placement: None,
                })
                .await
                .is_err()
        );
        assert!(!StorageKind::Fixture.accepts_inventory());
        assert_eq!(
            service
                .snapshot()
                .await
                .nodes
                .iter()
                .find(|node| node.id == drawer)
                .unwrap()
                .dimension_semantics(),
            DimensionSemantics::Usable
        );
    }

    #[tokio::test]
    async fn typed_shelves_inherit_rack_footprint_and_keep_room_geometry() {
        let service = StuffService::demo();
        let snapshot = service.snapshot().await;
        let rack = snapshot.nodes[1].id;
        let room = snapshot.nodes[0].id;
        let rack_shelf = service
            .create_shelf(CreateShelfSpec::InRack {
                name: "Inherited".into(),
                rack_id: rack,
                altitude: 900,
                usable_height: 250,
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
        let wall_shelf = service
            .create_shelf(CreateShelfSpec::InRoom {
                name: "Wall".into(),
                room_id: room,
                usable_size: Size3 {
                    width: 500,
                    depth: 180,
                    height: 220,
                },
                position: Point3 {
                    x: 2500,
                    y: 50,
                    z: 1300,
                },
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
        let after = service.snapshot().await;
        let inherited = after
            .nodes
            .iter()
            .find(|node| node.id == rack_shelf)
            .unwrap();
        assert_eq!(inherited.usable_dimensions().unwrap().width, 890);
        assert_eq!(inherited.usable_dimensions().unwrap().depth, 440);
        assert_eq!(
            inherited.placement.unwrap().position,
            Point3 { x: 5, y: 5, z: 900 }
        );
        let wall = after
            .nodes
            .iter()
            .find(|node| node.id == wall_shelf)
            .unwrap();
        assert_eq!(wall.usable_dimensions().unwrap().width, 500);
        assert_eq!(wall.placement.unwrap().position.z, 1300);
    }

    #[tokio::test]
    async fn organizer_types_support_cells_clone_and_safe_destructive_updates() {
        let service = StuffService::demo();
        let parent = service.snapshot().await.nodes[2].id;
        let cells = vec![
            PrimitiveCell {
                id: CellId { row: 0, column: 0 },
                rect: Rect {
                    x: 0,
                    y: 0,
                    width: 50,
                    depth: 50,
                },
            },
            PrimitiveCell {
                id: CellId { row: 0, column: 1 },
                rect: Rect {
                    x: 50,
                    y: 0,
                    width: 50,
                    depth: 50,
                },
            },
        ];
        let (template_id, node_id, organizer_id) = service
            .create_fresh_organizer(FreshOrganizerSpec {
                template_name: "Parts tray".into(),
                organizer_name: "Tray A".into(),
                parent_id: parent,
                outer_size: Size3 {
                    width: 100,
                    depth: 50,
                    height: 30,
                },
                cells: cells.clone(),
                placement: None,
                template_metadata: serde_json::Value::Null,
                organizer_metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
        let cloned = service
            .clone_organizer_template(CloneOrganizerTemplateSpec {
                source_template_id: template_id,
                name: "Large parts tray".into(),
                outer_size: Size3 {
                    width: 200,
                    depth: 100,
                    height: 30,
                },
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
        let clone_node = service
            .create_organizer_from_template(CreateOrganizerFromTemplateSpec {
                organizer_name: "Tray B".into(),
                parent_id: parent,
                template_id: cloned,
                placement: None,
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap()
            .0;
        let snapshot = service.snapshot().await;
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .find(|node| node.id == clone_node)
                .unwrap()
                .size
                .unwrap()
                .width,
            200
        );
        let key = snapshot
            .organizers
            .iter()
            .find(|instance| instance.id == organizer_id)
            .unwrap()
            .compartments(
                snapshot
                    .organizer_templates
                    .iter()
                    .find(|template| template.id == template_id)
                    .unwrap(),
            )[0]
        .key
        .clone();
        service
            .add_stock(StockLot {
                id: StockLotId::new(),
                item_id: snapshot.items[0].id,
                location: StockLocation {
                    node_id,
                    compartment: Some(CompartmentRef { organizer_id, key }),
                },
                quantity: 1,
                placement: None,
            })
            .await
            .unwrap();
        let mut destructive = snapshot
            .organizer_templates
            .iter()
            .find(|template| template.id == template_id)
            .unwrap()
            .clone();
        destructive.cells.remove(0);
        assert!(
            service
                .update_organizer_template(destructive)
                .await
                .is_err()
        );
        let overlapping = service
            .create_fresh_organizer(FreshOrganizerSpec {
                template_name: "Bad".into(),
                organizer_name: "Bad".into(),
                parent_id: parent,
                outer_size: Size3 {
                    width: 100,
                    depth: 100,
                    height: 10,
                },
                cells: vec![
                    cells[0].clone(),
                    PrimitiveCell {
                        id: CellId { row: 1, column: 0 },
                        rect: Rect {
                            x: 25,
                            y: 0,
                            width: 50,
                            depth: 50,
                        },
                    },
                ],
                placement: None,
                template_metadata: serde_json::Value::Null,
                organizer_metadata: serde_json::Value::Null,
            })
            .await;
        assert!(overlapping.is_err());
    }

    #[tokio::test]
    async fn add_or_create_and_subtree_clone_are_atomic_and_inventory_free() {
        let service = StuffService::demo();
        let snapshot = service.snapshot().await;
        let box_id = snapshot
            .nodes
            .iter()
            .find(|node| node.name == "Cable Box")
            .unwrap()
            .id;
        let item = Item {
            id: ItemId::default(),
            name: "Widget X".into(),
            manufacturer: Some("Acme".into()),
            manufacturer_part_number: Some("WX-1".into()),
            category: None,
            notes: None,
            tags: vec![],
            size: None,
            metadata: serde_json::Value::Null,
        };
        let first = service
            .add_or_create_item_with_stock(AddOrCreateItemSpec {
                item: item.clone(),
                location: StockLocation {
                    node_id: box_id,
                    compartment: None,
                },
                quantity: 2,
            })
            .await
            .unwrap();
        let mut equivalent = item;
        equivalent.name = " widget   x ".into();
        let second = service
            .add_or_create_item_with_stock(AddOrCreateItemSpec {
                item: equivalent,
                location: StockLocation {
                    node_id: box_id,
                    compartment: None,
                },
                quantity: 3,
            })
            .await
            .unwrap();
        assert!(first.created_item && !second.created_item && first.item_id == second.item_id);
        let rack = snapshot.nodes[1].id;
        let copy = service.clone_storage_subtree(rack, None).await.unwrap();
        let after = service.snapshot().await;
        assert!(
            after
                .nodes
                .iter()
                .find(|node| node.id == copy)
                .unwrap()
                .placement
                .is_some()
        );
        let clone_ids: std::collections::BTreeSet<_> = after
            .nodes
            .iter()
            .filter(|node| node.name.contains("(copy"))
            .map(|node| node.id)
            .collect();
        assert!(clone_ids.len() >= 4);
        assert!(
            after
                .stock
                .iter()
                .all(|lot| !clone_ids.contains(&lot.location.node_id))
        );
        assert!(
            after
                .organizers
                .iter()
                .any(|organizer| clone_ids.contains(&organizer.node_id)
                    && organizer.template_id == snapshot.organizers[0].template_id)
        );
    }

    #[tokio::test]
    async fn deleting_storage_requires_an_empty_unannotated_subtree() {
        let service = StuffService::demo();
        let snapshot = service.snapshot().await;
        let fixture = snapshot
            .nodes
            .iter()
            .find(|node| node.kind == StorageKind::Fixture)
            .unwrap()
            .id;
        assert_eq!(service.delete_storage_subtree(fixture).await.unwrap(), 1);
        assert!(
            service
                .snapshot()
                .await
                .nodes
                .iter()
                .all(|node| node.id != fixture)
        );

        let stocked_node = snapshot.stock[0].location.node_id;
        let error = service
            .delete_storage_subtree(stocked_node)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("contains stock"));
    }

    #[tokio::test]
    async fn deleting_storage_can_orphan_direct_children() {
        let service = StuffService::demo();
        let snapshot = service.snapshot().await;
        let shelf = snapshot
            .nodes
            .iter()
            .find(|node| node.kind == StorageKind::Shelf)
            .unwrap()
            .id;
        let parent = service
            .create_node(StorageNode {
                id: NodeId::default(),
                name: "Temporary box".into(),
                kind: StorageKind::Box,
                parent_id: Some(shelf),
                placement: None,
                size: Some(Size3 {
                    width: 100,
                    depth: 100,
                    height: 100,
                }),
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
        let child = service
            .create_node(StorageNode {
                id: NodeId::default(),
                name: "Keep me".into(),
                kind: StorageKind::Box,
                parent_id: Some(parent),
                placement: Some(Placement {
                    position: Point3 { x: 1, y: 2, z: 3 },
                }),
                size: Some(Size3 {
                    width: 50,
                    depth: 50,
                    height: 50,
                }),
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
        assert_eq!(
            service
                .delete_storage_orphan_children(parent)
                .await
                .unwrap(),
            1
        );
        let moved_child = service
            .snapshot()
            .await
            .nodes
            .into_iter()
            .find(|node| node.id == child)
            .unwrap();
        assert_eq!(moved_child.parent_id, Some(shelf));
        assert!(moved_child.placement.is_none());
    }

    #[tokio::test]
    async fn reusable_type_updates_reach_existing_instances() {
        let service = StuffService::demo();
        let before = service.snapshot().await;
        let organizer_node = before
            .nodes
            .iter()
            .find(|node| node.kind == StorageKind::Organizer)
            .unwrap()
            .id;
        let mut organizer_type = before.organizer_templates[0].clone();
        organizer_type.size.width = 360;
        service
            .update_organizer_template(organizer_type)
            .await
            .unwrap();
        assert_eq!(
            service
                .snapshot()
                .await
                .nodes
                .iter()
                .find(|node| node.id == organizer_node)
                .unwrap()
                .size
                .unwrap()
                .width,
            360
        );

        let mut rack_type = before.rack_templates[0].clone();
        rack_type.outer_size.width = 1000;
        service.update_rack_template(rack_type).await.unwrap();
        let after_rack_update = service.snapshot().await;
        let rack_id = after_rack_update.rack_instances[0].rack_id;
        assert_eq!(
            after_rack_update
                .nodes
                .iter()
                .find(|node| node.id == rack_id)
                .unwrap()
                .size
                .unwrap()
                .width,
            1000
        );

        let organizer_type_id = service
            .save_organizer_as_template(SaveOrganizerAsTemplateSpec {
                organizer_node_id: organizer_node,
                template_name: "Captured organizer".into(),
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
        assert_eq!(
            service
                .snapshot()
                .await
                .organizers
                .iter()
                .find(|instance| instance.node_id == organizer_node)
                .unwrap()
                .template_id,
            organizer_type_id
        );

        let rack_type_id = service
            .save_rack_as_template(SaveRackAsTemplateSpec {
                rack_id,
                template_name: "Captured rack".into(),
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
        assert_eq!(
            service
                .snapshot()
                .await
                .rack_instances
                .iter()
                .find(|instance| instance.rack_id == rack_id)
                .unwrap()
                .template_id,
            rack_type_id
        );
    }

    #[tokio::test]
    async fn rack_types_round_trip_physical_shelf_layouts() {
        let service = StuffService::demo();
        let before = service.snapshot().await;
        let room = before
            .nodes
            .iter()
            .find(|node| node.kind == StorageKind::Room)
            .unwrap()
            .id;
        let rack = before
            .nodes
            .iter()
            .find(|node| node.kind == StorageKind::Rack)
            .unwrap()
            .id;
        let saved = service
            .save_rack_as_template(SaveRackAsTemplateSpec {
                rack_id: rack,
                template_name: "Rack type A".into(),
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
        let cloned = service
            .clone_rack_template(CloneRackTemplateSpec {
                source_template_id: saved,
                name: "Rack type B".into(),
                outer_size: Size3 {
                    width: 1000,
                    depth: 500,
                    height: 2000,
                },
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
        let instance = service
            .create_rack_from_template(CreateRackFromTemplateSpec {
                rack_name: "Rack B".into(),
                room_id: room,
                template_id: cloned,
                placement: Some(Placement {
                    position: Point3 {
                        x: 1000,
                        y: 200,
                        z: 0,
                    },
                }),
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
        let after = service.snapshot().await;
        let rack_node = after.nodes.iter().find(|node| node.id == instance).unwrap();
        assert_eq!(rack_node.outer_dimensions().unwrap().width, 1000);
        let shelves = after
            .nodes
            .iter()
            .filter(|node| node.parent_id == Some(instance) && node.kind == StorageKind::Shelf)
            .collect::<Vec<_>>();
        assert!(!shelves.is_empty());
        assert!(shelves.iter().all(|shelf| {
            shelf.usable_dimensions().unwrap().width == 990
                && shelf.usable_dimensions().unwrap().depth == 490
        }));
        assert!(
            after
                .stock
                .iter()
                .all(|lot| lot.location.node_id != instance)
        );
        let managed_shelf_id = after
            .rack_instances
            .iter()
            .find(|rack| rack.rack_id == instance)
            .unwrap()
            .shelf_ids[0];
        let mut invalid_shelf = after
            .nodes
            .iter()
            .find(|node| node.id == managed_shelf_id)
            .unwrap()
            .clone();
        invalid_shelf.kind = StorageKind::Box;
        assert!(service.update_node(invalid_shelf).await.is_err());
        assert!(
            service
                .move_node(managed_shelf_id, Some(room), None)
                .await
                .is_err()
        );
        assert!(
            service
                .delete_storage_subtree(managed_shelf_id)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn drawer_cabinet_types_generate_stable_inventory_capable_drawers() {
        let service = StuffService::demo();
        let before = service.snapshot().await;
        let room = before
            .nodes
            .iter()
            .find(|node| node.kind == StorageKind::Room)
            .unwrap()
            .id;
        let item_id = before.items[0].id;
        let template_id = service
            .create_drawer_cabinet_template(DrawerCabinetTemplate {
                id: DrawerCabinetTemplateId::default(),
                name: "Parts chest".into(),
                outer_size: Size3 {
                    width: 400,
                    depth: 300,
                    height: 300,
                },
                drawers: vec![
                    DrawerTemplateDrawer {
                        name: "Top".into(),
                        altitude: 0,
                        usable_size: Size3 {
                            width: 380,
                            depth: 280,
                            height: 100,
                        },
                        metadata: serde_json::Value::Null,
                    },
                    DrawerTemplateDrawer {
                        name: "Bottom".into(),
                        altitude: 100,
                        usable_size: Size3 {
                            width: 380,
                            depth: 280,
                            height: 150,
                        },
                        metadata: serde_json::Value::Null,
                    },
                ],
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
        let cabinet_id = service
            .create_drawer_cabinet_from_template(CreateDrawerCabinetFromTemplateSpec {
                cabinet_name: "Parts chest A".into(),
                parent_id: room,
                template_id,
                placement: None,
                metadata: serde_json::Value::Null,
            })
            .await
            .unwrap();
        let created = service.snapshot().await;
        let instance = created
            .drawer_cabinet_instances
            .iter()
            .find(|instance| instance.cabinet_id == cabinet_id)
            .unwrap()
            .clone();
        assert_eq!(instance.drawer_ids.len(), 2);
        assert!(instance.drawer_ids.iter().all(|id| {
            created
                .nodes
                .iter()
                .any(|node| node.id == *id && node.kind == StorageKind::Drawer)
        }));
        service
            .add_stock(StockLot {
                id: StockLotId::default(),
                item_id,
                location: StockLocation {
                    node_id: instance.drawer_ids[0],
                    compartment: None,
                },
                quantity: 4,
                placement: None,
            })
            .await
            .unwrap();

        let mut edited = service
            .snapshot()
            .await
            .drawer_cabinet_templates
            .into_iter()
            .find(|template| template.id == template_id)
            .unwrap();
        edited.drawers[0].name = "Upper".into();
        service
            .update_drawer_cabinet_template(edited)
            .await
            .unwrap();
        let updated = service.snapshot().await;
        assert_eq!(
            updated
                .nodes
                .iter()
                .find(|node| node.id == instance.drawer_ids[0])
                .unwrap()
                .name,
            "Upper"
        );
        assert!(
            updated
                .stock
                .iter()
                .any(|lot| { lot.location.node_id == instance.drawer_ids[0] && lot.quantity == 4 })
        );
        let mut invalid_cabinet = updated
            .nodes
            .iter()
            .find(|node| node.id == cabinet_id)
            .unwrap()
            .clone();
        invalid_cabinet.kind = StorageKind::Box;
        assert!(service.update_node(invalid_cabinet).await.is_err());

        let second_drawer = instance.drawer_ids[1];
        assert!(
            service
                .move_node(second_drawer, Some(room), None)
                .await
                .is_err()
        );
        assert!(service.delete_storage_subtree(second_drawer).await.is_err());
    }
}
