//! End-to-end: a template with multiple roots compiles to a fragment that
//! renders every root with no wrapper element and stays reactive, on `MockDom`.

use vue_rs_dom::{Backend, El, MockDom};
use vue_rs_macro::view;
use vue_rs_reactive::signal;

mod sfc {
    // `component!` lifts the fixture's `use vue_rs_reactive::signal;` to module
    // level, so the component lives in its own module to avoid clashing with the
    // file's own `use signal`.
    use vue_rs_macro::component;
    component!(multi_root, "tests/fixtures/multi_root.vrs");
}
use sfc::multi_root;

#[test]
fn sfc_multiple_roots_render_without_a_wrapper() {
    let dom = MockDom::new();
    let node = multi_root(dom.clone(), Default::default());

    // Mounting the fragment into a host splices both roots in, no wrapper.
    let host = El::new(dom.clone(), "main").child(node).finish();
    assert_eq!(
        dom.to_html(host),
        "<main><h1>Hello</h1><button>count is 0</button></main>"
    );
}

#[test]
fn sfc_fragment_members_stay_reactive() {
    let dom = MockDom::new();
    let node = multi_root(dom.clone(), Default::default());
    let host = El::new(dom.clone(), "main").child(node).finish();

    let button = dom.find("button").expect("button exists");
    dom.dispatch(button, "click");
    assert_eq!(
        dom.to_html(host),
        "<main><h1>Hello</h1><button>count is 1</button></main>"
    );
}

#[test]
fn view_multiple_roots_compile_to_a_fragment() {
    let dom = MockDom::new();
    let count = signal(0);

    let frag = view!(
        dom.clone(),
        r#"<span>label</span><button @click="count.set(count.get() + 1)">{{ count.get() }}</button>"#
    );
    let host = El::new(dom.clone(), "div").child(frag).finish();

    assert_eq!(dom.to_html(host), "<div><span>label</span><button>0</button></div>");
    let button = dom.find("button").expect("button exists");
    dom.dispatch(button, "click");
    assert_eq!(dom.to_html(host), "<div><span>label</span><button>1</button></div>");
}

#[test]
fn fragment_as_a_conditional_branch_unmounts_as_a_unit() {
    // A multi-root fragment used as a `dyn_if` branch: removing it must take all
    // members out together (exercises `remove_child` against a fragment branch).
    let dom = MockDom::new();
    let show = signal(true);

    let host = El::new(dom.clone(), "div")
        .dyn_if(
            move || show.get(),
            move |b| {
                let a = El::new(b.clone(), "a").finish();
                let bb = El::new(b.clone(), "b").finish();
                b.create_fragment(vec![a, bb])
            },
        )
        .finish();

    assert_eq!(dom.to_html(host), "<div><a></a><b></b></div>");
    show.set(false);
    assert_eq!(dom.to_html(host), "<div></div>");
    show.set(true);
    assert_eq!(dom.to_html(host), "<div><a></a><b></b></div>");
}
