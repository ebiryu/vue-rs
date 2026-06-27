# counter-web browser demo

A demo that mounts a `.vrs` SFC ([src/counter.vrs](src/counter.vrs)) onto a real browser DOM using `WebDom`.

## Running

Use [trunk](https://trunk-rs.github.io/trunk/):

```sh
cargo install trunk          # first time only
cd examples/counter-web
trunk serve --open           # serves at http://localhost:8080 with hot reload
```

For a production build, run `trunk build --release` (output goes to `dist/`).

## Checking compilation only

```sh
cargo build --manifest-path examples/counter-web/Cargo.toml --target wasm32-unknown-unknown
```

## How it works

- `component!(counter, "src/counter.vrs")` compiles the `<template>` / `<script lang="rust">` / `<style scoped>`
  into a single `fn counter<B: Backend>(__backend: B) -> B::Node`.
- The `start()` function marked with `#[wasm_bindgen(start)]` builds the component using `WebDom` and mounts it to `<body>`.
- A button click updates the Rust-side `count` signal, and `{{ count.get() }}` together with `v-if` re-render in a fine-grained manner.
