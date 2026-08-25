use serde::Serialize;

use crate::adapter::{
    ContentsResult, FindResult, FreeResult, MutationOperation, MutationResult, TreeNode, TreeResult,
};

pub fn find(result: FindResult, json: bool) -> String {
    if json {
        return json_output(&result);
    }
    if result.items.is_empty() {
        return "No matching items.\n".into();
    }

    let mut out = String::new();
    for item in result.items {
        out.push_str(&format!(
            "{}  {} pcs total\n",
            item.name, item.total_quantity
        ));
        if item.locations.is_empty() {
            out.push_str("  (no stock)\n");
        } else {
            for stock in item.locations {
                out.push_str(&format!(
                    "  {:>6} pcs  {}\n",
                    stock.quantity, stock.location
                ));
            }
        }
    }
    out
}

pub fn contents(result: ContentsResult, json: bool) -> String {
    if json {
        return json_output(&result);
    }
    let mut out = format!("{}\n", result.location);
    if result.stock.is_empty() {
        out.push_str("  (empty)\n");
    } else {
        for entry in result.stock {
            out.push_str(&format!(
                "  {:>6} pcs  {}  @ {}\n",
                entry.quantity, entry.item, entry.location
            ));
        }
    }
    out
}

pub fn tree(result: TreeResult, json: bool) -> String {
    if json {
        return json_output(&result);
    }
    let mut out = String::new();
    for (index, node) in result.roots.iter().enumerate() {
        write_tree(&mut out, node, "", index + 1 == result.roots.len());
    }
    if out.is_empty() {
        out.push_str("(no storage)\n");
    }
    out
}

fn write_tree(out: &mut String, node: &TreeNode, prefix: &str, last: bool) {
    out.push_str(prefix);
    out.push_str(if last { "└── " } else { "├── " });
    out.push_str(&node.name);
    if !node.kind.is_empty() {
        out.push_str(&format!(" [{}]", node.kind));
    }
    out.push('\n');
    let child_prefix = format!("{}{}", prefix, if last { "    " } else { "│   " });
    for (index, child) in node.children.iter().enumerate() {
        write_tree(out, child, &child_prefix, index + 1 == node.children.len());
    }
}

pub fn free(result: FreeResult, json: bool) -> String {
    if json {
        return json_output(&result);
    }
    if result.locations.is_empty() {
        return "No matching storage locations.\n".into();
    }

    let mut out = String::new();
    for location in result.locations {
        out.push_str(&location.location);
        out.push_str("  ");
        out.push_str(&location.state.to_ascii_uppercase());
        if let Some(fill) = location.fill_percent {
            out.push_str(&format!("  ~{fill}%"));
        }
        if let Some(reserved) = location.reserved_for {
            out.push_str(&format!("  reserved for {reserved}"));
        }
        out.push('\n');
    }
    out
}

pub fn mutation(result: MutationResult, json: bool) -> String {
    if json {
        return json_output(&result);
    }
    match result.operation {
        MutationOperation::Add => format!(
            "Added {} pcs of {} at {}.\n",
            result.quantity.unwrap_or_default(),
            result.item,
            result.to
        ),
        MutationOperation::Adjust => format!(
            "Adjusted {} by {:+} pcs at {}.\n",
            result.item,
            result.quantity.unwrap_or_default(),
            result.to
        ),
        MutationOperation::Move => {
            let amount = result
                .quantity
                .map_or_else(|| "all stock".into(), |quantity| format!("{quantity} pcs"));
            format!(
                "Moved {amount} of {} from {} to {}.\n",
                result.item,
                result.from.as_deref().unwrap_or("unknown location"),
                result.to
            )
        }
    }
}

pub fn json_output<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| "{\"error\":\"cannot serialize result\"}".into())
        + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{ContentsEntry, FreeLocation, ItemResult, StockResult};

    #[test]
    fn formats_find_with_total_and_every_location() {
        let output = find(
            FindResult {
                items: vec![ItemResult {
                    id: "item-1".into(),
                    name: "MCP6022-I/P".into(),
                    total_quantity: 18,
                    locations: vec![
                        StockResult {
                            quantity: 12,
                            location: "Room A / Rack B / Shelf 3 / ICs / D4".into(),
                        },
                        StockResult {
                            quantity: 6,
                            location: "Room B / Bench box".into(),
                        },
                    ],
                }],
            },
            false,
        );
        assert_eq!(
            output,
            "MCP6022-I/P  18 pcs total\n      12 pcs  Room A / Rack B / Shelf 3 / ICs / D4\n       6 pcs  Room B / Bench box\n"
        );
    }

    #[test]
    fn formats_unstocked_match_and_empty_search_distinctly() {
        assert_eq!(
            find(
                FindResult {
                    items: vec![ItemResult {
                        id: "item-1".into(),
                        name: "MCP6022-I/P".into(),
                        total_quantity: 0,
                        locations: vec![],
                    }]
                },
                false
            ),
            "MCP6022-I/P  0 pcs total\n  (no stock)\n"
        );
        assert_eq!(find(FindResult::default(), false), "No matching items.\n");
    }

    #[test]
    fn contents_includes_each_exact_location() {
        assert_eq!(
            contents(
                ContentsResult {
                    location: "Room A / ICs".into(),
                    stock: vec![ContentsEntry {
                        item: "MCP6022-I/P".into(),
                        quantity: 12,
                        location: "Room A / ICs / D4".into(),
                    }],
                },
                false
            ),
            "Room A / ICs\n      12 pcs  MCP6022-I/P  @ Room A / ICs / D4\n"
        );
        assert_eq!(
            contents(
                ContentsResult {
                    location: "Box".into(),
                    stock: vec![],
                },
                false
            ),
            "Box\n  (empty)\n"
        );
    }

    #[test]
    fn json_find_keeps_item_and_location_shape() {
        let output = find(
            FindResult {
                items: vec![ItemResult {
                    id: "item-1".into(),
                    name: "part".into(),
                    total_quantity: 3,
                    locations: vec![StockResult {
                        quantity: 3,
                        location: "Room / Box".into(),
                    }],
                }],
            },
            true,
        );
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["items"][0]["total_quantity"], 3);
        assert_eq!(value["items"][0]["locations"][0]["location"], "Room / Box");
    }

    #[test]
    fn mutation_json_has_the_same_fields_for_every_operation() {
        let add: serde_json::Value = serde_json::from_str(&mutation(
            MutationResult {
                operation: MutationOperation::Add,
                item: "part".into(),
                quantity: Some(4),
                from: None,
                to: "Room A / Box".into(),
            },
            true,
        ))
        .unwrap();
        let moved: serde_json::Value = serde_json::from_str(&mutation(
            MutationResult {
                operation: MutationOperation::Move,
                item: "part".into(),
                quantity: None,
                from: Some("Room A / Box".into()),
                to: "Room B / Shelf".into(),
            },
            true,
        ))
        .unwrap();

        assert_eq!(add["operation"], "add");
        assert!(add["from"].is_null());
        assert_eq!(moved["operation"], "move");
        assert!(moved["quantity"].is_null());
        assert_eq!(
            add.as_object().unwrap().keys().collect::<Vec<_>>(),
            moved.as_object().unwrap().keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn formats_human_mutations_with_resolved_locations() {
        assert_eq!(
            mutation(
                MutationResult {
                    operation: MutationOperation::Adjust,
                    item: "part".into(),
                    quantity: Some(-2),
                    from: None,
                    to: "Room / D4".into(),
                },
                false
            ),
            "Adjusted part by -2 pcs at Room / D4.\n"
        );
        assert_eq!(
            mutation(
                MutationResult {
                    operation: MutationOperation::Move,
                    item: "part".into(),
                    quantity: None,
                    from: Some("Room A / D4".into()),
                    to: "Room B / Box".into(),
                },
                false
            ),
            "Moved all stock of part from Room A / D4 to Room B / Box.\n"
        );
    }

    #[test]
    fn renders_free_json_and_tree_connectors() {
        let json = free(
            FreeResult {
                locations: vec![FreeLocation {
                    location: "Rack A / Shelf 1 / D4".into(),
                    state: "reserved".into(),
                    fill_percent: Some(0),
                    reserved_for: Some("FluxPose".into()),
                }],
            },
            true,
        );
        assert!(json.contains("\"reserved_for\": \"FluxPose\""));

        let result = TreeResult {
            roots: vec![TreeNode {
                name: "Rack A".into(),
                kind: "rack".into(),
                id: "rack-a".into(),
                children: vec![TreeNode {
                    name: "Shelf 1".into(),
                    kind: "shelf".into(),
                    id: "shelf-1".into(),
                    children: vec![],
                }],
            }],
        };
        assert_eq!(
            tree(result, false),
            "└── Rack A [rack]\n    └── Shelf 1 [shelf]\n"
        );
    }
}
