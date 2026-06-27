use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::backend::Backend;

/// Browser backend rendering into the real DOM via `web-sys`.
#[derive(Clone, Default)]
pub struct WebDom;

fn document() -> web_sys::Document {
    web_sys::window()
        .expect("no window")
        .document()
        .expect("no document")
}

impl WebDom {
    /// Append `node` to the document body, then run `on_mounted` callbacks.
    pub fn mount(&self, node: &web_sys::Node) {
        let body = document().body().expect("document has no body");
        body.append_child(node).expect("mount append_child");
        vue_rs_reactive::flush_mounted();
    }

    /// Inject a `<style>` element with the given CSS into the document head.
    pub fn inject_style(&self, css: &str) {
        let document = document();
        let style = document.create_element("style").expect("create style");
        style.set_text_content(Some(css));
        let head = document.head().expect("document has no head");
        head.append_child(&style).expect("append style");
    }
}

impl Backend for WebDom {
    type Node = web_sys::Node;

    fn create_element(&self, tag: &str) -> web_sys::Node {
        document()
            .create_element(tag)
            .expect("create_element")
            .unchecked_into()
    }

    fn create_text(&self, data: &str) -> web_sys::Node {
        document().create_text_node(data).unchecked_into()
    }

    fn create_anchor(&self) -> web_sys::Node {
        document().create_comment("").unchecked_into()
    }

    fn set_text(&self, node: &web_sys::Node, data: &str) {
        node.set_text_content(Some(data));
    }

    fn set_attribute(&self, node: &web_sys::Node, name: &str, value: &str) {
        // For inputs, `value` is a live property, not just an attribute — set it
        // so v-model reflects programmatic changes (e.g. clearing after submit).
        if name == "value"
            && let Some(input) = node.dyn_ref::<web_sys::HtmlInputElement>()
        {
            input.set_value(value);
            return;
        }
        let element: &web_sys::Element = node.unchecked_ref();
        element.set_attribute(name, value).expect("set_attribute");
    }

    fn append_child(&self, parent: &web_sys::Node, child: &web_sys::Node) {
        parent.append_child(child).expect("append_child");
    }

    fn insert_before(&self, parent: &web_sys::Node, child: &web_sys::Node, anchor: &web_sys::Node) {
        parent
            .insert_before(child, Some(anchor))
            .expect("insert_before");
    }

    fn remove_child(&self, parent: &web_sys::Node, child: &web_sys::Node) {
        parent.remove_child(child).expect("remove_child");
    }

    fn add_event_listener(&self, node: &web_sys::Node, event: &str, handler: Rc<dyn Fn(&str)>) {
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            let value = event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
                .map(|input| input.value())
                .unwrap_or_default();
            handler(&value);
        });
        let target: &web_sys::EventTarget = node.unchecked_ref();
        target
            .add_event_listener_with_callback(event, closure.as_ref().unchecked_ref())
            .expect("add_event_listener");
        // Hand the closure to the JS GC roots so it stays alive with the listener.
        closure.forget();
    }
}
