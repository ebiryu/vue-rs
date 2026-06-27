//! Fine-grained DOM rendering primitives for vue-rs.
//!
//! Rendering targets a [`Backend`]: the in-memory [`MockDom`] for native tests,
//! or `WebDom` (with the `web` feature) for the real browser DOM. The [`El`]
//! builder wires reactive text/attributes to the reactive core via effects.

mod backend;
mod callback;
mod element;
mod mock;
mod slots;
#[cfg(feature = "web")]
mod web;

pub use backend::Backend;
pub use callback::Callback;
pub use element::El;
pub use mock::MockDom;
pub use slots::Slots;
#[cfg(feature = "web")]
pub use web::{install_microtask_scheduler, WebDom};
