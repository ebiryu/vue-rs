//! Browser demo: mounts the `.vrs` counter component into the real DOM.

use vue_rs_dom::WebDom;
use vue_rs_macro::component;
use wasm_bindgen::prelude::*;

component!(counter, "src/counter.vrs");

#[wasm_bindgen(start)]
pub fn start() {
    let dom = WebDom;
    let node = counter(dom.clone());
    dom.mount(&node);
}
