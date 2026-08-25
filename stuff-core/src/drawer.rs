//! Reusable chest-of-drawers types and their physical instances.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::domain::{DrawerCabinetTemplateId, Mm, NodeId, Size3};

/// The custom storage-kind value used by the physical cabinet node. Keeping
/// the cabinet identifiable through its instance association also leaves room
/// for adding a first-class kind without changing today's public enum.
pub const DRAWER_CABINET_KIND: &str = "drawer_cabinet";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DrawerTemplateDrawer {
    pub name: String,
    pub altitude: Mm,
    pub usable_size: Size3,
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DrawerCabinetTemplate {
    pub id: DrawerCabinetTemplateId,
    pub name: String,
    pub outer_size: Size3,
    pub drawers: Vec<DrawerTemplateDrawer>,
    pub metadata: serde_json::Value,
}

/// A cabinet node plus the stable generated Drawer child for every type slot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DrawerCabinetInstance {
    pub cabinet_id: NodeId,
    pub template_id: DrawerCabinetTemplateId,
    pub drawer_ids: Vec<NodeId>,
}

impl DrawerCabinetTemplate {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("drawer cabinet type name cannot be blank".into());
        }
        if self.outer_size.width <= 0 || self.outer_size.depth <= 0 || self.outer_size.height <= 0 {
            return Err("drawer cabinet dimensions must be positive".into());
        }
        let mut names = BTreeSet::new();
        for drawer in &self.drawers {
            if drawer.name.trim().is_empty() {
                return Err("drawer name cannot be blank".into());
            }
            if !names.insert(drawer.name.trim().to_ascii_lowercase()) {
                return Err("drawer names must be unique within a cabinet type".into());
            }
            if drawer.altitude < 0
                || drawer.usable_size.width <= 0
                || drawer.usable_size.depth <= 0
                || drawer.usable_size.height <= 0
            {
                return Err("drawer altitude and usable dimensions are invalid".into());
            }
            if drawer.usable_size.width > self.outer_size.width
                || drawer.usable_size.depth > self.outer_size.depth
                || drawer.altitude.saturating_add(drawer.usable_size.height)
                    > self.outer_size.height
            {
                return Err("drawer usable volume must fit inside the cabinet".into());
            }
        }
        let mut ordered = self.drawers.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|drawer| drawer.altitude);
        for pair in ordered.windows(2) {
            if pair[0].altitude.saturating_add(pair[0].usable_size.height) > pair[1].altitude {
                return Err("drawer usable volumes cannot overlap vertically".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_generated_drawer_geometry() {
        let template = DrawerCabinetTemplate {
            id: DrawerCabinetTemplateId::new(),
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
        };
        assert!(template.validate().is_ok());
    }
}
