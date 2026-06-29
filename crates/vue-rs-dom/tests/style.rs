//! Contract: `StyleList` joins CSS declarations into a single `;`-separated
//! `style` string, skipping empty fragments and empty-valued properties.

use vue_rs_dom::StyleList;

#[test]
fn push_joins_declarations_with_semicolons() {
    let s = StyleList::new()
        .push("color: red")
        .push("font-size: 14px")
        .finish();
    assert_eq!(s, "color: red; font-size: 14px");
}

#[test]
fn push_skips_empty_fragments() {
    let s = StyleList::new()
        .push("")
        .push("color: red")
        .push("")
        .finish();
    assert_eq!(s, "color: red");
}

#[test]
fn push_trims_trailing_semicolon() {
    let s = StyleList::new().push("color: red;").push("margin: 0;").finish();
    assert_eq!(s, "color: red; margin: 0");
}

#[test]
fn push_prop_builds_declaration() {
    let s = StyleList::new()
        .push_prop("color", "red")
        .push_prop("font-size", "14px")
        .finish();
    assert_eq!(s, "color: red; font-size: 14px");
}

#[test]
fn push_prop_skips_empty_value() {
    let s = StyleList::new()
        .push_prop("color", "")
        .push_prop("margin", "0")
        .finish();
    assert_eq!(s, "margin: 0");
}

#[test]
fn push_accepts_str_and_string() {
    let owned = String::from("margin: 0");
    let s = StyleList::new()
        .push("color: red")
        .push(&owned)
        .push(owned.clone())
        .finish();
    assert_eq!(s, "color: red; margin: 0; margin: 0");
}

#[test]
fn empty_builder_yields_empty_string() {
    assert_eq!(StyleList::new().finish(), "");
}
