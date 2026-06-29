//! Builds a `style` attribute string from individual CSS declarations.

/// Accumulates CSS declarations into a single `;`-separated `style` string.
///
/// Empty fragments and empty-valued properties are skipped, so a blank
/// expression never leaves a stray separator. This backs the template
/// compiler's `:style` object (`{ color: c }`) and array (`[a, b]`) syntax, and
/// the merge of a static `style` with a dynamic `:style`.
#[derive(Default)]
pub struct StyleList(String);

impl StyleList {
    /// An empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a declaration fragment (e.g. `"color: red"`, possibly several
    /// `;`-separated declarations), skipping it when empty. A trailing `;` is
    /// trimmed so joined fragments keep a single `; ` separator.
    pub fn push(mut self, decl: impl AsRef<str>) -> Self {
        let decl = decl.as_ref().trim();
        let decl = decl.strip_suffix(';').unwrap_or(decl).trim_end();
        if !decl.is_empty() {
            if !self.0.is_empty() {
                self.0.push_str("; ");
            }
            self.0.push_str(decl);
        }
        self
    }

    /// Append a single `prop: value` declaration (the object-syntax entry),
    /// skipping it when `value` is empty.
    pub fn push_prop(self, prop: &str, value: impl AsRef<str>) -> Self {
        let value = value.as_ref().trim();
        if value.is_empty() {
            self
        } else {
            self.push(format!("{prop}: {value}"))
        }
    }

    /// The joined `style` string.
    pub fn finish(self) -> String {
        self.0
    }
}
