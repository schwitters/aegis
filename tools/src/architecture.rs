//! Architecture gate: the layering of a Cargo workspace, checked on its dependency graph.
//!
//! Splitting a system into crates makes architecture rules compile-time facts — code cannot use a crate it does not
//! declare. What is left to guard is the declaration itself: this check reads `cargo metadata` and rejects every
//! direct dependency between workspace crates that the profile's layer policy does not allow, and every direct use
//! of a restricted third-party crate (e.g. sqlx, axum) outside the layers that may use it.
//!
//! Layers are matched by crate-name suffix: `shop-domain` and `domain` both belong to layer `domain`.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Layer of a workspace crate: the longest layer name that equals the crate name or is its `-<layer>` suffix.
pub fn layer_of<'a>(crate_name: &str, layers: &'a BTreeMap<String, Vec<String>>) -> Option<&'a str> {
    layers
        .keys()
        .filter(|l| crate_name == l.as_str() || crate_name.ends_with(&format!("-{}", l)))
        .max_by_key(|l| l.len())
        .map(|l| l.as_str())
}

fn allows(list: &[String], layer: &str) -> bool {
    list.iter().any(|l| l == "*" || l == layer)
}

/// Checks `cargo metadata --format-version 1 --no-deps` output against the policy.
/// `layers`: layer -> layers it may depend on (`*` = any). `restricted`: third-party crate -> layers that may depend
/// on it directly. Unrestricted third-party crates are allowed everywhere.
pub fn check(metadata: &Value, layers: &BTreeMap<String, Vec<String>>, restricted: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let mut problems = Vec::new();
    if layers.is_empty() {
        problems.push("INVALID  gate.architecture.layers is empty — no layer policy to enforce".to_string());
        return problems;
    }
    for (layer, deps) in layers {
        for d in deps {
            if d != "*" && !layers.contains_key(d) {
                problems.push(format!("INVALID  layer {} may depend on unknown layer {}", layer, d));
            }
        }
    }
    for (krate, allowed) in restricted {
        for l in allowed {
            if l != "*" && !layers.contains_key(l) {
                problems.push(format!("INVALID  restricted crate {} names unknown layer {}", krate, l));
            }
        }
    }

    let Some(packages) = metadata.get("packages").and_then(|p| p.as_array()) else {
        problems.push("INVALID  cargo metadata output has no `packages`".to_string());
        return problems;
    };
    let members: BTreeSet<&str> = packages.iter().filter_map(|p| p.get("name").and_then(|n| n.as_str())).collect();
    if members.is_empty() {
        problems.push("EMPTY    workspace has no member crates".to_string());
    }

    for pkg in packages {
        let name = pkg.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        let Some(layer) = layer_of(name, layers) else {
            problems.push(format!(
                "UNLAYERED {}: crate belongs to no layer (name it <project>-<layer>, layers: {})",
                name,
                layers.keys().cloned().collect::<Vec<_>>().join(", ")
            ));
            continue;
        };
        let allowed_layers = &layers[layer];
        let deps = pkg.get("dependencies").and_then(|d| d.as_array()).cloned().unwrap_or_default();
        for dep in deps {
            let dep_name = dep.get("name").and_then(|n| n.as_str()).unwrap_or("?");
            let kind = dep.get("kind").and_then(|k| k.as_str()).unwrap_or("normal");
            if members.contains(dep_name) {
                match layer_of(dep_name, layers) {
                    Some(dep_layer) if dep_layer == layer || allows(allowed_layers, dep_layer) => {}
                    Some(dep_layer) => problems.push(format!(
                        "LAYERING {} ({}) -> {} ({}) [{}]: not allowed (allowed: {})",
                        name,
                        layer,
                        dep_name,
                        dep_layer,
                        kind,
                        if allowed_layers.is_empty() { "none".to_string() } else { allowed_layers.join(", ") }
                    )),
                    None => {} // reported as UNLAYERED for the dependency itself
                }
            } else if let Some(may_use) = restricted.get(dep_name) {
                if !allows(may_use, layer) {
                    problems.push(format!(
                        "LAYERING {} ({}) -> {} [{}]: restricted to layer(s) {}",
                        name,
                        layer,
                        dep_name,
                        kind,
                        may_use.join(", ")
                    ));
                }
            }
        }
    }
    problems
}
