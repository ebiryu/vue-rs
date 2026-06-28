//! The SFC authoring layer lets `<script>` use Vue spellings that map onto the
//! reactive core. `ref(...)` is the headline case: `ref` is a Rust keyword, so
//! the compiler rewrites the constructor call (and its import path) to the
//! keyword-safe `signal`, while leaving genuine `ref` binding patterns intact.

use quote::quote;
use vue_rs_compiler::rewrite_script_sugar;

#[test]
fn ref_call_maps_to_signal() {
    let out = rewrite_script_sugar("let count = ref(0);").unwrap();
    assert_eq!(out.to_string(), quote! { let count = signal(0); }.to_string());
}

#[test]
fn ref_import_path_maps_to_signal() {
    let out = rewrite_script_sugar("use vue_rs_reactive::ref;").unwrap();
    assert_eq!(
        out.to_string(),
        quote! { use vue_rs_reactive::signal; }.to_string()
    );
}

#[test]
fn ref_binding_pattern_is_preserved() {
    let out = rewrite_script_sugar("let ref x = y;").unwrap();
    assert_eq!(out.to_string(), quote! { let ref x = y; }.to_string());
}

#[test]
fn ref_pattern_in_match_arm_is_preserved() {
    let out =
        rewrite_script_sugar("let n = match opt { Some(ref v) => v, None => &0 };").unwrap();
    assert_eq!(
        out.to_string(),
        quote! { let n = match opt { Some(ref v) => v, None => &0 }; }.to_string()
    );
}

#[test]
fn ref_mut_binding_is_preserved() {
    let out = rewrite_script_sugar("let ref mut x = y;").unwrap();
    assert_eq!(out.to_string(), quote! { let ref mut x = y; }.to_string());
}

#[test]
fn ref_call_inside_closure_is_mapped() {
    let out = rewrite_script_sugar("let f = move || { let c = ref(1); };").unwrap();
    assert_eq!(
        out.to_string(),
        quote! { let f = move || { let c = signal(1); }; }.to_string()
    );
}

#[test]
fn watch_effect_call_maps_to_effect() {
    let out = rewrite_script_sugar("watchEffect(move || { count.get(); });").unwrap();
    assert_eq!(
        out.to_string(),
        quote! { effect(move || { count.get(); }); }.to_string()
    );
}

#[test]
fn watch_effect_import_path_maps_to_effect() {
    let out = rewrite_script_sugar("use vue_rs_reactive::watchEffect;").unwrap();
    assert_eq!(
        out.to_string(),
        quote! { use vue_rs_reactive::effect; }.to_string()
    );
}

#[test]
fn watch_effect_as_bare_identifier_is_preserved() {
    let out = rewrite_script_sugar("let watchEffect = 1;").unwrap();
    assert_eq!(
        out.to_string(),
        quote! { let watchEffect = 1; }.to_string()
    );
}
