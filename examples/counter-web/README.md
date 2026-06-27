# counter-web — vue-rs ブラウザデモ

`.vrs` SFC（[src/counter.vrs](src/counter.vrs)）を `WebDom` で実ブラウザの DOM にマウントするデモ。

## 実行

[trunk](https://trunk-rs.github.io/trunk/) を使う:

```sh
cargo install trunk          # 初回のみ
cd examples/counter-web
trunk serve --open           # http://localhost:8080 でホットリロード付き起動
```

本番ビルドは `trunk build --release`（出力は `dist/`）。

## コンパイルだけ確認する

```sh
cargo build --manifest-path examples/counter-web/Cargo.toml --target wasm32-unknown-unknown
```

## 仕組み

- `component!(counter, "src/counter.vrs")` が `<template>`/`<script lang="rust">`/`<style scoped>` を
  単一の `fn counter<B: Backend>(__backend: B) -> B::Node` にコンパイルする。
- `#[wasm_bindgen(start)]` の `start()` が `WebDom` を使ってコンポーネントを構築し、`<body>` にマウントする。
- ボタンのクリックは Rust 側 `count` シグナルを更新し、`{{ count.get() }}` と `v-if` が細粒度に再描画される。
