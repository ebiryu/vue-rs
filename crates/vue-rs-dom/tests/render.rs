//! Contract for DOM rendering primitives, exercised against the in-memory
//! `MockDom` backend so the reactive wiring is testable without a browser.

use vue_rs_dom::{El, EventOptions, MockDom, RawHtml};
use vue_rs_reactive::{create_root, signal};

#[test]
fn renders_static_element_with_text() {
    let dom = MockDom::new();
    let node = El::new(dom.clone(), "button").text("count is 0").finish();
    assert_eq!(dom.to_html(node), "<button>count is 0</button>");
}

#[test]
fn renders_static_attribute() {
    let dom = MockDom::new();
    let node = El::new(dom.clone(), "div")
        .attr("id", "app")
        .attr("class", "root")
        .finish();
    assert_eq!(dom.to_html(node), r#"<div id="app" class="root"></div>"#);
}

#[test]
fn nests_child_elements() {
    let dom = MockDom::new();
    let li = El::new(dom.clone(), "li").text("item").finish();
    let ul = El::new(dom.clone(), "ul").child(li).finish();
    assert_eq!(dom.to_html(ul), "<ul><li>item</li></ul>");
}

#[test]
fn dyn_text_updates_when_signal_changes() {
    let dom = MockDom::new();
    let count = signal(0);
    let node = El::new(dom.clone(), "p")
        .dyn_text(move || count.get().to_string())
        .finish();
    assert_eq!(dom.to_html(node), "<p>0</p>");
    count.set(5);
    assert_eq!(dom.to_html(node), "<p>5</p>");
}

#[test]
fn dyn_attr_updates_when_signal_changes() {
    let dom = MockDom::new();
    let active = signal(false);
    let node = El::new(dom.clone(), "div")
        .dyn_attr("class", move || {
            if active.get() { "on".into() } else { "off".into() }
        })
        .finish();
    assert_eq!(dom.to_html(node), r#"<div class="off"></div>"#);
    active.set(true);
    assert_eq!(dom.to_html(node), r#"<div class="on"></div>"#);
}

#[test]
fn dyn_inner_html_sets_raw_markup_and_updates() {
    let dom = MockDom::new();
    let html = signal("<b>hi</b>".to_string());
    let node = El::new(dom.clone(), "div")
        // Constructing `RawHtml` is the explicit opt-in to unescaped insertion.
        .dyn_inner_html(move || RawHtml::dangerously_from_html(html.get()))
        .finish();
    // Inner HTML is inserted raw (not escaped) and replaces any children.
    assert_eq!(dom.to_html(node), "<div><b>hi</b></div>");
    html.set("<i>bye</i>".to_string());
    assert_eq!(dom.to_html(node), "<div><i>bye</i></div>");
}

#[test]
fn event_handler_fires_and_drives_reactivity() {
    let dom = MockDom::new();
    let count = signal(0);
    let node = El::new(dom.clone(), "button")
        .on("click", move || count.set(count.get() + 1))
        .dyn_text(move || count.get().to_string())
        .finish();
    assert_eq!(dom.to_html(node), "<button>0</button>");
    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>1</button>");
    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>2</button>");
}

#[test]
fn event_listener_is_removed_when_owning_scope_is_disposed() {
    let dom = MockDom::new();
    let count = signal(0);
    let node = std::cell::Cell::new(0usize);
    let disposer = create_root(|| {
        let n = El::new(dom.clone(), "button")
            .on("click", move || count.set(count.get() + 1))
            .finish();
        node.set(n);
    });
    let node = node.get();

    dom.dispatch(node, "click");
    assert_eq!(count.get(), 1);

    disposer.dispose();
    // The listener was registered inside the scope, so disposing the scope must
    // detach it: further dispatches do nothing.
    dom.dispatch(node, "click");
    assert_eq!(count.get(), 1, "listener should be removed after dispose");
}

#[test]
fn on_opts_prevent_default_is_reported_by_dispatch() {
    let dom = MockDom::new();
    let count = signal(0);
    let node = El::new(dom.clone(), "a")
        .on_opts(
            "click",
            EventOptions {
                prevent_default: true,
                ..Default::default()
            },
            move || count.set(count.get() + 1),
        )
        .finish();
    let outcome = dom.dispatch(node, "click");
    assert_eq!(count.get(), 1, "handler still runs");
    assert!(outcome.default_prevented, "prevent_default requested");
    assert!(!outcome.propagation_stopped);
}

#[test]
fn on_opts_stop_propagation_is_reported_by_dispatch() {
    let dom = MockDom::new();
    let node = El::new(dom.clone(), "button")
        .on_opts(
            "click",
            EventOptions {
                stop_propagation: true,
                ..Default::default()
            },
            || {},
        )
        .finish();
    let outcome = dom.dispatch(node, "click");
    assert!(outcome.propagation_stopped, "stop_propagation requested");
    assert!(!outcome.default_prevented);
}

#[test]
fn on_opts_once_runs_handler_only_once() {
    let dom = MockDom::new();
    let count = signal(0);
    let node = El::new(dom.clone(), "button")
        .on_opts(
            "click",
            EventOptions {
                once: true,
                ..Default::default()
            },
            move || count.set(count.get() + 1),
        )
        .finish();
    dom.dispatch(node, "click");
    dom.dispatch(node, "click");
    assert_eq!(count.get(), 1, "once listener fires a single time");
}

#[test]
fn plain_on_reports_no_modifiers() {
    let dom = MockDom::new();
    let node = El::new(dom.clone(), "button").on("click", || {}).finish();
    let outcome = dom.dispatch(node, "click");
    assert!(!outcome.default_prevented);
    assert!(!outcome.propagation_stopped);
}

#[test]
fn to_html_escapes_text_content() {
    let dom = MockDom::new();
    let node = El::new(dom.clone(), "p")
        .text("a < b && c > d")
        .finish();
    assert_eq!(dom.to_html(node), "<p>a &lt; b &amp;&amp; c &gt; d</p>");
}

#[test]
fn to_html_escapes_attribute_values() {
    let dom = MockDom::new();
    let node = El::new(dom.clone(), "div")
        .attr("title", r#"a "quote" & <tag>"#)
        .finish();
    assert_eq!(
        dom.to_html(node),
        r#"<div title="a &quot;quote&quot; &amp; <tag>"></div>"#
    );
}
