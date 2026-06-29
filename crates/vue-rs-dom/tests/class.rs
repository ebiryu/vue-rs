//! Contract: `ClassList` joins class-name fragments into a single
//! space-separated `class` string, skipping empties and conditional entries.

use vue_rs_dom::ClassList;

#[test]
fn push_joins_non_empty_with_single_spaces() {
    let s = ClassList::new().push("a").push("b").push("c").finish();
    assert_eq!(s, "a b c");
}

#[test]
fn push_skips_empty_fragments() {
    let s = ClassList::new().push("").push("a").push("").push("b").finish();
    assert_eq!(s, "a b");
}

#[test]
fn push_if_includes_only_when_true() {
    let s = ClassList::new()
        .push("base")
        .push_if("active", true)
        .push_if("error", false)
        .finish();
    assert_eq!(s, "base active");
}

#[test]
fn push_accepts_str_and_string() {
    let owned = String::from("b");
    let s = ClassList::new().push("a").push(&owned).push(owned.clone()).finish();
    assert_eq!(s, "a b b");
}

#[test]
fn empty_builder_yields_empty_string() {
    assert_eq!(ClassList::new().finish(), "");
}
