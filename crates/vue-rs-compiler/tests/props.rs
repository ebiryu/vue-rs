//! Contract: props are read-only. A component's `NameProps` struct may not
//! declare a field with a *writable* reactive type (`Signal`/`WritableMemo`),
//! so a child cannot mutate parent state through a prop. Read-only handles
//! (`ReadSignal`/`Memo`) and plain values are fine; updates go up via emits.

use vue_rs_compiler::check_prop_fields;

fn item(src: &str) -> syn::ItemStruct {
    syn::parse_str(src).expect("valid struct")
}

#[test]
fn read_only_and_plain_prop_fields_are_allowed() {
    let s = item(
        "struct P { pub value: ReadSignal<i32>, pub label: String, pub on_change: Callback<i32>, pub total: Memo<i32> }",
    );
    assert!(check_prop_fields(&s).is_ok());
}

#[test]
fn maybe_signal_prop_field_is_allowed() {
    // `MaybeSignal<T>` is a read-only prop value (static value or reactive
    // source, no `set`), so it is accepted like `ReadSignal`/`Memo`.
    let s = item("struct P { pub label: MaybeSignal<String>, pub count: MaybeSignal<i32> }");
    assert!(check_prop_fields(&s).is_ok());
}

#[test]
fn signal_prop_field_is_rejected() {
    let s = item("struct P { pub value: Signal<i32> }");
    let err = check_prop_fields(&s).unwrap_err().to_string();
    assert!(err.contains("value"), "should name the field: {err}");
    assert!(err.contains("read-only"), "should explain why: {err}");
}

#[test]
fn writable_memo_prop_field_is_rejected() {
    let s = item("struct P { pub value: WritableMemo<i32> }");
    assert!(check_prop_fields(&s).is_err());
}

#[test]
fn path_qualified_signal_prop_field_is_rejected() {
    let s = item("struct P { pub value: vue_rs_reactive::Signal<i32> }");
    assert!(check_prop_fields(&s).is_err());
}

#[test]
fn signal_nested_inside_another_type_is_allowed() {
    // An opaque user-defined type's own fields are invisible from here (no
    // whole-program type info), so a composite that happens to carry signals
    // inside its *own* definition (e.g. a row struct) is a deliberate pattern
    // and cannot be checked.
    let s = item("struct P { pub rows: Vec<Row> }");
    assert!(check_prop_fields(&s).is_ok());
}

#[test]
fn signal_inside_vec_prop_field_is_rejected() {
    // Unlike an opaque type, `Vec<Signal<i32>>` spells the writable handle
    // out directly in the prop's own type, so it must be caught.
    let s = item("struct P { pub rows: Vec<Signal<i32>> }");
    let err = check_prop_fields(&s).unwrap_err().to_string();
    assert!(err.contains("rows"), "should name the field: {err}");
}

#[test]
fn writable_memo_inside_option_prop_field_is_rejected() {
    let s = item("struct P { pub value: Option<WritableMemo<i32>> }");
    assert!(check_prop_fields(&s).is_err());
}

#[test]
fn signal_inside_tuple_prop_field_is_rejected() {
    let s = item("struct P { pub pair: (Signal<i32>, String) }");
    assert!(check_prop_fields(&s).is_err());
}

#[test]
fn signal_doubly_nested_in_generics_is_rejected() {
    let s = item("struct P { pub rows: Vec<Option<Signal<i32>>> }");
    assert!(check_prop_fields(&s).is_err());
}
