use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::domain::{Mm, NodeId, RackTemplateId, Size3};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RackShelfTemplate {
    pub name: String,
    pub altitude: Mm,
    pub usable_height: Mm,
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RackTemplate {
    pub id: RackTemplateId,
    pub name: String,
    pub outer_size: Size3,
    pub shelves: Vec<RackShelfTemplate>,
    pub metadata: serde_json::Value,
}

/// A physical rack that follows a reusable rack type. Shelf IDs stay stable
/// so type edits can update the real shelf nodes without losing their contents.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RackInstance {
    pub rack_id: NodeId,
    pub template_id: RackTemplateId,
    pub shelf_ids: Vec<NodeId>,
}

impl RackTemplate {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("rack type name cannot be blank".into());
        }
        if self.outer_size.width <= 10 || self.outer_size.depth <= 10 || self.outer_size.height <= 0
        {
            return Err("rack outer dimensions must be positive and leave 1 cm clearance".into());
        }
        let mut names = BTreeSet::new();
        for shelf in &self.shelves {
            if shelf.name.trim().is_empty() {
                return Err("rack shelf name cannot be blank".into());
            }
            if !names.insert(shelf.name.trim().to_ascii_lowercase()) {
                return Err("rack shelf names must be unique within a rack type".into());
            }
            if shelf.altitude < 0 || shelf.usable_height <= 0 {
                return Err(
                    "rack shelf altitude must be non-negative and usable height positive".into(),
                );
            }
            if shelf.altitude > self.outer_size.height {
                return Err("rack shelf base height exceeds the rack height".into());
            }
        }
        let mut ordered = self.shelves.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|shelf| shelf.altitude);
        for pair in ordered.windows(2) {
            if pair[0].altitude.saturating_add(pair[0].usable_height) > pair[1].altitude {
                return Err("rack shelf usable volumes cannot overlap vertically".into());
            }
        }
        // The highest level may use the volume above the rack itself (for
        // example up to a room ceiling). Its usable height therefore does not
        // need to end at the rack's outer height.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rack_type_rejects_overlapping_usable_volumes() {
        let template = RackTemplate {
            id: RackTemplateId::new(),
            name: "Rack".into(),
            outer_size: Size3 {
                width: 900,
                depth: 450,
                height: 1800,
            },
            shelves: vec![
                RackShelfTemplate {
                    name: "A".into(),
                    altitude: 100,
                    usable_height: 300,
                    metadata: serde_json::Value::Null,
                },
                RackShelfTemplate {
                    name: "B".into(),
                    altitude: 350,
                    usable_height: 300,
                    metadata: serde_json::Value::Null,
                },
            ],
            metadata: serde_json::Value::Null,
        };
        assert!(template.validate().is_err());
    }

    #[test]
    fn top_shelf_may_use_space_above_the_rack() {
        let template = RackTemplate {
            id: RackTemplateId::new(),
            name: "Rack".into(),
            outer_size: Size3 {
                width: 900,
                depth: 450,
                height: 1800,
            },
            shelves: vec![RackShelfTemplate {
                name: "Top".into(),
                altitude: 1700,
                usable_height: 700,
                metadata: serde_json::Value::Null,
            }],
            metadata: serde_json::Value::Null,
        };
        assert!(template.validate().is_ok());
    }
}
