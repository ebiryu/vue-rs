//! Splits a `.vrs` single-file component into its top-level blocks.
//!
//! Top-level `<template>`, `<script>`, and `<style>` blocks are located by their
//! opening/closing tags. As in HTML, a block's content cannot contain its own
//! closing tag, so the first matching `</tag>` terminates the block.

use crate::CompileError;

/// The blocks of a parsed single-file component.
pub struct Sfc {
    pub template: String,
    pub script: Option<String>,
    pub style: Option<String>,
}

/// Split a `.vrs` source into its `<template>`, `<script>`, and `<style>` blocks.
pub fn split_sfc(source: &str) -> Result<Sfc, CompileError> {
    let template = extract_block(source, "template")
        .map_err(CompileError)?
        .ok_or_else(|| CompileError("`.vrs` is missing a <template> block".to_string()))?;
    let script = extract_block(source, "script").map_err(CompileError)?;
    let style = extract_block(source, "style").map_err(CompileError)?;
    Ok(Sfc {
        template: template.trim().to_string(),
        script: script.map(|s| s.trim().to_string()),
        style: style.map(|s| s.trim().to_string()),
    })
}

/// Return the inner content of the first `<tag ...>...</tag>` block, if present.
fn extract_block(source: &str, tag: &str) -> Result<Option<String>, String> {
    let open = format!("<{tag}");
    let mut from = 0;
    while let Some(rel) = source[from..].find(&open) {
        let start = from + rel;
        // Reject names that merely share a prefix, e.g. `<template-foo>`.
        let next = source[start + open.len()..].chars().next();
        if matches!(next, Some(c) if c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            from = start + open.len();
            continue;
        }
        let open_end = source[start..]
            .find('>')
            .ok_or_else(|| format!("unterminated <{tag}> opening tag"))?;
        let content_start = start + open_end + 1;
        let close = format!("</{tag}>");
        let close_rel = source[content_start..]
            .find(&close)
            .ok_or_else(|| format!("missing </{tag}> closing tag"))?;
        return Ok(Some(source[content_start..content_start + close_rel].to_string()));
    }
    Ok(None)
}
