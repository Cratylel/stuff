//! The small, database-independent vocabulary used by Stuff.

use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            Serialize,
            Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);
        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }
        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }
        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
        impl FromStr for $name {
            type Err = uuid::Error;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::parse_str(value)?))
            }
        }
    };
}

id_type!(NodeId);
id_type!(TemplateId);
id_type!(RackTemplateId);
id_type!(DrawerCabinetTemplateId);
id_type!(OrganizerId);
id_type!(ItemId);
id_type!(StockLotId);
id_type!(ReservationId);

/// Fixed point millimetres. Keeping these as integers avoids floating point drift
/// while still allowing sub-centimetre placement.
pub type Mm = i64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Point3 {
    pub x: Mm,
    pub y: Mm,
    pub z: Mm,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Size3 {
    pub width: Mm,
    pub depth: Mm,
    pub height: Mm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub origin: Point3,
    pub size: Size3,
}

impl Aabb {
    pub fn max_x(self) -> Mm {
        self.origin.x.saturating_add(self.size.width)
    }
    pub fn max_y(self) -> Mm {
        self.origin.y.saturating_add(self.size.depth)
    }
    pub fn max_z(self) -> Mm {
        self.origin.z.saturating_add(self.size.height)
    }
    pub fn intersects(self, other: Self) -> bool {
        self.origin.x < other.max_x()
            && self.max_x() > other.origin.x
            && self.origin.y < other.max_y()
            && self.max_y() > other.origin.y
            && self.origin.z < other.max_z()
            && self.max_z() > other.origin.z
    }
    pub fn contains(self, other: Self) -> bool {
        other.origin.x >= self.origin.x
            && other.max_x() <= self.max_x()
            && other.origin.y >= self.origin.y
            && other.max_y() <= self.max_y()
            && other.origin.z >= self.origin.z
            && other.max_z() <= self.max_z()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PlacementWarning {
    OutsideParent,
    Overlap(NodeId),
    ExceedsHeight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapProposal {
    pub position: Point3,
    pub distance: Mm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SnapKind {
    GridX,
    GridY,
    GridZ,
    ParentLeadingX,
    ParentTrailingX,
    ParentLeadingY,
    ParentTrailingY,
    ParentLeadingZ,
    ParentTrailingZ,
    SiblingLeadingX,
    SiblingTrailingX,
    SiblingLeadingY,
    SiblingTrailingY,
    SiblingTopZ,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NeighborSnapProposal {
    pub position: Point3,
    pub distance: Mm,
    /// Whether the x, y, or z coordinate was changed by a neighbor/edge snap.
    pub snapped_axes: [bool; 3],
    pub snaps: Vec<SnapKind>,
}

/// Returns useful warnings rather than rejecting approximate real-world layouts.
pub fn placement_warnings(
    node: &StorageNode,
    parent: &StorageNode,
    siblings: &[StorageNode],
) -> Vec<PlacementWarning> {
    let (Some(place), Some(size), Some(parent_size)) = (node.placement, node.size, parent.size)
    else {
        return Vec::new();
    };
    let object = Aabb {
        origin: place.position,
        size,
    };
    let bounds = Aabb {
        origin: Point3::default(),
        size: parent_size,
    };
    let mut warnings = Vec::new();
    if !bounds.contains(object) {
        warnings.push(PlacementWarning::OutsideParent);
    }
    if object.max_z() > parent_size.height {
        warnings.push(PlacementWarning::ExceedsHeight);
    }
    for sibling in siblings {
        if sibling.id == node.id {
            continue;
        }
        if let (Some(s_place), Some(s_size)) = (sibling.placement, sibling.size)
            && object.intersects(Aabb {
                origin: s_place.position,
                size: s_size,
            })
        {
            warnings.push(PlacementWarning::Overlap(sibling.id));
        }
    }
    warnings
}

pub fn snap_position(position: Point3, size: Size3, parent: Size3, grid: Mm) -> SnapProposal {
    let snap = |v: Mm| {
        if grid <= 0 {
            v
        } else {
            ((v + grid / 2) / grid) * grid
        }
    };
    let proposed = Point3 {
        x: snap(position.x).clamp(0, (parent.width - size.width).max(0)),
        y: snap(position.y).clamp(0, (parent.depth - size.depth).max(0)),
        z: snap(position.z).clamp(0, (parent.height - size.height).max(0)),
    };
    let distance = (proposed.x - position.x).abs()
        + (proposed.y - position.y).abs()
        + (proposed.z - position.z).abs();
    SnapProposal {
        position: proposed,
        distance,
    }
}

/// Snap a proposed placement to the parent shelf and nearby sibling edges.
///
/// Grid snapping is always applied first. Parent and sibling snaps only apply
/// when they are within `tolerance`; arbitrary placement therefore remains
/// possible. A sibling's top is considered for z snapping only when the
/// proposed horizontal footprint overlaps it. This reports visual snap hints
/// and never creates a containment or stacking relationship.
pub fn snap_position_with_neighbors(
    proposed: Point3,
    moving_size: Size3,
    parent_size: Size3,
    siblings: &[Aabb],
    grid: Mm,
    tolerance: Mm,
) -> NeighborSnapProposal {
    let base = snap_position(proposed, moving_size, parent_size, grid).position;
    let tolerance = tolerance.max(0);
    let mut position = base;
    let mut snaps = Vec::new();
    let mut snapped_axes = [false; 3];
    if grid > 0 {
        if position.x != proposed.x {
            snaps.push(SnapKind::GridX);
        }
        if position.y != proposed.y {
            snaps.push(SnapKind::GridY);
        }
        if position.z != proposed.z {
            snaps.push(SnapKind::GridZ);
        }
    }

    fn nearest(
        current: Mm,
        candidates: &[(Mm, SnapKind)],
        tolerance: Mm,
    ) -> Option<(Mm, SnapKind)> {
        candidates
            .iter()
            .filter(|(value, _)| (value - current).abs() <= tolerance)
            .min_by_key(|(value, _)| (value - current).abs())
            .copied()
    }

    let x_max = (parent_size.width - moving_size.width).max(0);
    let y_max = (parent_size.depth - moving_size.depth).max(0);
    let z_max = (parent_size.height - moving_size.height).max(0);
    let x_edges = [
        (0, SnapKind::ParentLeadingX),
        (x_max, SnapKind::ParentTrailingX),
    ];
    let y_edges = [
        (0, SnapKind::ParentLeadingY),
        (y_max, SnapKind::ParentTrailingY),
    ];
    let z_edges = [
        (0, SnapKind::ParentLeadingZ),
        (z_max, SnapKind::ParentTrailingZ),
    ];
    if let Some((value, kind)) = nearest(position.x, &x_edges, tolerance) {
        position.x = value;
        snapped_axes[0] = true;
        snaps.push(kind);
    }
    if let Some((value, kind)) = nearest(position.y, &y_edges, tolerance) {
        position.y = value;
        snapped_axes[1] = true;
        snaps.push(kind);
    }
    if let Some((value, kind)) = nearest(position.z, &z_edges, tolerance) {
        position.z = value;
        snapped_axes[2] = true;
        snaps.push(kind);
    }

    let mut sibling_x = Vec::with_capacity(siblings.len() * 2);
    let mut sibling_y = Vec::with_capacity(siblings.len() * 2);
    for sibling in siblings {
        sibling_x.push((sibling.origin.x, SnapKind::SiblingLeadingX));
        sibling_x.push((
            sibling.max_x().saturating_sub(moving_size.width),
            SnapKind::SiblingTrailingX,
        ));
        sibling_y.push((sibling.origin.y, SnapKind::SiblingLeadingY));
        sibling_y.push((
            sibling.max_y().saturating_sub(moving_size.depth),
            SnapKind::SiblingTrailingY,
        ));
    }
    if let Some((value, kind)) = nearest(position.x, &sibling_x, tolerance) {
        position.x = value;
        snapped_axes[0] = true;
        snaps.push(kind);
    }
    if let Some((value, kind)) = nearest(position.y, &sibling_y, tolerance) {
        position.y = value;
        snapped_axes[1] = true;
        snaps.push(kind);
    }
    // Re-evaluate the footprint after x/y alignment before choosing a stack.
    let aligned_footprint = Aabb {
        origin: position,
        size: moving_size,
    };
    let mut stack_candidates = Vec::new();
    for sibling in siblings {
        if aligned_footprint.origin.x < sibling.max_x()
            && aligned_footprint.max_x() > sibling.origin.x
            && aligned_footprint.origin.y < sibling.max_y()
            && aligned_footprint.max_y() > sibling.origin.y
        {
            stack_candidates.push((sibling.max_z(), SnapKind::SiblingTopZ));
        }
    }
    if let Some((value, kind)) = nearest(position.z, &stack_candidates, tolerance) {
        position.z = value.clamp(0, z_max);
        snapped_axes[2] = true;
        snaps.push(kind);
    }
    NeighborSnapProposal {
        position,
        distance: (position.x - proposed.x).abs()
            + (position.y - proposed.y).abs()
            + (position.z - proposed.z).abs(),
        snapped_axes,
        snaps,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub position: Point3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: Mm,
    pub y: Mm,
    pub width: Mm,
    pub depth: Mm,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StorageKind {
    Room,
    Rack,
    Shelf,
    /// A usable-volume workspace. Its top-down rendering is a UI concern.
    Drawer,
    /// A named solid room object such as a desk, bed, or workbench. Fixtures
    /// are not inventory locations and cannot contain other storage nodes.
    Fixture,
    /// An organizer with one undivided inventory area. Unlike `Organizer`,
    /// this is an ordinary storage node and has no reusable template instance.
    CellLessOrganizer,
    /// A reusable, typed organizer whose inventory locations come from its
    /// template compartments.
    Organizer,
    Box,
    Bin,
    Loose,
    Custom(String),
}

impl Default for StorageKind {
    fn default() -> Self {
        Self::Custom("storage".into())
    }
}
impl StorageKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Room => "room",
            Self::Rack => "rack",
            Self::Shelf => "shelf",
            Self::Drawer => "drawer",
            Self::Fixture => "fixture",
            Self::CellLessOrganizer => "cell_less_organizer",
            Self::Organizer => "organizer",
            Self::Box => "box",
            Self::Bin => "bin",
            Self::Loose => "loose",
            Self::Custom(v) => v,
        }
    }

    /// Whether this kind can be selected as an inventory destination. A typed
    /// organizer is inventory-capable through its explicit compartments.
    pub fn accepts_inventory(&self) -> bool {
        !matches!(self, Self::Fixture)
    }

    /// Direct stock in these workspaces has physical dimensions and a
    /// position. Stock inside containers remains a non-spatial list.
    pub fn has_spatial_stock(&self) -> bool {
        matches!(self, Self::Room | Self::Rack | Self::Shelf)
    }
}

/// What the dimensions stored on a node mean. The snapshot keeps its existing
/// `size` field; this enum lets consumers label it correctly without a data
/// migration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DimensionSemantics {
    RoomBounds,
    Outer,
    Usable,
}

/// Rules for the logical storage tree. `None` means a root node.
pub fn is_allowed_child_kind(parent: Option<&StorageKind>, child: &StorageKind) -> bool {
    use StorageKind::*;
    match parent {
        None => matches!(child, Room),
        Some(Room) => !matches!(child, Room),
        Some(Rack) => matches!(
            child,
            Shelf | Drawer | CellLessOrganizer | Organizer | Box | Bin | Loose | Custom(_)
        ),
        Some(Shelf) => matches!(
            child,
            Drawer | CellLessOrganizer | Organizer | Box | Bin | Loose | Custom(_)
        ),
        Some(Fixture) => false,
        Some(Drawer | Box | Bin | Loose | Custom(_)) => {
            matches!(
                child,
                Drawer | CellLessOrganizer | Organizer | Box | Bin | Loose | Custom(_)
            )
        }
        // Organizers are terminal inventory containers, whether their contents
        // use one undivided area or typed compartments.
        Some(CellLessOrganizer | Organizer) => false,
    }
}

/// Kinds a caller may create below `parent`. This is intentionally available
/// to presentation code so it does not need to mirror core containment rules.
pub fn allowed_child_kinds(parent: Option<&StorageKind>) -> Vec<StorageKind> {
    use StorageKind::*;
    let candidates = [
        Room,
        Rack,
        Shelf,
        Drawer,
        Fixture,
        CellLessOrganizer,
        Organizer,
        Box,
        Bin,
        Loose,
    ];
    candidates
        .into_iter()
        .filter(|kind| is_allowed_child_kind(parent, kind))
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorageNode {
    pub id: NodeId,
    pub name: String,
    pub kind: StorageKind,
    /// Logical containment. Physical placement is optional and independent.
    pub parent_id: Option<NodeId>,
    pub placement: Option<Placement>,
    pub size: Option<Size3>,
    pub metadata: serde_json::Value,
}

impl StorageNode {
    pub fn dimension_semantics(&self) -> DimensionSemantics {
        match &self.kind {
            StorageKind::Room => DimensionSemantics::RoomBounds,
            StorageKind::Shelf | StorageKind::Drawer => DimensionSemantics::Usable,
            _ => DimensionSemantics::Outer,
        }
    }

    pub fn room_bounds(&self) -> Option<Size3> {
        (self.dimension_semantics() == DimensionSemantics::RoomBounds)
            .then_some(self.size)
            .flatten()
    }

    pub fn outer_dimensions(&self) -> Option<Size3> {
        (self.dimension_semantics() == DimensionSemantics::Outer)
            .then_some(self.size)
            .flatten()
    }

    pub fn usable_dimensions(&self) -> Option<Size3> {
        (self.dimension_semantics() == DimensionSemantics::Usable)
            .then_some(self.size)
            .flatten()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub revision: u64,
    pub nodes: Vec<StorageNode>,
    pub organizer_templates: Vec<crate::organizer::OrganizerTemplate>,
    #[serde(default)]
    pub rack_templates: Vec<crate::rack::RackTemplate>,
    #[serde(default)]
    pub rack_instances: Vec<crate::rack::RackInstance>,
    #[serde(default)]
    pub drawer_cabinet_templates: Vec<crate::drawer::DrawerCabinetTemplate>,
    #[serde(default)]
    pub drawer_cabinet_instances: Vec<crate::drawer::DrawerCabinetInstance>,
    pub organizers: Vec<crate::organizer::OrganizerInstance>,
    pub items: Vec<crate::inventory::Item>,
    pub stock: Vec<crate::inventory::StockLot>,
    pub reservations: Vec<crate::occupancy::Reservation>,
    pub occupancies: Vec<crate::occupancy::Occupancy>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boxes_touching_do_not_intersect() {
        let a = Aabb {
            origin: Point3 { x: 0, y: 0, z: 0 },
            size: Size3 {
                width: 10,
                depth: 10,
                height: 10,
            },
        };
        let b = Aabb {
            origin: Point3 { x: 10, y: 0, z: 0 },
            size: Size3 {
                width: 2,
                depth: 2,
                height: 2,
            },
        };
        assert!(!a.intersects(b));
    }

    #[test]
    fn neighbor_snap_aligns_parent_and_sibling_edges() {
        let sibling = Aabb {
            origin: Point3 {
                x: 100,
                y: 60,
                z: 20,
            },
            size: Size3 {
                width: 30,
                depth: 30,
                height: 10,
            },
        };
        let proposal = snap_position_with_neighbors(
            Point3 {
                x: 103,
                y: 69,
                z: 40,
            },
            Size3 {
                width: 20,
                depth: 20,
                height: 5,
            },
            Size3 {
                width: 300,
                depth: 300,
                height: 100,
            },
            &[sibling],
            1,
            3,
        );
        assert_eq!(proposal.position.x, 100);
        assert_eq!(proposal.position.y, 70);
        assert!(proposal.snaps.contains(&SnapKind::SiblingLeadingX));
        assert!(proposal.snaps.contains(&SnapKind::SiblingTrailingY));

        let edge = snap_position_with_neighbors(
            Point3 { x: 3, y: 50, z: 40 },
            Size3 {
                width: 20,
                depth: 20,
                height: 5,
            },
            Size3 {
                width: 300,
                depth: 300,
                height: 100,
            },
            &[],
            1,
            3,
        );
        assert_eq!(edge.position.x, 0);
        assert!(edge.snaps.contains(&SnapKind::ParentLeadingX));
    }

    #[test]
    fn neighbor_snap_stacks_only_with_horizontal_overlap() {
        let sibling = Aabb {
            origin: Point3 {
                x: 100,
                y: 100,
                z: 20,
            },
            size: Size3 {
                width: 30,
                depth: 30,
                height: 10,
            },
        };
        let stacked = snap_position_with_neighbors(
            Point3 {
                x: 105,
                y: 105,
                z: 28,
            },
            Size3 {
                width: 10,
                depth: 10,
                height: 5,
            },
            Size3 {
                width: 300,
                depth: 300,
                height: 100,
            },
            &[sibling],
            1,
            3,
        );
        assert_eq!(stacked.position.z, 30);
        assert!(stacked.snaps.contains(&SnapKind::SiblingTopZ));

        let separate = snap_position_with_neighbors(
            Point3 {
                x: 10,
                y: 10,
                z: 28,
            },
            Size3 {
                width: 10,
                depth: 10,
                height: 5,
            },
            Size3 {
                width: 300,
                depth: 300,
                height: 100,
            },
            &[sibling],
            1,
            3,
        );
        assert_eq!(separate.position.z, 28);
        assert!(!separate.snaps.contains(&SnapKind::SiblingTopZ));
    }

    #[test]
    fn neighbor_snap_keeps_distant_edges_free() {
        let sibling = Aabb {
            origin: Point3 {
                x: 100,
                y: 100,
                z: 20,
            },
            size: Size3 {
                width: 40,
                depth: 30,
                height: 10,
            },
        };
        let proposal = snap_position_with_neighbors(
            Point3 {
                x: 123,
                y: 57,
                z: 28,
            },
            Size3 {
                width: 10,
                depth: 10,
                height: 5,
            },
            Size3 {
                width: 300,
                depth: 300,
                height: 100,
            },
            &[sibling],
            10,
            3,
        );
        assert_eq!(
            proposal.position,
            Point3 {
                x: 120,
                y: 60,
                z: 30
            }
        );
        assert!(!proposal.snaps.iter().any(|snap| matches!(
            snap,
            SnapKind::SiblingLeadingX
                | SnapKind::SiblingTrailingX
                | SnapKind::SiblingLeadingY
                | SnapKind::SiblingTrailingY
                | SnapKind::SiblingTopZ
        )));
    }

    #[test]
    fn cell_less_organizer_is_a_distinct_terminal_kind() {
        assert!(StorageKind::CellLessOrganizer.accepts_inventory());
        assert!(allowed_child_kinds(Some(&StorageKind::CellLessOrganizer)).is_empty());
        let shelf_children = allowed_child_kinds(Some(&StorageKind::Shelf));
        assert!(shelf_children.contains(&StorageKind::CellLessOrganizer));
        assert!(shelf_children.contains(&StorageKind::Organizer));
        assert!(
            !shelf_children
                .iter()
                .any(|kind| kind == &StorageKind::Custom("storage".into()))
        );
    }
}
