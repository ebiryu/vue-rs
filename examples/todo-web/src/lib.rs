//! TodoMVC-style demo exercising v-for, v-if, v-model, events, and computed.
//! Components under `src/` are compiled at build time by `vue-rs-build`.

use vue_rs_dom::WebDom;
use wasm_bindgen::prelude::*;

include!(concat!(env!("OUT_DIR"), "/vue_rs_components.rs"));
use components::TodoApp;

#[wasm_bindgen(start)]
pub fn start() {
    let dom = WebDom;
    dom.inject_style(components::todo_app::TODOAPP_STYLE);
    let node = TodoApp(dom.clone(), Default::default());
    dom.mount(&node);
}
