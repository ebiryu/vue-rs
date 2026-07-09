//! End-to-end: `$name` value sugar in a template compiles, renders on `MockDom`,
//! and stays reactive. `$count` reads the signal (`count.get()`), `$count = v`
//! and `$count += v` write through it (`count.set(...)`), so a counter needs no
//! explicit `.get()`/`.set()` in its template.

use vue_rs_dom::{El, MockDom};
use vue_rs_macro::view;
use vue_rs_reactive::signal;

#[test]
fn read_and_compound_write_sugar_drive_a_counter() {
    let dom = MockDom::new();
    let count = signal(0);

    let node = view!(
        dom.clone(),
        r#"<button @click="$count += 1">count is {{ $count }}</button>"#
    );

    assert_eq!(dom.to_html(node), "<button>count is 0</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>count is 1</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>count is 2</button>");
}

#[test]
fn plain_write_sugar_assigns_through_the_setter() {
    let dom = MockDom::new();
    let count = signal(10);

    let node = view!(
        dom.clone(),
        r#"<button @click="$count = 42">{{ $count }}</button>"#
    );

    assert_eq!(dom.to_html(node), "<button>10</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>42</button>");
}

#[test]
fn read_sugar_projects_fields_of_a_signal() {
    let dom = MockDom::new();
    let point = signal((3, 4));

    // `$point.0` reads the signal, then projects the tuple field.
    let node = view!(dom.clone(), r#"<p>{{ $point.0 }}</p>"#);
    assert_eq!(dom.to_html(node), "<p>3</p>");

    point.set((7, 4));
    assert_eq!(dom.to_html(node), "<p>7</p>");
}
