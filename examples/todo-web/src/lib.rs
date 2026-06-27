//! TodoMVC-style demo exercising v-for, v-if, v-model, events, and computed.

use vue_rs_dom::WebDom;
use vue_rs_macro::component;
use wasm_bindgen::prelude::*;

component!(TodoApp, "src/todo_app.vrs");

#[wasm_bindgen(start)]
pub fn start() {
    let dom = WebDom;
    dom.inject_style(TODOAPP_STYLE);
    let node = TodoApp(dom.clone());
    dom.mount(&node);
}
