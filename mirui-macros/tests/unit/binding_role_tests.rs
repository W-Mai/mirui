#[test]
fn responsive_visibility_uses_the_retained_visibility_property() {
    for widget in ["Row", "Column", "Text", "View"] {
        assert_eq!(
            super::responsive_property(widget, "visible"),
            Some("Visible")
        );
    }
}

#[test]
fn lifecycle_properties_never_use_the_display_gate() {
    for property in [
        "Visible",
        "Page",
        "PageTransition",
        "LazyListItems",
        "LazyListExtent",
    ] {
        let role = super::binding_role(&syn::Ident::new(property, proc_macro2::Span::call_site()));
        assert!(role.to_string().ends_with("Maintenance"));
    }
    for (property, expected) in [
        ("TextContent", "Display"),
        ("ImageSource", "Display"),
        ("Width", "Display"),
        ("ScrollOffset", "Display"),
        ("CustomProperty", "Ordinary"),
    ] {
        let role = super::binding_role(&syn::Ident::new(property, proc_macro2::Span::call_site()));
        assert!(role.to_string().ends_with(expected));
    }
}

#[test]
fn static_attached_properties_do_not_install_reactive_bindings() {
    let attrs = [syn::parse_quote!(scroll_offset: ScrollOffset::default())];
    let parsed = super::MiruiRune::new().parse_attrs(&attrs, "Row", super::WidgetKind::Layout);
    assert!(parsed.errors.is_empty());
    assert_eq!(parsed.property_values.len(), 1);
    assert!(parsed.reactive_binds.is_empty());
    assert!(parsed.builder_calls.is_empty());
}
