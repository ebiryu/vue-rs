//! Validation for a component's `NameProps` struct.

use crate::CompileError;

/// Reject props whose top-level type is a *writable* reactive handle
/// (`Signal`/`WritableMemo`), enforcing one-way data flow: a child observes
/// props but cannot mutate parent state through them.
///
/// The parent already converts reactive values to a read-only `ReadSignal` when
/// passing them (`Into::into`), but std's reflexive `From<T> for T` means a
/// child that *declares* a writable field would receive the writable handle
/// unchanged. This check closes that loophole at the declaration site: props
/// must be `ReadSignal`/`Memo` (read-only) or plain values, and updates flow
/// back up through emits (`Callback`).
///
/// Only the top-level field type is checked; a composite prop that carries
/// signals inside (e.g. a list of rows each owning its own signals) is a
/// deliberate pattern and is allowed.
pub fn check_prop_fields(props: &syn::ItemStruct) -> Result<(), CompileError> {
    let syn::Fields::Named(fields) = &props.fields else {
        return Ok(());
    };
    for field in &fields.named {
        if let Some(handle) = writable_handle_name(&field.ty) {
            let name = field
                .ident
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default();
            return Err(CompileError(format!(
                "prop `{name}` has writable reactive type `{handle}<…>`; props are read-only — \
                 declare it `ReadSignal<…>` (or `Memo<…>`) and send updates back to the parent \
                 via an emit callback"
            )));
        }
    }
    Ok(())
}

/// If `ty` is a path ending in a writable handle (`Signal`/`WritableMemo`),
/// return that handle's name.
fn writable_handle_name(ty: &syn::Type) -> Option<&'static str> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    match path.path.segments.last()?.ident.to_string().as_str() {
        "Signal" => Some("Signal"),
        "WritableMemo" => Some("WritableMemo"),
        _ => None,
    }
}
