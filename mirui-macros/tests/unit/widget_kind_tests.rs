use super::*;
use proc_macro2::Span;

fn id(s: &str) -> syn::Ident {
    syn::Ident::new(s, Span::call_site())
}

#[test]
fn lowercase_is_illegal() {
    assert_eq!(
        classify_widget_name(&id("button")),
        WidgetKind::IllegalLowercase
    );
    assert_eq!(
        classify_widget_name(&id("dark_btn")),
        WidgetKind::IllegalLowercase
    );
}

#[test]
fn reserved_names_are_layout() {
    assert_eq!(classify_widget_name(&id("View")), WidgetKind::Layout);
    assert_eq!(classify_widget_name(&id("Row")), WidgetKind::Layout);
    assert_eq!(classify_widget_name(&id("Column")), WidgetKind::Layout);
    assert_eq!(classify_widget_name(&id("Scroll")), WidgetKind::Layout);
}

#[test]
fn other_capital_names_are_component() {
    assert_eq!(classify_widget_name(&id("Button")), WidgetKind::Component);
    assert_eq!(classify_widget_name(&id("MyCard")), WidgetKind::Component);
    assert_eq!(classify_widget_name(&id("Checkbox")), WidgetKind::Component);
}
