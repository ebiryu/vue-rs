//! Browser demo: mounts the `.vrs` counter component into the real DOM.
//! Components under `src/` are compiled at build time by `vue-rs-build`.

use vue_rs_dom::WebDom;
use wasm_bindgen::prelude::*;

include!(concat!(env!("OUT_DIR"), "/vue_rs_components.rs"));
use components::Counter;

#[wasm_bindgen(start)]
pub fn start() {
    let dom = WebDom;
    dom.inject_style(components::counter::COUNTER_STYLE);
    let node = Counter(dom.clone(), Default::default());
    dom.mount(&node);
}
