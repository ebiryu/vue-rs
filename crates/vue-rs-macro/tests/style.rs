//! End-to-end: a `<style scoped>` block marks the component's elements with a
//! `data-v-<id>` attribute, and the exposed CSS const targets that same id.

use vue_rs_dom::MockDom;
use vue_rs_macro::component;

component!(Boxed, "tests/fixtures/boxed.vrs");

#[test]
fn scoped_style_marks_elements_and_rewrites_css() {
    let dom = MockDom::new();
    let html = dom.to_html(Boxed(dom.clone()));

    // Both elements carry the same data-v marker.
    let id: String = html
        .split("data-v-")
        .nth(1)
        .expect("a data-v marker")
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    assert!(!id.is_empty());
    assert_eq!(html.matches(&format!("data-v-{id}")).count(), 2);

    // The exposed CSS targets the same id.
    assert!(BOXED_STYLE.contains(&format!(".box[data-v-{id}]")));
    assert!(BOXED_STYLE.contains(&format!("p[data-v-{id}]")));
}
