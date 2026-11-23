//! A page's accessibility snapshot, from CDP's `Accessibility.getFullAXTree` (BROWSER-9): the
//! tree as agent-browser wrote it, one `- role "name" [ref=eN]` line per node worth naming,
//! indented by depth, and each ref's node (`backendDOMNodeId`) for the acts that name it. It owns
//! no state and writes nothing.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Value, json};

/// Roles that are only structure: their children are listed, they get no line of their own.
const STRUCTURE: &[&str] = &[
    "generic",
    "none",
    "presentation",
    "InlineTextBox",
    "LineBreak",
    "RootWebArea",
    "WebArea",
];

/// Roles a person (or an agent) acts on: these get a ref even without a name.
const ACTABLE: &[&str] = &[
    "button",
    "link",
    "textbox",
    "searchbox",
    "checkbox",
    "radio",
    "combobox",
    "listbox",
    "option",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "slider",
    "spinbutton",
    "switch",
    "tab",
    "treeitem",
];

/// A snapshot: the text the agent reads, each ref's role and name, and each ref's DOM node.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub text: String,
    pub refs: BTreeMap<String, Value>,
    pub nodes: HashMap<String, i64>,
}

fn value(v: &Value) -> &str {
    v["value"].as_str().unwrap_or_default()
}

/// The snapshot of `tree` (`getFullAXTree`'s result).
pub fn snapshot(tree: &Value) -> Snapshot {
    let nodes: Vec<&Value> = tree["nodes"].as_array().into_iter().flatten().collect();
    let by_id: HashMap<&str, &Value> = nodes
        .iter()
        .filter_map(|n| n["nodeId"].as_str().map(|id| (id, *n)))
        .collect();
    // The root: the node no other node names as its child.
    let children: std::collections::HashSet<&str> = nodes
        .iter()
        .flat_map(|n| n["childIds"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    let mut out = Snapshot::default();
    let mut next = 1;
    for root in nodes.iter().filter(|n| {
        n["nodeId"]
            .as_str()
            .is_some_and(|id| !children.contains(id))
    }) {
        walk(root, &by_id, 0, &mut out, &mut next);
    }
    out
}

fn walk(
    node: &Value,
    by_id: &HashMap<&str, &Value>,
    depth: usize,
    out: &mut Snapshot,
    next: &mut usize,
) {
    let role = value(&node["role"]);
    let name = value(&node["name"]).trim();
    let ignored = node["ignored"] == true;
    let structure = STRUCTURE.contains(&role) || role.is_empty();
    let text_only = role == "StaticText";
    let mut depth_below = depth;
    if !ignored && !structure {
        let indent = "  ".repeat(depth);
        if text_only {
            if !name.is_empty() {
                out.text.push_str(&format!("{indent}- text: {name}\n"));
            }
        } else {
            let backend = node["backendDOMNodeId"].as_i64();
            let named = !name.is_empty();
            let mut line = format!("{indent}- {role}");
            if named {
                line.push_str(&format!(" {}", json!(name)));
            }
            if let Some(b) = backend.filter(|_| named || ACTABLE.contains(&role)) {
                let r = format!("e{next}");
                *next += 1;
                line.push_str(&format!(" [ref={r}]"));
                out.refs
                    .insert(r.clone(), json!({"role": role, "name": name}));
                out.nodes.insert(r, b);
            }
            out.text.push_str(&line);
            out.text.push('\n');
            depth_below = depth + 1;
        }
    }
    for child in node["childIds"].as_array().into_iter().flatten() {
        if let Some(c) = child.as_str().and_then(|id| by_id.get(id)) {
            walk(c, by_id, depth_below, out, next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, role: &str, name: &str, backend: Option<i64>, kids: &[&str]) -> Value {
        let mut n = json!({"nodeId": id, "ignored": false, "role": {"value": role},
                           "name": {"value": name}, "childIds": kids});
        if let Some(b) = backend {
            n["backendDOMNodeId"] = json!(b);
        }
        n
    }

    #[test]
    fn browser_9_a_snapshot_names_what_can_be_acted_on_and_maps_each_ref_to_its_node() {
        let tree = json!({"nodes": [
            node("1", "RootWebArea", "Sign in", Some(1), &["2"]),
            node("2", "generic", "", Some(2), &["3", "4", "5", "6"]),
            node("3", "heading", "Sign in", Some(3), &["7"]),
            node("7", "StaticText", "Sign in", None, &[]),
            node("4", "textbox", "", Some(4), &[]),
            node("5", "button", "Sign in", Some(5), &[]),
            {"nodeId": "6", "ignored": true, "role": {"value": "button"}, "childIds": []},
        ]});
        let s = snapshot(&tree);
        assert_eq!(
            s.text,
            "- heading \"Sign in\" [ref=e1]\n  - text: Sign in\n- textbox [ref=e2]\n- button \"Sign in\" [ref=e3]\n"
        );
        assert_eq!(s.refs["e3"], json!({"role": "button", "name": "Sign in"}));
        assert_eq!(s.nodes["e2"], 4);
        assert_eq!(s.nodes.len(), 3);
    }
}
