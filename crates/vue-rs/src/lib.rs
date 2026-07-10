//! vue-rs のファサード crate。リアクティブコア・DOM 層・マクロを 1 つの名前空間へ
//! まとめて再輸出する。利用者はこの crate だけに依存し `use vue_rs::*;` で API に届く。
//!
//! ブラウザ (`WebDom`) を使うときは `features = ["web"]` を有効化する。

// リアクティブコア（signal / computed / effect / scope / watch / reactive ほか）。
pub use vue_rs_reactive::*;

// DOM 層（Backend / El / Callback / MockDom ほか。WebDom は `web` feature 時のみ）。
pub use vue_rs_dom::*;

// マクロ（テンプレート・コンポーネント・derive）。
pub use vue_rs_macro::{component, view, Reactive};
