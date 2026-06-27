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
    <button @click="increment()">count is {{ count.get() }}</button>
    <p v-if="count.get() > 4">that's a lot of clicks!</p>
  </div>
</template>

<script lang="rust">
use vue_rs_reactive::signal;

let count = signal(0);
let increment = move || count.set(count.get() + 1);
</script>

<style scoped>
button { font-size: 1.2rem; padding: 0.5rem 1rem; }
</style>
```

## How it works

```
.vrs file ──split──▶ <template> AST + Rust <script> + scoped CSS
   <template> AST ──codegen──▶ El-builder Rust code (reactive view tree)
   component!  ──merges──▶ one  fn name<B: Backend>(backend) -> B::Node
   cargo build --target wasm32 ──▶ app.wasm
```

- `{{ expr }}`, `:attr="expr"`, and `@event="handler"` bodies are passed through
  as Rust expressions. There is no custom expression parser: `rustc` resolves
  names and types against the bindings in scope.
- `{{ count.get() }}` compiles to a reactive text node that updates only that
  node when `count` changes. Static elements are built once.
- Directives map to runtime primitives: `v-if`/`v-else` → `dyn_if`/`dyn_if_else`,
  keyed `v-for` → `dyn_for`, `v-model` (text) → value bind + `oninput`.

## Reactivity

The reactive core (`vue-rs-reactive`) is a pull-based, alien-signals-style runtime
(generational arena, intrusive doubly-linked dependency links, async scheduler with
`batch` / `next_tick`). It is pure Rust and fully testable without a browser.

| Vue | vue-rs |
| --- | --- |
| `ref(0)` | `signal(0) -> Signal<T>` |
| `x.value` (read/write) | `x.get()` / `x.set(v)` / `x.update(\|v\| ...)` / `x.with(\|v\| ...)` |
| `computed(fn)` | `computed(move \|\| ...) -> Memo<T>` |
| `watchEffect(fn)` | `effect(move \|\| ...)` |
| `onMounted` / `onUnmounted` | `on_mounted(...)` / `on_unmounted(...)` |
| `provide` / `inject` | `provide_context::<T>(v)` / `use_context::<T>()` |
| `defineProps` | `struct NameProps { .. }` in `<script>` |
| `defineEmits` | `Callback<T>` props |

Like Vue, `signal` and `computed` dedup on equality (`T: PartialEq`); the escape
hatches for non-comparable values are `signal_raw` / `computed_raw`.

> **Note on `ref`:** `ref` is a reserved word in Rust, so the core constructor is
> `signal()`.

## Workspace layout

```
vue-rs/
├── crates/
│   ├── vue-rs-reactive/   # fine-grained reactive core (pure Rust, fully tested)
│   ├── vue-rs-dom/        # DOM layer: Backend trait, El builder, WebDom / MockDom
│   ├── vue-rs-compiler/   # .vrs splitter + <template> → Rust codegen + scoped CSS
│   └── vue-rs-macro/      # view! and component! procedural macros
└── examples/
    ├── counter-web/       # .vrs counter mounted in the real browser DOM
    └── todo-web/          # TodoMVC-class demo (v-for / v-if / v-model / computed)
```

The DOM layer is abstracted behind a `Backend` trait: `MockDom` for native
`cargo test`, and `WebDom` (`web-sys`, behind the `web` feature) for the browser.
The same trait leaves room for future backends (e.g. SSR).

## Getting started

### Run the test suite (native, no wasm needed)

```sh
cargo test
cargo clippy --all-targets
```

The reactive core and the compiler/codegen are verified entirely on the native
target via `MockDom`.

### Build a browser example

The browser examples are wasm-only (they use the `web` feature) and are excluded
from the native workspace build. Build them with [trunk](https://trunkrs.dev):

```sh
cd examples/counter-web
trunk serve        # or: trunk build
```

### Author a component

A `.vrs` file has the familiar three blocks. The `<script>`'s top-level `let`
bindings, functions, and `use` items are spliced directly into the generated
render function, so the template captures them automatically:

```rust
use vue_rs_dom::WebDom;
use vue_rs_macro::component;
use wasm_bindgen::prelude::*;

// Generates `fn counter<B: Backend>(backend: B) -> B::Node` and `COUNTER_STYLE`.
component!(counter, "src/counter.vrs");

#[wasm_bindgen(start)]
pub fn start() {
    let dom = WebDom;
    dom.inject_style(COUNTER_STYLE);
    let node = counter(dom.clone());
    dom.mount(&node);
}
```

If the `<script>` declares a `struct CounterProps { .. }`, the generated function
gains a `props: CounterProps` parameter; a `<slot>` in the template adds a
`Slots<B>` parameter.

## Status

Implemented: the reactive core, the DOM layer, the SFC compiler, control flow
(`v-if` / keyed `v-for` / `v-model`), component composition (props/emit, default
and named slots, `provide`/`inject`, lifecycle hooks), and scoped CSS. All of
this is exercised end-to-end and confirmed in a real browser through the examples.

Not yet built: build tooling (Vite plugin / HMR / source maps), SSR/hydration,
`<Transition>`, error boundaries, and the `ref`/`$count` authoring sugar.

## License

MIT
