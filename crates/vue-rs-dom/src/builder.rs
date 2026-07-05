//! Typestate markers for generated props builders.
//!
//! A component's generated `NamePropsBuilder` carries one type parameter per
//! required prop, each either [`Unset`] or [`Set`]. A required-prop setter flips
//! its marker from `Unset` to `Set`, and `build()` exists only when every marker
//! is `Set`. Omitting a required prop therefore leaves its marker `Unset`, and
//! the `.build()` call fails to compile — required props stay checked at compile
//! time even though the parent passes props by name through the builder.

/// Marker for a required prop that has not been provided yet.
pub struct Unset;

/// Marker for a required prop that has been provided.
pub struct Set;
