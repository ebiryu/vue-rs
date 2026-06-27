//! Scoped-style helpers: deriving a scope id and rewriting CSS to target it.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Derive a short, stable-per-build scope id from a seed (e.g. the file path).
pub fn scope_id(seed: &str) -> String {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    format!("{:08x}", hasher.finish() as u32)
}

/// Rewrite each selector to also require the `[data-v-<id>]` marker attribute,
/// so the rules only match this component's elements.
///
/// Minimal: the marker is appended to the end of each comma-separated selector.
/// Pseudo-classes and `::before`/`::after` are not specially handled yet.
pub fn scope_css(css: &str, scope_id: &str) -> String {
    let marker = format!("[data-v-{scope_id}]");
    let mut out = String::new();
    for rule in css.split_inclusive('}') {
        match rule.find('{') {
            Some(brace) => {
                let (selectors, body) = rule.split_at(brace);
                let scoped: Vec<String> = selectors
                    .split(',')
                    .map(|sel| format!("{}{}", sel.trim(), marker))
                    .collect();
                out.push_str(&scoped.join(", "));
                out.push_str(body);
            }
            None => out.push_str(rule),
        }
    }
    out
}
