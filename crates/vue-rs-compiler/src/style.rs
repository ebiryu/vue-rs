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
/// The marker is placed on the last compound selector (the rightmost target),
/// before any pseudo-class/pseudo-element so the result stays valid CSS:
/// `button:hover` becomes `button[data-v-id]:hover`, `.a .b` becomes
/// `.a .b[data-v-id]`. Combinator and `:` characters inside `[...]`/`(...)`
/// (e.g. `:nth-child(2n+1)`) are not treated as structure.
pub fn scope_css(css: &str, scope_id: &str) -> String {
    let marker = format!("[data-v-{scope_id}]");
    let mut out = String::new();
    for rule in css.split_inclusive('}') {
        match rule.find('{') {
            Some(brace) => {
                let (selectors, body) = rule.split_at(brace);
                let scoped: Vec<String> = selectors
                    .split(',')
                    .map(|sel| scope_selector(sel.trim(), &marker))
                    .collect();
                out.push_str(&scoped.join(", "));
                out.push_str(body);
            }
            None => out.push_str(rule),
        }
    }
    out
}

/// Insert `marker` into a single (already trimmed) selector: at the end of the
/// last compound selector, but before its first pseudo-class/element.
fn scope_selector(sel: &str, marker: &str) -> String {
    // Start of the last compound selector = just after the last top-level
    // combinator (whitespace / `>` / `+` / `~`). Chars inside `[]`/`()` are
    // skipped so attribute selectors and pseudo args don't split the compound.
    let mut depth = 0i32;
    let mut compound_start = 0;
    for (i, c) in sel.char_indices() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = (depth - 1).max(0),
            ' ' | '\t' | '\n' | '>' | '+' | '~' if depth == 0 => {
                compound_start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    let (head, compound) = sel.split_at(compound_start);

    // Within the compound, the marker goes before the first top-level `:`.
    let mut depth = 0i32;
    let mut insert_at = compound.len();
    for (i, c) in compound.char_indices() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = (depth - 1).max(0),
            ':' if depth == 0 => {
                insert_at = i;
                break;
            }
            _ => {}
        }
    }
    let (base, pseudo) = compound.split_at(insert_at);
    format!("{head}{base}{marker}{pseudo}")
}
