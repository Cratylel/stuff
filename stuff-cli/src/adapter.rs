//! Boundary between the command line and stuff-core.
//!
//! These records are presentation-facing. They contain the human-readable
//! location path rather than making the formatter know about database rows or
//! the storage tree. CoreAdapter is the only code that should translate the
//! core service into these records.

use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("database URL is required; pass --database-url or set STUFF_DATABASE_URL")]
    MissingDatabaseUrl,
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Core(String),
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FindResult {
    pub items: Vec<ItemResult>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ItemResult {
    pub id: String,
    pub name: String,
    pub total_quantity: i64,
    pub locations: Vec<StockResult>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StockResult {
    pub quantity: i64,
    pub location: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ContentsResult {
    pub location: String,
    pub stock: Vec<ContentsEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContentsEntry {
    pub item: String,
    pub quantity: i64,
    pub location: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct TreeResult {
    pub roots: Vec<TreeNode>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TreeNode {
    pub name: String,
    pub kind: String,
    pub id: String,
    pub children: Vec<TreeNode>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FreeResult {
    pub locations: Vec<FreeLocation>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FreeLocation {
    pub location: String,
    pub state: String,
    pub fill_percent: Option<u8>,
    pub reserved_for: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationOperation {
    Add,
    Adjust,
    Move,
}

#[derive(Clone, Debug, Serialize)]
pub struct MutationResult {
    pub operation: MutationOperation,
    pub item: String,
    pub quantity: Option<i64>,
    pub from: Option<String>,
    pub to: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FreeFilter {
    pub completely_free: bool,
    pub max_fill: Option<u8>,
    pub reserved_empty: bool,
}

#[async_trait]
pub trait Service: Send + Sync {
    async fn find(&self, query: &str) -> Result<FindResult, ServiceError>;
    async fn contents(&self, selector: &str) -> Result<ContentsResult, ServiceError>;
    async fn tree(&self, selector: Option<&str>) -> Result<TreeResult, ServiceError>;
    async fn free(&self, filter: FreeFilter) -> Result<FreeResult, ServiceError>;
    async fn add_stock(
        &self,
        item: &str,
        quantity: i64,
        location: &str,
    ) -> Result<MutationResult, ServiceError>;
    async fn adjust_stock(
        &self,
        item: &str,
        delta: i64,
        location: &str,
    ) -> Result<MutationResult, ServiceError>;
    async fn move_stock(
        &self,
        item: &str,
        quantity: Option<i64>,
        from: &str,
        to: &str,
    ) -> Result<MutationResult, ServiceError>;
}

/// Thin translation layer over the core service.
#[derive(Clone)]
pub struct CoreAdapter {
    service: stuff_core::StuffService,
}

impl CoreAdapter {
    pub async fn migrate(database_url: &str) -> Result<(), ServiceError> {
        stuff_core::StuffService::migrate(database_url)
            .await
            .map_err(core_error)
    }

    pub async fn connect(database_url: Option<&str>) -> Result<Self, ServiceError> {
        let database_url = database_url.ok_or(ServiceError::MissingDatabaseUrl)?;
        Ok(Self {
            service: stuff_core::StuffService::connect(database_url)
                .await
                .map_err(core_error)?,
        })
    }
}

#[async_trait]
impl Service for CoreAdapter {
    async fn find(&self, query: &str) -> Result<FindResult, ServiceError> {
        Ok(find_from_search(self.service.search(query).await))
    }

    async fn contents(&self, selector: &str) -> Result<ContentsResult, ServiceError> {
        let location = self
            .service
            .resolve_location_selector(selector)
            .await
            .map_err(core_error)?
            .path;
        let entries = self
            .service
            .contents_by_selector(selector)
            .await
            .map_err(core_error)?;
        let mut stock = entries
            .into_iter()
            .map(|entry| ContentsEntry {
                item: entry.item_name,
                quantity: entry.quantity,
                location: entry.location,
            })
            .collect::<Vec<_>>();
        stock.sort_by(|left, right| {
            left.location
                .to_ascii_lowercase()
                .cmp(&right.location.to_ascii_lowercase())
                .then_with(|| {
                    left.item
                        .to_ascii_lowercase()
                        .cmp(&right.item.to_ascii_lowercase())
                })
        });
        Ok(ContentsResult { location, stock })
    }

    async fn tree(&self, selector: Option<&str>) -> Result<TreeResult, ServiceError> {
        let root = match selector {
            Some(selector) => Some(
                self.service
                    .resolve_selector(selector)
                    .await
                    .map_err(core_error)?
                    .id,
            ),
            None => None,
        };
        Ok(TreeResult {
            roots: self
                .service
                .tree(root)
                .await
                .into_iter()
                .map(tree_node)
                .collect(),
        })
    }

    async fn free(&self, filter: FreeFilter) -> Result<FreeResult, ServiceError> {
        let locations = self
            .service
            .free_locations_filtered(stuff_core::FreeLocationFilter {
                completely_free: filter.completely_free,
                max_fill: filter.max_fill,
                reserved_empty: filter.reserved_empty,
            })
            .await
            .map_err(core_error)?;
        let mut locations = locations
            .into_iter()
            .map(|location| FreeLocation {
                location: location.path,
                state: occupancy_name(location.state).into(),
                fill_percent: location.fill_percent,
                reserved_for: location.reserved_for,
            })
            .collect::<Vec<_>>();
        locations.sort_by(|left, right| {
            left.location
                .to_ascii_lowercase()
                .cmp(&right.location.to_ascii_lowercase())
        });
        Ok(FreeResult { locations })
    }

    async fn add_stock(
        &self,
        item: &str,
        quantity: i64,
        location: &str,
    ) -> Result<MutationResult, ServiceError> {
        let item_name = self
            .service
            .resolve_item_selector(item)
            .await
            .map_err(core_error)?
            .name;
        let location_path = self
            .service
            .resolve_location_selector(location)
            .await
            .map_err(core_error)?
            .path;
        self.service
            .add_stock_by_selector(item, location, quantity)
            .await
            .map_err(core_error)?;
        Ok(MutationResult {
            operation: MutationOperation::Add,
            item: item_name,
            quantity: Some(quantity),
            from: None,
            to: location_path,
        })
    }

    async fn adjust_stock(
        &self,
        item: &str,
        delta: i64,
        location: &str,
    ) -> Result<MutationResult, ServiceError> {
        let item_name = self
            .service
            .resolve_item_selector(item)
            .await
            .map_err(core_error)?
            .name;
        let location_path = self
            .service
            .resolve_location_selector(location)
            .await
            .map_err(core_error)?
            .path;
        self.service
            .adjust_stock_by_selector(item, location, delta)
            .await
            .map_err(core_error)?;
        Ok(MutationResult {
            operation: MutationOperation::Adjust,
            item: item_name,
            quantity: Some(delta),
            from: None,
            to: location_path,
        })
    }

    async fn move_stock(
        &self,
        item: &str,
        quantity: Option<i64>,
        from: &str,
        to: &str,
    ) -> Result<MutationResult, ServiceError> {
        let item_name = self
            .service
            .resolve_item_selector(item)
            .await
            .map_err(core_error)?
            .name;
        let from_path = self
            .service
            .resolve_location_selector(from)
            .await
            .map_err(core_error)?
            .path;
        let to_path = self
            .service
            .resolve_location_selector(to)
            .await
            .map_err(core_error)?
            .path;
        self.service
            .move_stock_by_selector(item, from, to, quantity)
            .await
            .map_err(core_error)?;
        Ok(MutationResult {
            operation: MutationOperation::Move,
            item: item_name,
            quantity,
            from: Some(from_path),
            to: to_path,
        })
    }
}

fn core_error(error: stuff_core::StuffError) -> ServiceError {
    match error {
        stuff_core::StuffError::Ambiguous {
            selector,
            candidates,
        } => {
            let candidates = candidates
                .into_iter()
                .map(|candidate| {
                    let location = candidate.path.unwrap_or(candidate.name);
                    format!("{location} ({})", candidate.id)
                })
                .collect::<Vec<_>>()
                .join(", ");
            ServiceError::Core(format!(
                "selector '{selector}' is ambiguous; candidates: {candidates}"
            ))
        }
        error => ServiceError::Core(error.to_string()),
    }
}

fn find_from_search(search: stuff_core::SearchResults) -> FindResult {
    let mut items = search
        .items
        .into_iter()
        .map(|item| ItemResult {
            id: item.item.id.to_string(),
            name: item.item.name,
            total_quantity: item.total_quantity,
            locations: Vec::new(),
        })
        .collect::<Vec<_>>();
    for location in search.locations {
        if let Some(item) = items
            .iter_mut()
            .find(|item| item.id == location.item_id.to_string())
        {
            item.locations.push(StockResult {
                quantity: location.quantity,
                location: location.location,
            });
        }
    }
    for item in &mut items {
        item.locations.sort_by(|left, right| {
            left.location
                .to_ascii_lowercase()
                .cmp(&right.location.to_ascii_lowercase())
        });
    }
    items.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    FindResult { items }
}

fn tree_node(node: stuff_core::TreeNode) -> TreeNode {
    TreeNode {
        name: node.node.name,
        kind: node.node.kind.as_str().to_owned(),
        id: node.node.id.to_string(),
        children: node.children.into_iter().map(tree_node).collect(),
    }
}

fn occupancy_name(state: stuff_core::OccupancyState) -> &'static str {
    match state {
        stuff_core::OccupancyState::Free => "free",
        stuff_core::OccupancyState::Occupied => "occupied",
        stuff_core::OccupancyState::Reserved => "reserved",
        stuff_core::OccupancyState::Blocked => "blocked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stuff_core::inventory::{ItemMatch, StockLocationResult};
    use stuff_core::{Item, ItemId, SearchResults};

    fn item(id: ItemId, name: &str) -> Item {
        Item {
            id,
            name: name.into(),
            manufacturer: None,
            manufacturer_part_number: None,
            category: None,
            notes: None,
            tags: vec![],
            size: None,
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn search_translation_groups_locations_and_sorts_output() {
        let alpha_id = ItemId::new();
        let zulu_id = ItemId::new();
        let result = find_from_search(SearchResults {
            items: vec![
                ItemMatch {
                    item: item(zulu_id, "Zulu"),
                    total_quantity: 1,
                    lots: vec![],
                },
                ItemMatch {
                    item: item(alpha_id, "alpha"),
                    total_quantity: 5,
                    lots: vec![],
                },
            ],
            locations: vec![
                StockLocationResult {
                    item_id: alpha_id,
                    item_name: "alpha".into(),
                    quantity: 2,
                    location: "Room B / Box".into(),
                },
                StockLocationResult {
                    item_id: zulu_id,
                    item_name: "Zulu".into(),
                    quantity: 1,
                    location: "Room C".into(),
                },
                StockLocationResult {
                    item_id: alpha_id,
                    item_name: "alpha".into(),
                    quantity: 3,
                    location: "Room A / D4".into(),
                },
            ],
        });

        assert_eq!(result.items[0].name, "alpha");
        assert_eq!(result.items[1].name, "Zulu");
        assert_eq!(result.items[0].locations[0].location, "Room A / D4");
        assert_eq!(result.items[0].locations[1].location, "Room B / Box");
        assert_eq!(result.items[1].locations[0].location, "Room C");
    }
}
