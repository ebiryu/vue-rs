use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::backend::{Backend, EventOptions};

/// Browser backend rendering into the real DOM via `web-sys`.
#[derive(Clone, Default)]
pub struct WebDom;

/// A backend node: either a single real DOM node, or a fragment grouping several
/// sibling nodes (a template with multiple roots). The DOM has no persistent node
/// that represents a group of siblings — a `DocumentFragment` empties when
/// inserted — so a fragment keeps its members in a shared list and applies
/// append/insert/remove to each, which lets it move and unmount as a unit.
#[derive(Clone)]
pub enum WebNode {
    /// A single real DOM node.
    Single(web_sys::Node),
    /// A group of sibling nodes spliced into the parent with no wrapper.
    Fragment(Rc<RefCell<Vec<WebNode>>>),
}

impl WebNode {
    /// The underlying real node, for operations (attributes, listeners, …) that
    /// only ever target a single element. Panics on a fragment, which those
    /// operations never receive.
    fn single(&self) -> &web_sys::Node {
        match self {
            WebNode::Single(node) => node,
            WebNode::Fragment(_) => panic!("expected a single node, found a fragment"),
        }
    }

    /// Collect the real DOM nodes this node represents, in order: a single node
    /// yields itself; a fragment yields its members (recursively).
    fn collect(&self, out: &mut Vec<web_sys::Node>) {
        match self {
            WebNode::Single(node) => out.push(node.clone()),
            WebNode::Fragment(members) => {
                for member in members.borrow().iter() {
                    member.collect(out);
                }
            }
        }
    }

    /// The real DOM nodes this node represents, in order.
    fn real_nodes(&self) -> Vec<web_sys::Node> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }
}

/// Read the system-modifier state (`[ctrl, alt, shift, meta]`) off an event.
/// Keyboard and mouse events both carry these flags; other event types report
/// no modifiers held.
fn modifier_states(event: &web_sys::Event) -> [bool; 4] {
    if let Some(e) = event.dyn_ref::<web_sys::MouseEvent>() {
        [e.ctrl_key(), e.alt_key(), e.shift_key(), e.meta_key()]
    } else if let Some(e) = event.dyn_ref::<web_sys::KeyboardEvent>() {
        [e.ctrl_key(), e.alt_key(), e.shift_key(), e.meta_key()]
    } else {
        [false; 4]
    }
}

/// Read the value an event's target carries back to a `v-model`. A checkbox
/// syncs its boolean `checked` state (sent as `"true"`/`"false"`); a text input,
/// `<textarea>`, or `<select>` syncs its text `value`. Other targets carry none.
fn read_value(target: &web_sys::EventTarget) -> String {
    if let Some(input) = target.dyn_ref::<web_sys::HtmlInputElement>() {
        if input.type_() == "checkbox" {
            input.checked().to_string()
        } else {
            input.value()
        }
    } else if let Some(textarea) = target.dyn_ref::<web_sys::HtmlTextAreaElement>() {
        textarea.value()
    } else if let Some(select) = target.dyn_ref::<web_sys::HtmlSelectElement>() {
        select.value()
    } else {
        String::new()
    }
}

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
    /// run `on_mounted` callbacks. A fragment appends each of its members.
    pub fn mount(&self, node: &WebNode) {
        install_microtask_scheduler();
        let body = document().body().expect("document has no body");
        for real in node.real_nodes() {
            body.append_child(&real).expect("mount append_child");
        }
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
    type Node = WebNode;
    /// The event name, its capture flag (needed to detach), and the live JS
    /// closure; dropping the closure releases it.
    type Listener = (String, bool, Closure<dyn FnMut(web_sys::Event)>);

    fn create_element(&self, tag: &str) -> WebNode {
        WebNode::Single(
            document()
                .create_element(tag)
                .expect("create_element")
                .unchecked_into(),
        )
    }

    fn create_text(&self, data: &str) -> WebNode {
        WebNode::Single(document().create_text_node(data).unchecked_into())
    }

    fn create_anchor(&self) -> WebNode {
        WebNode::Single(document().create_comment("").unchecked_into())
    }

    fn create_fragment(&self, children: Vec<WebNode>) -> WebNode {
        WebNode::Fragment(Rc::new(RefCell::new(children)))
    }

    fn set_text(&self, node: &WebNode, data: &str) {
        node.single().set_text_content(Some(data));
    }

    fn set_attribute(&self, node: &WebNode, name: &str, value: &str) {
        let node = node.single();
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

    fn set_property(&self, node: &WebNode, name: &str, value: &str) {
        // Assign a DOM property (`node[name] = value`) rather than an attribute.
        let _ = js_sys::Reflect::set(
            node.single().as_ref(),
            &JsValue::from_str(name),
            &JsValue::from_str(value),
        );
    }

    fn set_bool_property(&self, node: &WebNode, name: &str, value: bool) {
        // Assign a boolean DOM property (`node[name] = true`), e.g. a checkbox's
        // `checked`. A string would always be truthy, so set an actual bool.
        let _ = js_sys::Reflect::set(
            node.single().as_ref(),
            &JsValue::from_str(name),
            &JsValue::from_bool(value),
        );
    }

    fn remove_attribute(&self, node: &WebNode, name: &str) {
        let element: &web_sys::Element = node.single().unchecked_ref();
        element.remove_attribute(name).expect("remove_attribute");
    }

    fn set_inner_html(&self, node: &WebNode, html: &str) {
        let element: &web_sys::Element = node.single().unchecked_ref();
        element.set_inner_html(html);
    }

    fn append_child(&self, parent: &WebNode, child: &WebNode) {
        let parent = parent.single();
        for real in child.real_nodes() {
            parent.append_child(&real).expect("append_child");
        }
    }

    fn insert_before(&self, parent: &WebNode, child: &WebNode, anchor: &WebNode) {
        let parent = parent.single();
        let anchor = anchor.single();
        for real in child.real_nodes() {
            parent
                .insert_before(&real, Some(anchor))
                .expect("insert_before");
        }
    }

    fn remove_child(&self, parent: &WebNode, child: &WebNode) {
        let parent = parent.single();
        for real in child.real_nodes() {
            parent.remove_child(&real).expect("remove_child");
        }
    }

    fn add_event_listener(
        &self,
        node: &WebNode,
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
            // The leading disjunction skips the event downcast when no
            // system-modifier guard is requested (the common case).
            if (options.ctrl || options.alt || options.shift || options.meta || options.exact)
                && !options.system_modifiers_pass(modifier_states(&event))
            {
                return;
            }
            if options.prevent_default {
                event.prevent_default();
            }
            if options.stop_propagation {
                event.stop_propagation();
            }
            let value = event.target().map(|target| read_value(&target)).unwrap_or_default();
            handler(&value);
        });
        let target: &web_sys::EventTarget = node.single().unchecked_ref();
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

    fn remove_event_listener(&self, node: &WebNode, listener: Self::Listener) {
        let (event, capture, closure) = listener;
        let target: &web_sys::EventTarget = node.single().unchecked_ref();
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
