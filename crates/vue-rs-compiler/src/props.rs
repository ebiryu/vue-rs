//! Validation for a component's `NameProps` struct.

use crate::CompileError;

/// Reject props whose type spells out a *writable* reactive handle
/// (`Signal`/`WritableMemo`) anywhere in its own type expression, enforcing
/// one-way data flow: a child observes props but cannot mutate parent state
/// through them.
///
/// The parent already converts reactive values to a read-only `ReadSignal` when
/// passing them (`Into::into`), but std's reflexive `From<T> for T` means a
/// child that *declares* a writable field would receive the writable handle
/// unchanged. This check closes that loophole at the declaration site: props
/// must be `ReadSignal`/`Memo` (read-only) or plain values, and updates flow
/// back up through emits (`Callback`).
///
/// The check walks generic arguments, tuples, arrays/slices and references
/// (e.g. `Vec<Signal<i32>>`, `Option<WritableMemo<i32>>`, `(Signal<i32>, …)`),
/// so a writable handle can't be smuggled in through a visible container type.
/// It cannot see inside an opaque user-defined type's own fields (no
/// whole-program type info at this stage), so a composite prop whose *own*
/// definition carries signals (e.g. a list of rows each owning its own
/// signal) is a deliberate pattern and stays allowed.
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

/// If a writable handle (`Signal`/`WritableMemo`) appears anywhere in `ty`'s
/// own type expression, return that handle's name.
fn writable_handle_name(ty: &syn::Type) -> Option<&'static str> {
    match ty {
        syn::Type::Path(path) => {
            let seg = path.path.segments.last()?;
            match seg.ident.to_string().as_str() {
                "Signal" => return Some("Signal"),
                "WritableMemo" => return Some("WritableMemo"),
                _ => {}
            }
            let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
                return None;
            };
            args.args.iter().find_map(|arg| match arg {
                syn::GenericArgument::Type(inner) => writable_handle_name(inner),
                _ => None,
            })
        }
        syn::Type::Tuple(tuple) => tuple.elems.iter().find_map(writable_handle_name),
        syn::Type::Reference(r) => writable_handle_name(&r.elem),
        syn::Type::Array(a) => writable_handle_name(&a.elem),
        syn::Type::Slice(s) => writable_handle_name(&s.elem),
        syn::Type::Paren(p) => writable_handle_name(&p.elem),
        syn::Type::Group(g) => writable_handle_name(&g.elem),
        _ => None,
    }
}
