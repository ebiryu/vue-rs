# todo-web — vue-rs TodoMVC デモ

`.vrs` 単一コンポーネント（[src/todo_app.vrs](src/todo_app.vrs)）で、vue-rs の主要機能を総合的に使うデモ。

- **v-model**: 入力欄を `draft` シグナルに双方向バインド（追加後にクリア）
- **v-for(keyed)**: `:key="todo.id"` で TODO 一覧を差分描画
- **イベント**: 行内の「toggle / x」ボタンが Rust クロージャを呼ぶ
- **computed**: 残数 `remaining` を派生
- **v-if**: 空のときにメッセージ表示
- **scoped style**: `data-v-*` + `inject_style`

## 実行

```sh
cd examples/todo-web
trunk serve --open      # http://localhost:8080
```

## 実装メモ

- 各 TODO は `Todo { id, text: Signal<String>, done: Signal<bool> }`（全フィールド `Copy`）。
  `done` をシグナルにすることで、リストを作り直さずに行内だけ細粒度更新できる。
- リアクティブな読み取り（`with`/`get`）のクロージャ内では reactive な書き込みを行わない。
  必要な値（シグナル）を取り出してから外で `set` する（`toggle` 参照）。
