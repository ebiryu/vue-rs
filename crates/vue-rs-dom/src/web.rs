use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::backend::{Backend, EventOptions};

/// Browser backend rendering into the real DOM via `web-sys`.
#[derive(Clone, Default)]
pub struct WebDom;

fn document() -> web_sys::Document {
    web_sys::window()
        .expect("no window")
        .document()
        .expect("no document")
}

thread_local! {
    /// Persistent closure that drains the reactive job queue, reused for every
    /// scheduled microtask. Kept alive here so the JS callback stays valid.
    static FLUSH_CLOSURE: RefCell<Option<Closure<dyn FnMut()>>> = const { RefCell::new(None) };
}

/// Route reactive flushes through the browser microtask queue (`queueMicrotask`).
/// After this call, synchronous writes are coalesced and effects re-run once per
/// microtask, matching Vue's asynchronous update scheduling.
pub fn install_microtask_scheduler() {
    FLUSH_CLOSURE.with_borrow_mut(|slot| {
        if slot.is_none() {
            *slot = Some(Closure::<dyn FnMut()>::new(vue_rs_reactive::flush_jobs));
        }
    });
    vue_rs_reactive::set_scheduler(|| {
        let window = web_sys::window().expect("no window");
        FLUSH_CLOSURE.with_borrow(|slot| {
            let closure = slot.as_ref().expect("flush closure installed");
            window.queue_microtask(closure.as_ref().unchecked_ref());
        });
    });
}

impl WebDom {
    /// Append `node` to the document body, enable microtask-based scheduling, then
    /// run `on_mounted` callbacks.
    pub fn mount(&self, node: &web_sys::Node) {
        install_microtask_scheduler();
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
    /// The event name, its capture flag (needed to detach), and the live JS
    /// closure; dropping the closure releases it.
    type Listener = (String, bool, Closure<dyn FnMut(web_sys::Event)>);

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

    fn set_inner_html(&self, node: &web_sys::Node, html: &str) {
        let element: &web_sys::Element = node.unchecked_ref();
        element.set_inner_html(html);
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

    fn add_event_listener(
        &self,
        node: &web_sys::Node,
        event: &str,
        options: EventOptions,
        handler: Rc<dyn Fn(&str)>,
    ) -> Self::Listener {
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            // Guard modifiers short-circuit before prevent/stop and the handler.
            if options.self_only {
                let same = match (event.target(), event.current_target()) {
                    (Some(t), Some(c)) => js_sys::Object::is(t.as_ref(), c.as_ref()),
                    _ => false,
                };
                if !same {
                    return;
                }
            }
            if !options.keys.is_empty() {
                let key = event.dyn_ref::<web_sys::KeyboardEvent>().map(|e| e.key());
                if !key.is_some_and(|k| options.keys.contains(&k.as_str())) {
                    return;
                }
            }
            if !options.buttons.is_empty() {
                let button = event.dyn_ref::<web_sys::MouseEvent>().map(|e| e.button());
                if !button.is_some_and(|b| options.buttons.iter().any(|w| *w as i16 == b)) {
                    return;
                }
            }
            if options.prevent_default {
                event.prevent_default();
            }
            if options.stop_propagation {
                event.stop_propagation();
            }
            let value = event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
                .map(|input| input.value())
                .unwrap_or_default();
            handler(&value);
        });
        let target: &web_sys::EventTarget = node.unchecked_ref();
        let listener_options = web_sys::AddEventListenerOptions::new();
        listener_options.set_once(options.once);
        listener_options.set_capture(options.capture);
        listener_options.set_passive(options.passive);
        target
            .add_event_listener_with_callback_and_add_event_listener_options(
                event,
                closure.as_ref().unchecked_ref(),
                &listener_options,
            )
            .expect("add_event_listener");
        // Keep the closure alive by handing it back to the caller, which holds it
        // until `remove_event_listener` drops it together with the listener.
        (event.to_string(), options.capture, closure)
    }

    fn remove_event_listener(&self, node: &web_sys::Node, listener: Self::Listener) {
        let (event, capture, closure) = listener;
        let target: &web_sys::EventTarget = node.unchecked_ref();
        target
            .remove_event_listener_with_callback_and_bool(
                &event,
                closure.as_ref().unchecked_ref(),
                capture,
            )
            .expect("remove_event_listener");
        // `closure` is dropped here, freeing the boxed Rust handler.
    }
}
