//! Fine-grained reactivity core for vue-rs.
//!
//! Public API: [`signal`], [`computed`], [`effect`], the scope helpers
//! [`create_root`] / [`on_cleanup`] / [`effect_scope`] (with [`on_scope_dispose`]
//! / [`get_current_scope`]), and the handle types [`Signal`] / [`Memo`] /
//! [`ReadSignal`] (a read-only view, used for props).

mod runtime;

pub use runtime::{
    batch, clear_scheduler, computed, computed_raw, create_root, create_root_detached, effect,
    effect_scope, effect_scope_detached, flush_jobs, flush_mounted, get_current_scope, next_tick,
    on_cleanup, on_mounted, on_scope_dispose, on_unmounted, provide_context, reactive, readonly,
    run_in_child_scope, set_scheduler, signal, signal_raw, use_context, watch, watch_immediate,
    watch_with, writable_computed, EffectScope,
    Flush, MaybeSignal, Memo, Reactive, Readonly, ReadSignal, RootDisposer, Signal, WatchOptions,
    WritableMemo,
};
