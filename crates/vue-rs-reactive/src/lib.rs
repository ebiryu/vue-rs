//! Fine-grained reactivity core for vue-rs.
//!
//! Public API: [`signal`], [`computed`], [`effect`], the scope helpers
//! [`create_root`] / [`on_cleanup`], and the handle types [`Signal`] / [`Memo`].

mod runtime;

pub use runtime::{
    computed, create_root, create_root_detached, effect, on_cleanup, signal, Memo, RootDisposer,
    Signal,
};
