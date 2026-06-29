//! Builds a `class` attribute string from individual fragments.

/// Accumulates class-name fragments into a single space-separated string.
///
/// Empty fragments are skipped, so a falsy conditional or a blank expression
/// never leaves a stray space. This backs the template compiler's `:class`
/// object (`{ active: cond }`) and array (`[a, b]`) syntax, and the merge of a
/// static `class` with a dynamic `:class`.
#[derive(Default)]
pub struct ClassList(String);

impl ClassList {
    /// An empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append `class` unless it is empty, separating it from the previous
    /// fragment with a single space.
    pub fn push(mut self, class: impl AsRef<str>) -> Self {
        let class = class.as_ref();
        if !class.is_empty() {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            self.0.push_str(class);
        }
        self
    }

    /// Append `class` only when `cond` holds (the object-syntax entry).
    pub fn push_if(self, class: impl AsRef<str>, cond: bool) -> Self {
        if cond {
            self.push(class)
        } else {
            self
        }
    }

    /// The joined `class` string.
    pub fn finish(self) -> String {
        self.0
    }
}
