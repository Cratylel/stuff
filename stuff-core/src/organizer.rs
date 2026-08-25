//! Organizer templates and their divider topology.

use crate::domain::{Mm, OrganizerId, Rect, Size3, TemplateId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct CellId {
    pub row: u32,
    pub column: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrimitiveCell {
    pub id: CellId,
    pub rect: crate::domain::Rect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DividerAxis {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DividerKind {
    Fixed,
    Removable,
    Impossible,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DividerEdge {
    pub a: CellId,
    pub b: CellId,
    pub axis: DividerAxis,
    pub kind: DividerKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrganizerTemplate {
    pub id: TemplateId,
    pub name: String,
    pub size: crate::domain::Size3,
    pub cells: Vec<PrimitiveCell>,
    pub dividers: Vec<DividerEdge>,
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrganizerInstance {
    pub id: OrganizerId,
    pub node_id: crate::domain::NodeId,
    pub template_id: TemplateId,
    /// Only removable edges occur here. Missing means installed.
    pub removed_dividers: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Compartment {
    pub key: String,
    pub cells: Vec<CellId>,
    pub rect: crate::domain::Rect,
}

fn edge_key(edge: &DividerEdge) -> String {
    let (a, b) = if edge.a <= edge.b {
        (edge.a, edge.b)
    } else {
        (edge.b, edge.a)
    };
    format!("{}:{}-{}:{}", a.row, a.column, b.row, b.column)
}

impl DividerEdge {
    pub fn key(&self) -> String {
        edge_key(self)
    }
}

impl OrganizerTemplate {
    pub fn compartments(&self, removed: &BTreeSet<String>) -> Vec<Compartment> {
        let cells: BTreeMap<_, _> = self.cells.iter().map(|c| (c.id, c)).collect();
        let mut adjacency: BTreeMap<CellId, Vec<CellId>> =
            cells.keys().map(|id| (*id, Vec::new())).collect();
        for edge in &self.dividers {
            let connected = match edge.kind {
                // There is no physical wall at an impossible boundary.
                DividerKind::Impossible => true,
                DividerKind::Fixed => false,
                DividerKind::Removable => removed.contains(&edge.key()),
            };
            if connected {
                adjacency.entry(edge.a).or_default().push(edge.b);
                adjacency.entry(edge.b).or_default().push(edge.a);
            }
        }
        let mut seen = BTreeSet::new();
        let mut result = Vec::new();
        for &start in cells.keys() {
            if !seen.insert(start) {
                continue;
            }
            let mut queue = VecDeque::from([start]);
            let mut group = Vec::new();
            while let Some(id) = queue.pop_front() {
                group.push(id);
                for next in adjacency.get(&id).into_iter().flatten() {
                    if seen.insert(*next) {
                        queue.push_back(*next);
                    }
                }
            }
            group.sort();
            let min_x = group.iter().map(|id| cells[id].rect.x).min().unwrap_or(0);
            let min_y = group.iter().map(|id| cells[id].rect.y).min().unwrap_or(0);
            let max_x = group
                .iter()
                .map(|id| cells[id].rect.x + cells[id].rect.width)
                .max()
                .unwrap_or(min_x);
            let max_y = group
                .iter()
                .map(|id| cells[id].rect.y + cells[id].rect.depth)
                .max()
                .unwrap_or(min_y);
            let key = group
                .iter()
                .map(|id| format!("{}{}", id.row, id.column))
                .collect::<Vec<_>>()
                .join("+");
            result.push(Compartment {
                key,
                cells: group,
                rect: crate::domain::Rect {
                    x: min_x,
                    y: min_y,
                    width: max_x - min_x,
                    depth: max_y - min_y,
                },
            });
        }
        result
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("organizer template name cannot be blank".into());
        }
        if self.size.width <= 0 || self.size.depth <= 0 || self.size.height <= 0 {
            return Err("template dimensions must be positive".into());
        }
        let ids: BTreeSet<_> = self.cells.iter().map(|c| c.id).collect();
        if ids.len() != self.cells.len() {
            return Err("organizer cell ids must be unique".into());
        }
        for cell in &self.cells {
            let rect = cell.rect;
            if rect.width <= 0 || rect.depth <= 0 {
                return Err("organizer cell dimensions must be positive".into());
            }
            if rect.x < 0
                || rect.y < 0
                || rect.x.saturating_add(rect.width) > self.size.width
                || rect.y.saturating_add(rect.depth) > self.size.depth
            {
                return Err("organizer cell must fit inside organizer bounds".into());
            }
        }
        for (index, cell) in self.cells.iter().enumerate() {
            for other in self.cells.iter().skip(index + 1) {
                let intersects = cell.rect.x < other.rect.x.saturating_add(other.rect.width)
                    && cell.rect.x.saturating_add(cell.rect.width) > other.rect.x
                    && cell.rect.y < other.rect.y.saturating_add(other.rect.depth)
                    && cell.rect.y.saturating_add(cell.rect.depth) > other.rect.y;
                if intersects {
                    return Err("organizer cells must not overlap".into());
                }
            }
        }
        let mut divider_keys = BTreeSet::new();
        for edge in &self.dividers {
            if edge.a == edge.b || !ids.contains(&edge.a) || !ids.contains(&edge.b) {
                return Err("divider endpoints must be two existing cells".into());
            }
            if !divider_keys.insert(edge.key()) {
                return Err("divider endpoints must be unique".into());
            }
        }
        Ok(())
    }
}

/// Convenience constructor for equal rectangular cells. The resulting
/// template still stores primitive cells and divider edges, so callers can
/// later use irregular topology by editing those records directly.
pub fn grid_template(
    name: impl Into<String>,
    size: Size3,
    rows: u32,
    columns: u32,
    divider_kind: DividerKind,
) -> Result<OrganizerTemplate, String> {
    let name = name.into();
    if name.trim().is_empty() {
        return Err("organizer template name cannot be blank".into());
    }
    if rows == 0 || columns == 0 {
        return Err("grid needs at least one row and column".into());
    }
    if size.width <= 0 || size.depth <= 0 || size.height <= 0 {
        return Err("template dimensions must be positive".into());
    }
    let cell_count = u64::from(rows)
        .checked_mul(u64::from(columns))
        .ok_or_else(|| "grid cell count overflow".to_string())?;
    if cell_count > usize::MAX as u64 {
        return Err("grid is too large".into());
    }
    let columns_mm = Mm::from(columns);
    let rows_mm = Mm::from(rows);
    if size.width < columns_mm || size.depth < rows_mm {
        return Err("external dimensions must leave every cell positive".into());
    }
    let cell_width = size.width / columns_mm;
    let cell_depth = size.depth / rows_mm;
    let width_remainder = size.width % columns_mm;
    let depth_remainder = size.depth % rows_mm;
    let mut cells = Vec::with_capacity(cell_count as usize);
    for row in 0..rows {
        for column in 0..columns {
            let x = Mm::from(column)
                .checked_mul(cell_width)
                .ok_or_else(|| "grid x coordinate overflow".to_string())?;
            let y = Mm::from(row)
                .checked_mul(cell_depth)
                .ok_or_else(|| "grid y coordinate overflow".to_string())?;
            let width = cell_width
                + if column == columns - 1 {
                    width_remainder
                } else {
                    0
                };
            let depth = cell_depth + if row == rows - 1 { depth_remainder } else { 0 };
            cells.push(PrimitiveCell {
                id: CellId { row, column },
                rect: Rect { x, y, width, depth },
            });
        }
    }
    let mut dividers = Vec::new();
    for row in 0..rows {
        for column in 0..columns.saturating_sub(1) {
            dividers.push(DividerEdge {
                a: CellId { row, column },
                b: CellId {
                    row,
                    column: column + 1,
                },
                axis: DividerAxis::Vertical,
                kind: divider_kind,
            });
        }
    }
    for row in 0..rows.saturating_sub(1) {
        for column in 0..columns {
            dividers.push(DividerEdge {
                a: CellId { row, column },
                b: CellId {
                    row: row + 1,
                    column,
                },
                axis: DividerAxis::Horizontal,
                kind: divider_kind,
            });
        }
    }
    Ok(OrganizerTemplate {
        id: TemplateId::new(),
        name,
        size,
        cells,
        dividers,
        metadata: serde_json::Value::Null,
    })
}

impl OrganizerInstance {
    pub fn compartments(&self, template: &OrganizerTemplate) -> Vec<Compartment> {
        template.compartments(&self.removed_dividers)
    }
    pub fn validate(&self, template: &OrganizerTemplate) -> Result<(), String> {
        template.validate()?;
        let removable: BTreeSet<_> = template
            .dividers
            .iter()
            .filter(|d| d.kind == DividerKind::Removable)
            .map(DividerEdge::key)
            .collect();
        if self
            .removed_dividers
            .iter()
            .any(|key| !removable.contains(key))
        {
            return Err("instance contains an invalid removed divider".into());
        }
        Ok(())
    }
}

/// A split after contents have been merged is ambiguous: the caller must assign
/// stock to one of the resulting compartments instead of guessing.
pub fn split_ambiguities(
    old: &[Compartment],
    new: &[Compartment],
    occupied: &BTreeMap<String, u64>,
) -> Vec<String> {
    let mut out = Vec::new();
    for old_compartment in old {
        let count = occupied.get(&old_compartment.key).copied().unwrap_or(0);
        if count == 0 {
            continue;
        }
        let parts = new
            .iter()
            .filter(|n| {
                n.cells
                    .iter()
                    .any(|cell| old_compartment.cells.contains(cell))
            })
            .count();
        if parts > 1 {
            out.push(old_compartment.key.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Mm, Rect, Size3};
    fn template() -> OrganizerTemplate {
        let cells = (0..2)
            .flat_map(|r| {
                (0..2).map(move |c| PrimitiveCell {
                    id: CellId { row: r, column: c },
                    rect: Rect {
                        x: c as Mm * 10,
                        y: r as Mm * 10,
                        width: 10,
                        depth: 10,
                    },
                })
            })
            .collect::<Vec<_>>();
        let mut dividers = Vec::new();
        for r in 0..2 {
            dividers.push(DividerEdge {
                a: CellId { row: r, column: 0 },
                b: CellId { row: r, column: 1 },
                axis: DividerAxis::Vertical,
                kind: DividerKind::Removable,
            });
        }
        for c in 0..2 {
            dividers.push(DividerEdge {
                a: CellId { row: 0, column: c },
                b: CellId { row: 1, column: c },
                axis: DividerAxis::Horizontal,
                kind: DividerKind::Removable,
            });
        }
        OrganizerTemplate {
            id: TemplateId::new(),
            name: "test".into(),
            size: Size3 {
                width: 20,
                depth: 20,
                height: 5,
            },
            cells,
            dividers,
            metadata: serde_json::Value::Null,
        }
    }
    #[test]
    fn removing_wall_merges_cells() {
        let t = template();
        let edge = t.dividers[0].key();
        let mut removed = BTreeSet::new();
        removed.insert(edge);
        assert_eq!(t.compartments(&removed).len(), 3);
    }
    #[test]
    fn split_reports_populated_merged_compartment() {
        let t = template();
        let edge = t.dividers[0].key();
        let mut removed = BTreeSet::new();
        removed.insert(edge);
        let merged = t.compartments(&removed);
        let separate = t.compartments(&BTreeSet::new());
        let mut occupancy = BTreeMap::new();
        occupancy.insert(merged[0].key.clone(), 4);
        assert!(!split_ambiguities(&merged, &separate, &occupancy).is_empty());
        let mut occupancy = BTreeMap::new();
        occupancy.insert(separate[0].key.clone(), 4);
        assert!(split_ambiguities(&separate, &merged, &occupancy).is_empty());
    }

    #[test]
    fn template_rejects_duplicate_divider_endpoints_in_either_direction() {
        let mut template = template();
        let mut duplicate = template.dividers[0].clone();
        std::mem::swap(&mut duplicate.a, &mut duplicate.b);
        template.dividers.push(duplicate);
        assert!(template.validate().is_err());
    }

    #[test]
    fn grid_constructor_covers_non_divisible_dimensions() {
        let template = grid_template(
            "grid",
            Size3 {
                width: 34,
                depth: 22,
                height: 5,
            },
            2,
            3,
            DividerKind::Removable,
        )
        .unwrap();
        assert_eq!(template.cells.len(), 6);
        assert_eq!(template.cells[2].rect.x + template.cells[2].rect.width, 34);
        assert_eq!(template.cells[5].rect.y + template.cells[5].rect.depth, 22);
        assert!(
            grid_template(
                "bad",
                Size3 {
                    width: 2,
                    depth: 2,
                    height: 5
                },
                3,
                1,
                DividerKind::Fixed,
            )
            .is_err()
        );
    }
}
