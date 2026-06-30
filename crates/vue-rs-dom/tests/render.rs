//! Contract for DOM rendering primitives, exercised against the in-memory
//! `MockDom` backend so the reactive wiring is testable without a browser.

use vue_rs_dom::{El, EventOptions, MockDom, MockEvent, RawHtml};
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
fn dyn_attr_named_sets_and_renames_attribute() {
    // The `:[key]` dynamic argument computes the attribute name reactively; when
    // the name changes the old attribute is removed before the new one is set.
    let dom = MockDom::new();
    let attr = signal("id".to_string());
    let value = signal("a".to_string());
    let node = El::new(dom.clone(), "div")
        .dyn_attr_named(move || attr.get(), move || value.get())
        .finish();
    assert_eq!(dom.to_html(node), r#"<div id="a"></div>"#);

    value.set("b".to_string());
    assert_eq!(dom.to_html(node), r#"<div id="b"></div>"#);

    attr.set("title".to_string());
    assert_eq!(
        dom.to_html(node),
        r#"<div title="b"></div>"#,
        "renaming should drop the stale attribute"
    );
}

#[test]
fn dyn_prop_sets_property_reactively() {
    // `:name.prop` sets a DOM property (kept separate from attributes), updating
    // when its reactive deps change.
    let dom = MockDom::new();
    let text = signal("hello".to_string());
    let node = El::new(dom.clone(), "input")
        .dyn_prop("value", move || text.get())
        .finish();
    assert_eq!(dom.property(node, "value").as_deref(), Some("hello"));
    // A property is not serialized as an attribute.
    assert_eq!(dom.to_html(node), "<input></input>");

    text.set("world".to_string());
    assert_eq!(dom.property(node, "value").as_deref(), Some("world"));
}

#[test]
fn on_named_resubscribes_when_event_name_changes() {
    let dom = MockDom::new();
    let event = signal("click".to_string());
    let clicks = signal(0);
    let node = El::new(dom.clone(), "button")
        .on_named(move || event.get(), move || clicks.set(clicks.get() + 1))
        .finish();

    dom.dispatch(node, "click");
    assert_eq!(clicks.get(), 1);

    event.set("dblclick".to_string());
    // The old `click` listener is detached; only `dblclick` fires now.
    dom.dispatch(node, "click");
    assert_eq!(clicks.get(), 1, "old listener should be detached");
    dom.dispatch(node, "dblclick");
    assert_eq!(clicks.get(), 2);
}

#[test]
fn on_named_listener_is_removed_when_owning_scope_is_disposed() {
    let dom = MockDom::new();
    let clicks = signal(0);
    let node = std::cell::Cell::new(0usize);
    let disposer = create_root(|| {
        let n = El::new(dom.clone(), "button")
            .on_named(
                move || "click".to_string(),
                move || clicks.set(clicks.get() + 1),
            )
            .finish();
        node.set(n);
    });
    let node = node.get();

    dom.dispatch(node, "click");
    assert_eq!(clicks.get(), 1);

    disposer.dispose();
    dom.dispatch(node, "click");
    assert_eq!(clicks.get(), 1, "listener should be removed after dispose");
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
fn on_opts_self_only_runs_when_target_is_the_element() {
    let dom = MockDom::new();
    let count = signal(0);
    let node = El::new(dom.clone(), "div")
        .on_opts(
            "click",
            EventOptions {
                self_only: true,
                ..Default::default()
            },
            move || count.set(count.get() + 1),
        )
        .finish();
    // Target is a different node (a descendant): the guard skips the handler.
    let child = El::new(dom.clone(), "span").finish();
    dom.dispatch_event(
        node,
        "click",
        MockEvent {
            target: Some(child),
            ..Default::default()
        },
    );
    assert_eq!(count.get(), 0, "self guard skips events from descendants");
    // Target is the element itself: the handler runs.
    dom.dispatch(node, "click");
    assert_eq!(count.get(), 1);
}

#[test]
fn on_opts_key_filter_runs_only_for_matching_key() {
    let dom = MockDom::new();
    let count = signal(0);
    let node = El::new(dom.clone(), "input")
        .on_opts(
            "keyup",
            EventOptions {
                keys: &["Enter"],
                ..Default::default()
            },
            move || count.set(count.get() + 1),
        )
        .finish();
    dom.dispatch_event(
        node,
        "keyup",
        MockEvent {
            key: Some("a".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(count.get(), 0, "non-matching key is ignored");
    dom.dispatch_event(
        node,
        "keyup",
        MockEvent {
            key: Some("Enter".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(count.get(), 1, "matching key runs the handler");
}

#[test]
fn on_opts_mouse_button_filter_runs_only_for_matching_button() {
    let dom = MockDom::new();
    let count = signal(0);
    let node = El::new(dom.clone(), "button")
        .on_opts(
            "click",
            EventOptions {
                buttons: &[2],
                ..Default::default()
            },
            move || count.set(count.get() + 1),
        )
        .finish();
    dom.dispatch_event(
        node,
        "click",
        MockEvent {
            button: Some(0),
            ..Default::default()
        },
    );
    assert_eq!(count.get(), 0, "left button ignored by right-button filter");
    dom.dispatch_event(
        node,
        "click",
        MockEvent {
            button: Some(2),
            ..Default::default()
        },
    );
    assert_eq!(count.get(), 1, "right button runs the handler");
}

#[test]
fn on_opts_system_modifier_runs_only_when_modifier_key_is_pressed() {
    let dom = MockDom::new();
    let count = signal(0);
    let node = El::new(dom.clone(), "button")
        .on_opts(
            "click",
            EventOptions {
                ctrl: true,
                ..Default::default()
            },
            move || count.set(count.get() + 1),
        )
        .finish();
    // No ctrl held: the guard skips the handler.
    dom.dispatch_event(node, "click", MockEvent::default());
    assert_eq!(count.get(), 0, "plain click ignored when .ctrl is required");
    // ctrl held (plus an irrelevant shift): the handler runs.
    dom.dispatch_event(
        node,
        "click",
        MockEvent {
            ctrl: true,
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(count.get(), 1, "ctrl+click runs the handler");
}

#[test]
fn on_opts_exact_modifier_requires_the_exact_modifier_set() {
    let dom = MockDom::new();
    let count = signal(0);
    let node = El::new(dom.clone(), "button")
        .on_opts(
            "click",
            EventOptions {
                ctrl: true,
                exact: true,
                ..Default::default()
            },
            move || count.set(count.get() + 1),
        )
        .finish();
    // ctrl + an extra shift: exact rejects the surplus modifier.
    dom.dispatch_event(
        node,
        "click",
        MockEvent {
            ctrl: true,
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(count.get(), 0, "exact rejects extra modifiers");
    // exactly ctrl and nothing else: the handler runs.
    dom.dispatch_event(
        node,
        "click",
        MockEvent {
            ctrl: true,
            ..Default::default()
        },
    );
    assert_eq!(count.get(), 1, "exact ctrl-only runs the handler");
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
