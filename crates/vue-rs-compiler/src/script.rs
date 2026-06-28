//! Rewrites Vue authoring spellings in a `<script>` body onto the reactive core.
//!
//! `ref` is a Rust keyword, so it cannot name the core constructor. The core
//! exposes `signal`; this rewrite lets a `.vrs` author write Vue's `ref(...)`
//! (and `use ...::ref`) and maps the constructor call and its import path to
//! `signal`. Genuine `ref` binding patterns (`let ref x`, `Some(ref v)`) are
//! left untouched.

use proc_macro2::{Delimiter, Group, Ident, TokenStream, TokenTree};

use crate::CompileError;

/// Rewrite Vue authoring spellings in a script body, returning the mapped tokens.
pub fn rewrite_script_sugar(script: &str) -> Result<TokenStream, CompileError> {
    let tokens: TokenStream = script
        .parse()
        .map_err(|e| CompileError(format!("invalid <script>: {e}")))?;
    Ok(rewrite_stream(tokens))
}

/// Walk a token sequence, mapping `ref` to `signal` where it acts as the
/// constructor (a call `ref(...)`) or an import-path segment (`...::ref`), and
/// recursing into every delimited group.
fn rewrite_stream(tokens: TokenStream) -> TokenStream {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    let mut out: Vec<TokenTree> = Vec::with_capacity(trees.len());
    for (i, tree) in trees.iter().enumerate() {
        match tree {
            TokenTree::Ident(id) if *id == "ref" => {
                if is_call(&trees, i) || is_path_segment(&trees, i) {
                    out.push(TokenTree::Ident(Ident::new("signal", id.span())));
                } else {
                    out.push(tree.clone());
                }
            }
            TokenTree::Group(g) => {
                let mut group = Group::new(g.delimiter(), rewrite_stream(g.stream()));
                group.set_span(g.span());
                out.push(TokenTree::Group(group));
            }
            other => out.push(other.clone()),
        }
    }
    out.into_iter().collect()
}

/// Whether the token at `i` is immediately followed by a `(...)` group (a call).
fn is_call(trees: &[TokenTree], i: usize) -> bool {
    matches!(
        trees.get(i + 1),
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis
    )
}

/// Whether the token at `i` is preceded by a `::` path separator.
fn is_path_segment(trees: &[TokenTree], i: usize) -> bool {
    i >= 2 && is_colon(&trees[i - 1]) && is_colon(&trees[i - 2])
}

fn is_colon(tree: &TokenTree) -> bool {
    matches!(tree, TokenTree::Punct(p) if p.as_char() == ':')
}
