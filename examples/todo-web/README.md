# todo-web TodoMVC demo

A demo built from a single `.vrs` component ([src/todo_app.vrs](src/todo_app.vrs)) that exercises vue-rs's main features together.

- **v-model**: two-way binds the input field to the `draft` signal (cleared after adding)
- **v-for (keyed)**: renders the TODO list with diffing via `:key="todo.id"`
- **events**: the per-row "toggle / x" buttons call Rust closures
- **computed**: derives the remaining count `remaining`
- **v-if**: shows a message when the list is empty
- **scoped style**: `data-v-*` + `inject_style`

## Running

```sh
cd examples/todo-web
trunk serve --open      # http://localhost:8080
```

## Implementation notes

- Each TODO is `Todo { id, text: Signal<String>, done: Signal<bool> }` (all fields `Copy`).
  Making `done` a signal lets each row update in a fine-grained way without rebuilding the list.
- Do not perform reactive writes inside a reactive read (`with` / `get`) closure.
  Extract the value (signal) you need first, then `set` it outside the closure (see `toggle`).
