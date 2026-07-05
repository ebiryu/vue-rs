//! Converts a value into a bag of attributes for the `v-bind="obj"` directive.

use std::fmt::Display;

/// Converts a value into an ordered list of `(name, value)` attribute pairs.
///
/// This backs the template compiler's `v-bind="obj"` (bulk bind) directive: the
/// bound expression is turned into pairs that are spread onto the element. Keys
/// and values are stringified through [`Display`], so a bag of heterogeneous
/// values (numbers, strings) can be bound as long as each is homogeneous within
/// the collection.
pub trait IntoAttrs {
    /// The `(name, value)` pairs to set, in order.
    fn into_attrs(self) -> Vec<(String, String)>;
}

impl<K: Display, V: Display> IntoAttrs for Vec<(K, V)> {
    fn into_attrs(self) -> Vec<(String, String)> {
        self.into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
}

impl<K: Display, V: Display, const N: usize> IntoAttrs for [(K, V); N] {
    fn into_attrs(self) -> Vec<(String, String)> {
        self.into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
}
