//! Fine-grained DOM rendering primitives for vue-rs.
//!
//! Rendering targets a [`Backend`]: the in-memory [`MockDom`] for native tests,
//! or `WebDom` (with the `web` feature) for the real browser DOM. The [`El`]
//! builder wires reactive text/attributes to the reactive core via effects.

mod backend;
mod callback;
mod class;
mod element;
mod mock;
mod slots;
mod style;
#[cfg(feature = "web")]
mod web;

pub use backend::{Backend, EventOptions};
pub use callback::Callback;
pub use class::ClassList;
pub use element::{switch_views, El, RawHtml};
pub use mock::{DispatchOutcome, MockDom, MockEvent};
pub use slots::SlotFn;
pub use style::StyleList;
#[cfg(feature = "web")]
pub use web::{install_microtask_scheduler, WebDom};
