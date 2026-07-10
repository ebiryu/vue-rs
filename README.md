# vue-rs

> Write Vue Single-File Components, but with the `<script>` in Rust.

vue-rs keeps Vue's `<template>` and `<style scoped>` authoring style and writes
the `<script>` in Rust, compiled to WebAssembly. The template is transpiled into
Rust source and merged into the same compilation unit as the script, then run on
a fine-grained reactive runtime in the style of Leptos and Sycamore. There is no
virtual DOM and no JS↔WASM boundary crossing at runtime, and the template itself
is type-checked by `rustc`.

```vue
<template>
  <div>
    <h1>vue-rs counter</h1>
    <button @click="$count += 1">count is {{ $count }}</button>
    <p v-if="$count > 4">that's a lot of clicks!</p>
  </div>
</template>

<script lang="rust">
use vue_rs_reactive::signal;

let count = signal(0);
</script>

<style scoped>
button { font-size: 1.2rem; padding: 0.5rem 1rem; }
</style>
```

## How it works

```
.vrs file ──split──▶ <template> AST + Rust <script> + scoped CSS
   <template> AST ──codegen──▶ El-builder Rust code (reactive view tree)
   script + template ──merge──▶ one fn Name<B: Backend>(backend, ..) -> B::Node
   cargo build --target wasm32 ──▶ app.wasm
```

- `{{ expr }}`, `:attr="expr"`, and `@event="handler"` bodies are passed through
  as Rust expressions. There is no custom expression parser: `rustc` resolves
  names and types against the bindings in scope.
- `{{ count.get() }}` compiles to a reactive text node that updates only that
  node when `count` changes. Static elements are built once.
- The `$name` template sugar stands in for `.value`: `$count` reads
  `count.get()`, and `$count = v` / `$count += v` write through `count.set(..)`.
  The core API is unchanged; the sugar is confined to template expressions.
- Directives map to runtime primitives: `v-if` / `v-else-if` / `v-else` →
  `dyn_switch`, keyed `v-for` → per-row fine-grained patching, `v-show`,
  `v-model` (text / checkbox / radio / textarea / select), `v-html`, and
  `v-text`. `:class` / `:style` accept Vue's object and array forms and merge
  with the static attribute. Event, key, mouse, and system modifiers
  (`.stop`, `.prevent`, `.enter`, `.ctrl`, `.exact`, …) are supported.

## Reactivity

The reactive core (`vue-rs-reactive`) is a pull-based, alien-signals-style runtime
(generational arena, intrusive doubly-linked dependency links, async scheduler with
`batch` / `next_tick`). It is pure Rust and testable without a browser.

| Vue | vue-rs |
| --- | --- |
| `ref(0)` | `signal(0) -> Signal<T>` |
| `x.value` (read/write) | `x.get()` / `x.set(v)` / `x.update(\|v\| ...)` / `x.with(\|v\| ...)` |
| `computed(fn)` | `computed(move \|\| ...) -> Memo<T>` |
| writable `computed` | `writable_computed(get, set) -> WritableMemo<T>` |
| `watch(src, cb)` | `watch(...)` / `watch_immediate(...)` / `watch_with(.., WatchOptions)` |
| `watchEffect(fn)` | `effect(move \|\| ...)` |
| `effectScope()` | `effect_scope() -> EffectScope` (`run` / `stop`) |
| `reactive(obj)` | `#[derive(Reactive)]` struct + `reactive(Name { .. })` |
| `readonly(x)` | `ReadSignal<T>` (props flow down as read-only views) |
| `onMounted` / `onUnmounted` | `on_mounted(...)` / `on_unmounted(...)` |
| `provide` / `inject` | `provide_context::<T>(v)` / `use_context::<T>()` |
| `defineProps` | `struct NameProps { .. }` in `<script>` |
| `defineEmits` | `Callback<T>` props |

Like Vue, `signal` and `computed` dedup on equality (`T: PartialEq`); the escape
hatches for non-comparable values are `signal_raw` / `computed_raw`. A child prop
declared as `MaybeSignal<T>` accepts either a static value or a reactive source,
mirroring Vue props that may be passed either way.

> **Note on `ref`:** `ref` is a reserved word in Rust, so the core constructor is
> `signal()`. The template's `ref="el"` (template refs) is unrelated and works.

## Workspace layout

```
vue-rs/
├── crates/
│   ├── vue-rs-reactive/   # fine-grained reactive core (pure Rust, tested)
│   ├── vue-rs-dom/        # DOM layer: Backend trait, El builder, WebDom / MockDom
│   ├── vue-rs-compiler/   # .vrs splitter + <template> → Rust codegen + scoped CSS
│   ├── vue-rs-macro/      # component! / view! macros and #[derive(Reactive)]
│   ├── vue-rs-build/      # build.rs helper: compile a directory of .vrs files
│   └── vue-rs/            # facade: re-exports the above under one namespace
└── examples/
    ├── counter-web/       # .vrs counter, compiled via build.rs
    └── todo-web/          # TodoMVC-style demo (v-for / v-if / v-model / computed)
```

The DOM layer is abstracted behind a `Backend` trait: `MockDom` for native
`cargo test`, and `WebDom` (`web-sys`, behind the `web` feature) for the browser.
The same trait leaves room for other backends such as SSR.

Applications can depend on the `vue-rs` facade alone and reach the whole API via
`use vue_rs::*;` (enable `features = ["web"]` for `WebDom`).

## Getting started

### Run the test suite (native, no wasm needed)

```sh
cargo test
cargo clippy --all-targets
```

The reactive core and the compiler/codegen are verified on the native target via
`MockDom`.

### Build a browser example

The browser examples are wasm-only (they use the `web` feature) and are excluded
from the native workspace build. Build them with [trunk](https://trunkrs.dev):

```sh
cd examples/counter-web
trunk serve        # or: trunk build
```

### Author a component

A `.vrs` file has the familiar three blocks. The `<script>`'s top-level `let`
bindings, functions, `use` items, and any declared structs are spliced into the
generated render function, so the template captures them directly.

Each component compiles to a function whose name is the PascalCase form of the
file stem, plus a `NAME_STYLE` constant when a `<style>` block is present:

```
fn Name<B: Backend>(backend: B, [props: NameProps,] slots: NameSlots) -> B::Node
```

The `slots` argument is always present (a unit struct when the template has no
`<slot>`); a `props` argument is added only when the `<script>` declares a
`struct NameProps`. Both `NameProps` and `NameSlots` implement `Default`, so a
component with neither is called as `Name(backend, Default::default())`.

There are two ways to compile a `.vrs` file. The `component!` macro reads one
file inline:

```rust
use vue_rs_dom::WebDom;
use vue_rs_macro::component;
use wasm_bindgen::prelude::*;

component!(Counter, "src/counter.vrs");

#[wasm_bindgen(start)]
pub fn start() {
    let dom = WebDom;
    dom.inject_style(COUNTER_STYLE);
    let node = Counter(dom.clone(), Default::default());
    dom.mount(&node);
}
```

Or a `build.rs` compiles every `.vrs` under a directory at build time:

```rust
// build.rs
fn main() {
    vue_rs_build::compile_dir("src").expect("compiling .vrs components");
}
```

```rust
// src/lib.rs — each component lives in its own module.
include!(concat!(env!("OUT_DIR"), "/vue_rs_components.rs"));
use components::Counter;

// components::counter::COUNTER_STYLE holds the scoped CSS.
```

## Status

Implemented: the reactive core (including `watch`, `effectScope`, writable
`computed`, and `#[derive(Reactive)]`), the DOM layer, and the SFC compiler.
Templates cover interpolation, `:attr` / `@event`, the `$name` sugar,
`:class` / `:style` objects, control flow (`v-if` / keyed `v-for`), `v-model`
(text / checkbox / radio / textarea / select), `v-show`, `v-html`, `v-text`,
event and key modifiers, dynamic arguments, fragments, template refs, and
dynamic components (`<component :is>`). Component composition covers props with
defaults, emits, default and named/scoped slots, `provide` / `inject`, lifecycle
hooks, and scoped CSS. The `component!` macro and the `vue-rs-build` `build.rs`
path both compile `.vrs` files. All of this is exercised on the native target and
confirmed in a real browser through the examples.

Not yet built: richer build tooling (a Vite plugin, HMR, source maps),
SSR / hydration, `<Transition>` and other built-in components, and error
boundaries. `reactive`, component `v-model`, and template refs are partial.

## License

MIT
