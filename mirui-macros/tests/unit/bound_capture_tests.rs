use super::FreeCaptureUse;
use proc_macro2::Span;

#[test]
fn format_literals_only_capture_free_named_references() {
    let name = syn::Ident::new("counter", Span::call_site());
    assert!(FreeCaptureUse::contains(
        &syn::parse_quote!(format!("{counter:>4}")),
        &name
    ));
    assert!(FreeCaptureUse::contains(
        &syn::parse_quote!(::std::format_args!("{counter}")),
        &name
    ));
    assert!(FreeCaptureUse::contains(
        &syn::parse_quote!(format!("{item:>counter$}", item = "x")),
        &name
    ));
    assert!(!FreeCaptureUse::contains(
        &syn::parse_quote!(format!("{item:>mycounter$}", item = "x")),
        &name
    ));
    assert!(!FreeCaptureUse::contains(
        &syn::parse_quote!(format!("{{counter}} {value}", value = 1)),
        &name
    ));
    assert!(!FreeCaptureUse::contains(
        &syn::parse_quote!(format!("{counter}", counter = 7)),
        &name
    ));
    assert!(!FreeCaptureUse::contains(
        &syn::parse_quote!(custom::format!("{counter}")),
        &name
    ));
    assert!(!FreeCaptureUse::contains(
        &syn::parse_quote!(std::format!("{counter}")),
        &name
    ));
    assert!(!FreeCaptureUse::contains(
        &syn::parse_quote!({
            let counter = 7;
            format!("{counter}")
        }),
        &name
    ));
}
