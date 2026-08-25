use crate::domain::{ItemId, Mm, NodeId, OrganizerId, Placement, Size3, StockLotId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub name: String,
    pub manufacturer: Option<String>,
    pub manufacturer_part_number: Option<String>,
    pub category: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    /// Physical dimensions of one item. Required when the item is stored
    /// directly in a room, rack, or shelf workspace.
    #[serde(default)]
    pub size: Option<Size3>,
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StockLocation {
    pub node_id: NodeId,
    pub compartment: Option<CompartmentRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompartmentRef {
    pub organizer_id: OrganizerId,
    pub key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StockLot {
    pub id: StockLotId,
    pub item_id: ItemId,
    pub location: StockLocation,
    pub quantity: i64,
    /// Position inside a spatial stock location. Container and compartment
    /// lots deliberately have no placement.
    #[serde(default)]
    pub placement: Option<Placement>,
}

impl StockLot {
    pub fn adjust(&mut self, delta: i64) -> Result<(), String> {
        let next = self
            .quantity
            .checked_add(delta)
            .ok_or_else(|| "quantity overflow".to_string())?;
        if next < 0 {
            return Err("quantity cannot be negative".into());
        }
        self.quantity = next;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StockMove {
    pub from: StockLotId,
    pub to: StockLocation,
    pub quantity: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SearchResults {
    pub items: Vec<ItemMatch>,
    pub locations: Vec<StockLocationResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemMatch {
    pub item: Item,
    pub total_quantity: i64,
    pub lots: Vec<StockLot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StockLocationResult {
    pub item_id: ItemId,
    pub item_name: String,
    pub quantity: i64,
    pub location: String,
}

/// The unit used by quantity operations; useful for validating API input.
pub fn validate_quantity(quantity: i64) -> Result<(), String> {
    if quantity < 0 {
        Err("quantity cannot be negative".into())
    } else {
        Ok(())
    }
}

#[allow(dead_code)]
fn _mm_marker(_: Mm) {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quantity_never_goes_negative() {
        let mut lot = StockLot {
            id: StockLotId::new(),
            item_id: ItemId::new(),
            location: StockLocation {
                node_id: NodeId::new(),
                compartment: None,
            },
            quantity: 2,
            placement: None,
        };
        assert!(lot.adjust(-3).is_err());
        assert_eq!(lot.quantity, 2);
    }
}
