extern crate proc_macro;

mod animate_attr;
mod binding;
mod callback_lowering;
mod child_scope;
mod component_attr;
mod compose;
mod compose_attr;
mod diag;
mod event_attr;
mod function_attrs;
mod layout_reactive;
mod vector;
mod view_attr;
mod visit_id;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::parse_macro_input;
use syn::visit_mut::VisitMut;

mod model_attr;
mod system_attr;
mod timer_attr;

/// Declare entity data with optional Model fields and output dependencies.
///
/// `#[depends_on(T)]` on an `Entity` or `Option<Entity>` field requests that source's current output. On the component type it refers to the owner. Type-level `#[retain_previous(T)]` requests owner history. Declarations activate when a View filtered by this component is registered; ordinary fields and component instances retain their authored types.
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    let item = proc_macro2::TokenStream::from(item);
    component_attr::expand(attr, item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
mod mold;

/// Declare registered state, observed values, and forwarded business methods.
///
/// Apply to a struct and its inherent implementation. A `model!(owned Child)` field stores child data constructed with `.into()`; registering the parent also registers its children. The field getter returns the child instance for existing Model roles. Parent and child mutations commit the same tree before delivering effects.
///
/// Mark an extractor with `#[effects]` to return `Option<E>` or `[Option<E>; N]`. Register a consumer with `model.on_effect(|event: &E| { ... })`; each present event is borrowed separately after state publication. Extractors are called by the framework and are not exposed on the registered instance.
#[proc_macro_attribute]
pub fn model(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    let item = proc_macro2::TokenStream::from(item);
    model_attr::expand(attr, item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declare a component-filtered widget with independent behavior and paint hooks.
///
/// A nonempty function body supplies the paint callback. An empty body omits painting while retaining declared systems, attachment, gestures, and model subscriptions. The generated `function_name::view()` factory creates its widget descriptor.
///
/// Read-only `query!` parameters share system query syntax and can fetch other entities. Required items, optional items, and presence filters declare static component dependencies; changes invalidate this widget's instances according to its policy. Queries need a required item or `with` filter as their driver and cannot request mutable component access.
///
/// ```ignore
/// #[view(component = TabContent, name = "TabPages", systems = [update_pages::system()])]
/// fn tab_pages_view() {}
///
/// app.with_widget(tab_pages_view::view());
/// ```
#[proc_macro_attribute]
pub fn view(attr: TokenStream, item: TokenStream) -> TokenStream {
    view_attr::expand(attr.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

struct ComponentRead {
    entity: syn::Expr,
    component: syn::Type,
}

impl Parse for ComponentRead {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let entity = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let component = input.parse()?;
        Ok(Self { entity, component })
    }
}

/// Read one component from the active `#[compose]` context.
/// The read does not subscribe to later changes.
#[proc_macro]
pub fn com(input: TokenStream) -> TokenStream {
    let ComponentRead { entity, component } = parse_macro_input!(input as ComponentRead);
    quote! { cx.component::<#component>(#entity) }.into()
}

use xrune::ds_node::ds_attr::DsAttr;
use xrune::ds_node::{DsTree, DsTreeRef};
use xrune::ds_rune::DsRune;
use xrune::ds_rune::decipher::decipher;

const LAYOUT_ATTRS: &[&str] = &[
    "width",
    "min_width",
    "max_width",
    "height",
    "min_height",
    "max_height",
    "grow",
    "shrink",
    "direction",
    "wrap",
    "justify",
    "align",
    "padding",
    "gap",
    "row_gap",
    "column_gap",
    "position",
    "left",
    "top",
];

const STYLE_ATTRS: &[&str] = &[
    "bg_color",
    "text_color",
    "border_color",
    "border_radius",
    "border_width",
    "clip_children",
    "font",
    "font_stack",
    "font_size",
];

const RESERVED_LAYOUT_NAMES: &[&str] = &["View", "Row", "Column", "Scroll"];

fn primary_attr_for(widget_name: &str) -> Option<&'static str> {
    match widget_name {
        "Text" | "Button" => Some("text"),
        "Image" => Some("src"),
        "MirrorOf" => Some("source"),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum WidgetKind {
    IllegalLowercase,
    Layout,
    Component,
}

fn classify_widget_name(name: &syn::Ident) -> WidgetKind {
    let s = name.to_string();
    let first = s.chars().next().unwrap_or('_');
    if first.is_ascii_lowercase() {
        WidgetKind::IllegalLowercase
    } else if RESERVED_LAYOUT_NAMES.contains(&s.as_str()) {
        WidgetKind::Layout
    } else {
        WidgetKind::Component
    }
}

#[allow(clippy::large_enum_variant)]
enum Cmd {
    Widget(WidgetCmd),
    Compose(ComposeCmd),
    Iter(IterCmd),
    If(IfCmd),
    Niche(NicheCmd),
    Match(MatchCmd),
    CodeBlock(proc_macro2::TokenStream),
}

struct ComposeCmd {
    function: syn::Ident,
    var: syn::Ident,
    args: Vec<syn::Expr>,
    errors: Vec<proc_macro2::TokenStream>,
    id_lookups: Vec<(syn::Ident, String)>,
}

struct OnCmd {
    qualifier: Option<syn::Ident>,
    name: syn::Ident,
    options: Vec<syn::Expr>,
    callback: syn::Expr,
}

struct NicheCmd {
    name: syn::Ident,
    is_declaration: bool,
    mold_slot_var: Option<syn::Ident>,
    parent_widget: Option<syn::Ident>,
    body: Vec<Cmd>,
}

struct MatchCmd {
    scrutinee: syn::Expr,
    reactive: bool,
    arms: Vec<MatchArm>,
}

struct MatchArm {
    pat: syn::Pat,
    body: Vec<Cmd>,
}

// A `!signal` / `!{ expr }` attribute: re-applied via a per-widget effect.
struct ReactiveBind {
    property: syn::Ident,
    expr: proc_macro2::TokenStream,
    format_expr: syn::Expr,
}

fn reactive_read(value: &syn::Expr) -> proc_macro2::TokenStream {
    match value {
        syn::Expr::Path(p) => quote! { #p.get() },
        syn::Expr::Block(b) => quote! { #b },
        other => quote! { #other },
    }
}

fn direct_format_args_call(
    value: &syn::Expr,
    entity: &proc_macro2::TokenStream,
    capacity: &proc_macro2::TokenStream,
) -> Option<proc_macro2::TokenStream> {
    match value {
        syn::Expr::Block(block) => {
            let (last, prefix) = block.block.stmts.split_last()?;
            let syn::Stmt::Expr(value, None) = last else {
                return None;
            };
            let call = direct_format_args_call(value, entity, capacity)?;
            Some(quote! {{ #(#prefix)* #call }})
        }
        syn::Expr::Paren(value) => direct_format_args_call(&value.expr, entity, capacity),
        syn::Expr::Group(value) => direct_format_args_call(&value.expr, entity, capacity),
        syn::Expr::Macro(value)
            if match value.mac.path.segments.len() {
                1 => value.mac.path.segments[0].ident == "format_args",
                2 => {
                    matches!(
                        value.mac.path.segments[0].ident.to_string().as_str(),
                        "core" | "std" | "alloc"
                    ) && value.mac.path.segments[1].ident == "format_args"
                }
                _ => false,
            } =>
        {
            Some(quote! {
                mirui::ui::property::apply_bounded_text::<{ #capacity }>(#entity, #value);
            })
        }
        _ => None,
    }
}

fn reactive_property(widget: &str, attr: &str) -> Option<&'static str> {
    match attr {
        "text" => Some("TextContent"),
        "visible" => Some("Visible"),
        "bg_color" => Some("BackgroundColor"),
        "text_color" => Some("TextColor"),
        "border_color" => Some("BorderColor"),
        "border_width" => Some("BorderWidth"),
        "normal_color" if widget == "Button" => Some("ButtonNormalColor"),
        "render_key" => Some("RenderKey"),
        "transform" => Some("Transform"),
        "font_size" => Some("FontSize"),
        "direction" => Some("Direction"),
        "width" => Some("Width"),
        "height" => Some("Height"),
        "left" => Some("Left"),
        "top" => Some("Top"),
        "paragraph" if widget == "Text" => Some("Paragraph"),
        "path" if widget == "Text" => Some("TextPath"),
        "value" if widget == "ProgressBar" => Some("ProgressValue"),
        "value" if widget == "Slider" => Some("SliderValue"),
        "item_count" if widget == "LazyList" => Some("LazyListCount"),
        "item_extent" if widget == "LazyList" => Some("LazyListExtent"),
        "on" if widget == "Switch" => Some("SwitchOn"),
        "checked" if widget == "Checkbox" => Some("CheckboxChecked"),
        _ => None,
    }
}

fn responsive_property(widget: &str, attr: &str) -> Option<&'static str> {
    match attr {
        "bg_color" => Some("BackgroundColor"),
        "text_color" => Some("TextColor"),
        "normal_color" if widget == "Button" => Some("ButtonNormalColor"),
        "font_size" => Some("FontSize"),
        "direction" => Some("Direction"),
        "paragraph" if widget == "Text" => Some("Paragraph"),
        "width" => Some("Width"),
        "min_width" => Some("MinWidth"),
        "max_width" => Some("MaxWidth"),
        "height" => Some("Height"),
        "min_height" => Some("MinHeight"),
        "max_height" => Some("MaxHeight"),
        "padding" => Some("Padding"),
        "row_gap" => Some("RowGap"),
        "column_gap" => Some("ColumnGap"),
        "left" => Some("Left"),
        "top" => Some("Top"),
        _ => None,
    }
}

struct LayoutBind {
    property: syn::Ident,
    value: crate::layout_reactive::LayoutValue,
}

struct WidgetCmd {
    name: syn::Ident,
    kind: WidgetKind,
    var: syn::Ident,
    attrs: Vec<proc_macro2::TokenStream>,
    layout_fields: Vec<proc_macro2::TokenStream>,
    errors: Vec<proc_macro2::TokenStream>,
    enchants: Vec<proc_macro2::TokenStream>,
    on_handlers: Vec<OnCmd>,
    component_fields: Vec<proc_macro2::TokenStream>,
    text_tuple_value: Option<proc_macro2::TokenStream>,
    text_capacity: Option<proc_macro2::TokenStream>,
    text_paragraph_value: Option<proc_macro2::TokenStream>,
    id: Option<syn::LitStr>,
    id_lookups: Vec<(syn::Ident, String)>,
    reactive_binds: Vec<ReactiveBind>,
    layout_binds: Vec<LayoutBind>,
    children: Vec<Cmd>,
}

struct ParsedAttrs {
    builder_calls: Vec<proc_macro2::TokenStream>,
    layout_fields: Vec<proc_macro2::TokenStream>,
    errors: Vec<proc_macro2::TokenStream>,
    component_inserts: Vec<proc_macro2::TokenStream>,
    component_fields: Vec<proc_macro2::TokenStream>,
    text_tuple_value: Option<proc_macro2::TokenStream>,
    text_capacity: Option<proc_macro2::TokenStream>,
    text_paragraph_value: Option<proc_macro2::TokenStream>,
    id: Option<syn::LitStr>,
    id_lookups: Vec<(syn::Ident, String)>,
    reactive_binds: Vec<ReactiveBind>,
    layout_binds: Vec<LayoutBind>,
    user_set_direction: bool,
}

struct IterCmd {
    iterable: proc_macro2::TokenStream,
    variable: syn::Ident,
    body: Vec<Cmd>,
    reactive: bool,
    key: Option<proc_macro2::TokenStream>,
}

struct IfCmd {
    branches: Vec<Branch>,
    reactive: bool,
}

struct Branch {
    cond: Option<syn::Expr>,
    body: Vec<Cmd>,
}

struct MiruiRune {
    world_expr: proc_macro2::TokenStream,
    parent_expr: proc_macro2::TokenStream,
    stack: Vec<Vec<Cmd>>,
    widget_kind_stack: Vec<Option<syn::Ident>>,
    counter: usize,
    mold_mode: bool,
}

impl MiruiRune {
    fn new() -> Self {
        Self {
            world_expr: quote! { __world },
            parent_expr: quote! { __parent },
            stack: vec![Vec::new()],
            widget_kind_stack: Vec::new(),
            counter: 0,
            mold_mode: false,
        }
    }

    fn new_mold(world: proc_macro2::TokenStream, entity: proc_macro2::TokenStream) -> Self {
        Self {
            world_expr: world,
            parent_expr: entity,
            stack: vec![Vec::new()],
            widget_kind_stack: Vec::new(),
            counter: 0,
            mold_mode: true,
        }
    }

    fn next_var(&mut self) -> syn::Ident {
        let name = format!("__w{}", self.counter);
        self.counter += 1;
        syn::Ident::new(&name, proc_macro2::Span::call_site())
    }

    fn parse_attrs(
        &self,
        attrs: &[DsAttr],
        widget_name: &str,
        widget_kind: WidgetKind,
    ) -> ParsedAttrs {
        let mut builder_calls = Vec::new();
        let mut layout_fields = Vec::new();
        let mut errors = Vec::new();
        let mut component_inserts = Vec::new();
        let mut component_fields = Vec::new();
        let mut text_tuple_value: Option<proc_macro2::TokenStream> = None;
        let mut text_capacity: Option<proc_macro2::TokenStream> = None;
        let mut text_paragraph_value: Option<proc_macro2::TokenStream> = None;
        let mut id = None;
        let mut id_lookups: Vec<(syn::Ident, String)> = Vec::new();
        let mut reactive_binds: Vec<ReactiveBind> = Vec::new();
        let mut layout_binds = Vec::new();
        let mut user_set_direction = false;

        let is_text_widget = widget_name == "Text";
        let is_button_widget = widget_name == "Button";
        let is_text_input_widget = widget_name == "TextInput";
        const TEXT_INPUT_FIELDS: &[&str] = &[
            "text_color",
            "placeholder_color",
            "cursor_color",
            "focus_border_color",
        ];

        let mut positional_consumed = false;
        for attr in attrs {
            let mut rewritten = attr.value.clone();
            syn::visit_mut::VisitMut::visit_expr_mut(
                &mut crate::visit_id::IdRewriter {
                    captured: &mut id_lookups,
                },
                &mut rewritten,
            );
            let value = &rewritten;

            let name = match &attr.name {
                Some(n) => n.to_string(),
                None => match (primary_attr_for(widget_name), positional_consumed) {
                    (Some(primary), false) => {
                        positional_consumed = true;
                        primary.to_string()
                    }
                    (Some(_), true) => {
                        errors.push(
                            syn::Error::new(
                                syn::spanned::Spanned::span(&attr.value),
                                format!(
                                    "{widget_name} accepts only one positional argument; pass extra fields by name",
                                ),
                            )
                            .to_compile_error(),
                        );
                        continue;
                    }
                    (None, _) => {
                        errors.push(
                            syn::Error::new(
                                syn::spanned::Spanned::span(&attr.value),
                                format!(
                                    "{widget_name} does not accept positional arguments; use `name: value` form",
                                ),
                            )
                            .to_compile_error(),
                        );
                        continue;
                    }
                },
            };

            let attr_span = attr
                .name
                .as_ref()
                .map(|n| n.span())
                .unwrap_or_else(|| syn::spanned::Spanned::span(&attr.value));

            if name == "text_capacity" && (is_text_widget || is_button_widget) {
                if text_capacity.is_some() {
                    errors.push(
                        syn::Error::new(attr_span, "duplicate `text_capacity`").to_compile_error(),
                    );
                } else if attr.reactive {
                    errors.push(
                        syn::Error::new(
                            attr_span,
                            "`text_capacity` must be a compile-time constant",
                        )
                        .to_compile_error(),
                    );
                } else {
                    text_capacity = Some(quote! { #value });
                }
                continue;
            }

            if crate::layout_reactive::is_layout_placeholder(&attr.value) {
                match responsive_property(widget_name, &name) {
                    Some(property) => {
                        match crate::layout_reactive::LayoutValue::parse_expr(&attr.value) {
                            Ok(Some(value)) => layout_binds.push(LayoutBind {
                                property: syn::Ident::new(property, attr_span),
                                value,
                            }),
                            Ok(None) => unreachable!("layout placeholder already checked"),
                            Err(error) => errors.push(error.to_compile_error()),
                        }
                    }
                    None => errors.push(
                        syn::Error::new(
                            attr_span,
                            format!("`{name}` does not support layout-responsive `@` binding"),
                        )
                        .to_compile_error(),
                    ),
                }
                continue;
            }

            if name == "container" {
                match value {
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Bool(enabled),
                        ..
                    }) if enabled.value => {
                        component_inserts.push(quote! { ::mirui::ui::LayoutContainer });
                    }
                    _ => errors.push(
                        syn::Error::new_spanned(value, "`container` only accepts `true`")
                            .to_compile_error(),
                    ),
                }
                continue;
            }

            // Must run before the Text / Component `text` routes below, or a
            // reactive `text` gets frozen as static content instead of bound.
            if attr.reactive {
                match reactive_property(widget_name, &name) {
                    Some(property) => {
                        reactive_binds.push(ReactiveBind {
                            property: syn::Ident::new(property, attr_span),
                            expr: reactive_read(value),
                            format_expr: value.clone(),
                        });
                        continue;
                    }
                    None => {
                        errors.push(
                            syn::Error::new(
                                attr_span,
                                format!("`{name}` does not support reactive `$` binding"),
                            )
                            .to_compile_error(),
                        );
                        continue;
                    }
                }
            }

            if (is_text_widget || is_button_widget) && name == "text" {
                text_tuple_value = Some(quote! { #value });
                continue;
            }

            if is_text_widget && name == "paragraph" {
                text_paragraph_value = Some(quote! { #value });
                continue;
            }

            if is_text_widget && name == "path" {
                builder_calls.push(quote! { .text_path(#value) });
                continue;
            }

            if is_text_input_widget && TEXT_INPUT_FIELDS.contains(&name.as_str()) {
                let field_ident = syn::Ident::new(&name, attr_span);
                component_fields.push(Self::field_init_tokens(&field_ident, value));
                continue;
            }

            if widget_kind == WidgetKind::Component && name == "text" {
                let field_ident = syn::Ident::new(&name, attr_span);
                component_fields.push(Self::field_init_tokens(&field_ident, value));
                continue;
            }

            match name.as_str() {
                "bg_color" => builder_calls.push(quote! { .bg_color(#value) }),
                "text" => builder_calls.push(quote! { .text(#value) }),
                "text_color" => builder_calls.push(quote! { .text_color(#value) }),
                "border_radius" => builder_calls.push(quote! { .border_radius(#value) }),
                "clip_children" => builder_calls.push(quote! { .clip_children(#value) }),
                "border_color" => builder_calls.push(quote! { .border(#value, 1) }),
                "border_width" => builder_calls.push(quote! { .border_width(#value) }),
                "font" => builder_calls.push(quote! { .font(#value) }),
                "font_stack" => builder_calls.push(quote! { .font_stack(#value) }),
                "font_size" => builder_calls.push(quote! { .font_size(#value) }),
                "width" => {
                    layout_fields.push(quote! { width: mirui::types::Dimension::from(#value) })
                }
                "min_width" => {
                    layout_fields.push(quote! { min_width: mirui::types::Dimension::from(#value) })
                }
                "max_width" => {
                    layout_fields.push(quote! { max_width: mirui::types::Dimension::from(#value) })
                }
                "height" => {
                    layout_fields.push(quote! { height: mirui::types::Dimension::from(#value) })
                }
                "min_height" => {
                    layout_fields.push(quote! { min_height: mirui::types::Dimension::from(#value) })
                }
                "max_height" => {
                    layout_fields.push(quote! { max_height: mirui::types::Dimension::from(#value) })
                }
                "grow" => {
                    layout_fields.push(quote! { grow: mirui::types::Fixed::from_f32(#value) })
                }
                "shrink" => {
                    layout_fields.push(quote! { shrink: mirui::types::Fixed::from_f32(#value) })
                }
                "direction" => {
                    user_set_direction = true;
                    layout_fields.push(quote! { direction: #value });
                }
                "wrap" => layout_fields.push(quote! { wrap: #value }),
                "justify" => layout_fields.push(quote! { justify: #value }),
                "align" => layout_fields.push(quote! { align: #value }),
                "padding" => layout_fields.push(quote! { padding: #value }),
                "gap" => {
                    layout_fields.push(quote! { row_gap: mirui::types::Dimension::from(#value) });
                    layout_fields
                        .push(quote! { column_gap: mirui::types::Dimension::from(#value) });
                }
                "row_gap" => {
                    layout_fields.push(quote! { row_gap: mirui::types::Dimension::from(#value) })
                }
                "column_gap" => {
                    layout_fields.push(quote! { column_gap: mirui::types::Dimension::from(#value) })
                }
                "position" => layout_fields.push(quote! { position: #value }),
                "left" => {
                    layout_fields.push(quote! { left: mirui::types::Dimension::from(#value) })
                }
                "top" => layout_fields.push(quote! { top: mirui::types::Dimension::from(#value) }),
                "image" => builder_calls.push(quote! { .image(#value) }),
                "id" => match Self::extract_id_str(value) {
                    Some(s) => id = Some(s),
                    None => errors.push(
                        syn::Error::new_spanned(
                            value,
                            "id attribute must be a string literal, e.g. `id: \"submit\"`",
                        )
                        .to_compile_error(),
                    ),
                },
                unknown => match widget_kind {
                    WidgetKind::Component => {
                        let field_ident = syn::Ident::new(unknown, attr_span);
                        component_fields.push(Self::field_init_tokens(&field_ident, value));
                    }
                    WidgetKind::Layout | WidgetKind::IllegalLowercase => {
                        let mut msg = format!("unknown widget attribute `{unknown}`");
                        let candidates = LAYOUT_ATTRS
                            .iter()
                            .copied()
                            .chain(STYLE_ATTRS.iter().copied())
                            .chain(["text", "image", "id"].iter().copied());
                        if let Some(hint) = crate::diag::closest(unknown, candidates, 2) {
                            msg.push_str(&format!(". did you mean `{hint}`?"));
                        }
                        errors.push(syn::Error::new(attr_span, msg).to_compile_error());
                    }
                },
            }
        }
        ParsedAttrs {
            builder_calls,
            layout_fields,
            errors,
            component_inserts,
            component_fields,
            text_tuple_value,
            text_capacity,
            text_paragraph_value,
            id,
            id_lookups,
            reactive_binds,
            layout_binds,
            user_set_direction,
        }
    }

    fn extract_id_str(expr: &syn::Expr) -> Option<syn::LitStr> {
        if let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) = expr
        {
            Some(s.clone())
        } else {
            None
        }
    }

    fn field_init_tokens(field_ident: &syn::Ident, value: &syn::Expr) -> proc_macro2::TokenStream {
        let is_float_literal = matches!(
            value,
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Float(_),
                ..
            })
        );
        if is_float_literal {
            quote! { #field_ident: #value }
        } else {
            quote! { #field_ident: (#value).into() }
        }
    }

    /// Emit a Cmd, returning generated tokens.
    /// `parent_var` is the variable name of the parent widget that children attach to.
    fn emit_cmd(
        cmd: &Cmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
        reserved: Option<&syn::Ident>,
    ) -> proc_macro2::TokenStream {
        match cmd {
            Cmd::Widget(w) => Self::emit_widget(w, world, parent_var, reserved),
            Cmd::Compose(c) => Self::emit_compose(c, world, parent_var),
            Cmd::Iter(i) => Self::emit_iter(i, world, parent_var),
            Cmd::If(i) => Self::emit_if(i, world, parent_var),
            Cmd::Niche(n) => Self::emit_niche(n, world, parent_var),
            Cmd::Match(m) => Self::emit_match(m, world, parent_var),
            Cmd::CodeBlock(ts) => quote! { #ts },
        }
    }

    fn references_id(cmd: &Cmd, key: &str) -> bool {
        let body_references = |body: &[Cmd]| body.iter().any(|cmd| Self::references_id(cmd, key));
        match cmd {
            Cmd::Widget(w) => {
                w.id_lookups.iter().any(|(_, name)| name == key) || body_references(&w.children)
            }
            Cmd::Compose(c) => c.id_lookups.iter().any(|(_, name)| name == key),
            Cmd::Iter(i) => body_references(&i.body),
            Cmd::If(i) => i
                .branches
                .iter()
                .any(|branch| body_references(&branch.body)),
            Cmd::Niche(n) => body_references(&n.body),
            Cmd::Match(m) => m.arms.iter().any(|arm| body_references(&arm.body)),
            Cmd::CodeBlock(_) => false,
        }
    }

    fn emit_siblings<'a>(
        body: &'a [Cmd],
        world: &'a proc_macro2::TokenStream,
        parent_var: &'a proc_macro2::TokenStream,
    ) -> impl Iterator<Item = proc_macro2::TokenStream> + 'a {
        let mut declarations = proc_macro2::TokenStream::new();
        let reserved: Vec<_> = body
            .iter()
            .enumerate()
            .map(|(index, cmd)| {
                let Cmd::Widget(widget) = cmd else {
                    return None;
                };
                let id = widget.id.as_ref()?;
                let key = id.value();
                if !body[..index]
                    .iter()
                    .any(|cmd| Self::references_id(cmd, &key))
                {
                    return None;
                }
                let var = quote::format_ident!("__reserved_{}", widget.var);
                declarations.extend(quote! {
                    let #var = ::mirui::__macro_support::ReservedWidget::new(#world, #id);
                });
                Some(var)
            })
            .collect();
        body.iter()
            .zip(reserved)
            .enumerate()
            .map(move |(index, (cmd, reserved))| {
                let stmt = Self::emit_cmd(cmd, world, parent_var, reserved.as_ref());
                if index == 0 {
                    quote! { #declarations #stmt }
                } else {
                    stmt
                }
            })
    }

    fn emit_compose(
        cmd: &ComposeCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let function = &cmd.function;
        let var = &cmd.var;
        let args = &cmd.args;
        let errors = &cmd.errors;
        let id_lookups = cmd.id_lookups.iter().map(|(ident, key)| {
            quote! {
                let #ident = mirui::ecs::World::find_by_id(&*(#world), #key)
                    .expect(concat!("ui!: id '", #key, "' not found in IdMap"));
            }
        });
        quote! {
            #(#errors)*
            let #var: mirui::ecs::Entity = {
                #(#id_lookups)*
                let __compose_parent = #parent_var;
                let __compose_factory = #function(#(#args),*);
                let mut __compose_cx = mirui::__macro_support::new_scope(#world, __compose_parent);
                __compose_factory(&mut __compose_cx)
            };
        }
    }

    fn emit_on_dispatch(
        widget_name: &str,
        handlers: &[&OnCmd],
        widget_var: &syn::Ident,
        world: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let mut tokens = proc_macro2::TokenStream::new();
        for handler in handlers {
            let event = &handler.name;
            let host = match &handler.qualifier {
                Some(qualifier) => quote!(#qualifier),
                None if RESERVED_LAYOUT_NAMES.contains(&widget_name) => quote!(::mirui::ui::Widget),
                None => {
                    let component = syn::Ident::new(widget_name, proc_macro2::Span::call_site());
                    quote!(#component)
                }
            };
            let key = syn::Ident::new("__mirui_event_key", proc_macro2::Span::mixed_site());
            let callback_name = syn::LitStr::new(&format!("on {event}"), handler.name.span());
            let callback =
                match event_attr::lower_inline_callback(&handler.callback, &key, &callback_name) {
                    Ok(callback) => callback,
                    Err(error) => {
                        tokens.extend(error.to_compile_error());
                        continue;
                    }
                };
            let install = quote! {
                let __mirui_callback: ::mirui::__macro_support::EventCallback<
                    _, ::mirui::__macro_support::NodeContext,
                > = #callback;
                ::mirui::__macro_support::install_entity_event(
                    #world, #widget_var, #key, __mirui_callback,
                );
            };
            let selector = quote!(#host::#event);
            let options = &handler.options;
            let binding = if options.is_empty() {
                quote! {
                    let #key = #selector;
                    #install
                }
            } else {
                quote! {
                    match (#selector).configure((#(#options,)*)) {
                        ::core::result::Result::Ok(#key) => { #install }
                        ::core::result::Result::Err(error) => {
                            ::mirui::warn!(
                                target: "mirui::event::install",
                                "skipping event binding: {:?}",
                                error,
                            );
                        }
                    }
                }
            };
            tokens.extend(quote! {
                {
                    #[allow(unused_imports)]
                    use ::mirui::input::event::gesture::GestureSelectors as _;
                    #[allow(unused_imports)]
                    use ::mirui::input::event::input::InputSelectors as _;
                    #[allow(unused_imports)]
                    use ::mirui::input::event::layout::LayoutSelectors as _;
                    #binding
                }
            });
        }
        tokens
    }
    fn emit_widget(
        cmd: &WidgetCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
        reserved: Option<&syn::Ident>,
    ) -> proc_macro2::TokenStream {
        let var = &cmd.var;
        let attrs = &cmd.attrs;
        let layout_fields = &cmd.layout_fields;
        let errors = &cmd.errors;

        let mut tokens = proc_macro2::TokenStream::new();

        for e in errors {
            tokens.extend(e.clone());
        }

        for (ident, key) in &cmd.id_lookups {
            tokens.extend(quote! {
                let #ident = mirui::ecs::World::find_by_id(&*(#world), #key)
                    .expect(concat!("ui!: id '", #key, "' not found in IdMap"));
            });
        }

        let on_handlers: Vec<&OnCmd> = cmd.on_handlers.iter().collect();
        let layout_call = if layout_fields.is_empty() {
            quote! {}
        } else {
            quote! { .layout(mirui::ui::layout::LayoutStyle { #(#layout_fields,)* ..Default::default() }) }
        };

        let builder = match reserved {
            Some(reserved) => quote! { #reserved.build(#world) },
            None => quote! { mirui::__macro_support::WidgetBuilder::new(#world) },
        };
        tokens.extend(quote! {
            let #var = #builder
                #(#attrs)*
                #layout_call
                .id();
        });

        if cmd.name == "Scroll" {
            tokens.extend(quote! {
                if let Some(__style) = (#world).get_mut::<mirui::ui::Style>(#var) {
                    __style.clip_children = true;
                }
                (#world).insert(#var, mirui::input::event::scroll::ScrollOffset::default());
                (#world).insert(
                    #var,
                    mirui::input::event::scroll::ScrollConfig {
                        elastic: false,
                        ..Default::default()
                    },
                );
            });
        }

        // Must precede the reactive-bind injection: an effect's first run writes
        // into the component, so seeding it after would clobber that value.
        if cmd.kind == WidgetKind::Component {
            let comp_name = &cmd.name;
            if cmd.name == "Text" {
                let init = if let Some(capacity) = &cmd.text_capacity {
                    let paragraph = cmd
                        .text_paragraph_value
                        .as_ref()
                        .map(|value| quote! { .with_paragraph(#value) });
                    quote! {
                        #comp_name::new_bounded(#capacity)
                            .expect("Text text_capacity could not be reserved")
                            #paragraph
                    }
                } else {
                    match (&cmd.text_tuple_value, &cmd.text_paragraph_value) {
                        (Some(text_value), Some(paragraph_value)) => quote! {
                            ::core::convert::Into::<#comp_name>::into(#text_value)
                                .with_paragraph(#paragraph_value)
                        },
                        (Some(text_value), None) => {
                            quote! { ::core::convert::Into::<#comp_name>::into(#text_value) }
                        }
                        (None, Some(paragraph_value)) => {
                            quote! { #comp_name::from("").with_paragraph(#paragraph_value) }
                        }
                        (None, None) => quote! { #comp_name::from("") },
                    }
                };
                tokens.extend(quote! {
                    (#world).insert(#var, #init);
                });
            } else {
                let comp_fields = &cmd.component_fields;
                tokens.extend(quote! {
                    (#world).insert(#var, {
                        #[allow(clippy::needless_update)]
                        let __c = #comp_name {
                            #(#comp_fields,)*
                            ..Default::default()
                        };
                        __c
                    });
                });
                if cmd.name == "Button" {
                    if let Some(capacity) = &cmd.text_capacity {
                        tokens.extend(quote! {
                            (#world).insert(
                                #var,
                                ::mirui::ui::widgets::Text::new_bounded(#capacity)
                                    .expect("Button text_capacity could not be reserved")
                                    .with_paragraph(::mirui::ui::widgets::ParagraphStyle::label()),
                            );
                        });
                    } else if let Some(text_value) = &cmd.text_tuple_value {
                        tokens.extend(quote! {
                            (#world).insert(
                                #var,
                                ::mirui::ui::widgets::Text::label(#text_value),
                            );
                        });
                    }
                }
            }
        }

        if let Some(capacity) = &cmd.text_capacity
            && !cmd
                .reactive_binds
                .iter()
                .any(|binding| binding.property == "TextContent")
        {
            let value = cmd
                .text_tuple_value
                .as_ref()
                .map(|value| quote! { #value })
                .unwrap_or_else(|| quote! { "" });
            let apply = syn::parse2::<syn::Expr>(value.clone())
                .ok()
                .and_then(|value| direct_format_args_call(&value, &quote! { #var }, capacity))
                .unwrap_or_else(|| {
                    quote! {
                        mirui::ui::property::apply_bounded_text::<{ #capacity }>(
                            #var,
                            ::core::format_args!("{}", #value),
                        );
                    }
                });
            tokens.extend(quote! {
                mirui::__macro_support::with_world_scope(#world, || {
                    #apply
                });
            });
        }

        // Capture bindings before attachments may move their declared Models.
        let mut prepared_bindings = Vec::new();
        for (index, binding) in cmd.reactive_binds.iter().enumerate() {
            let prepared = syn::Ident::new(
                &format!("{var}_binding_{index}"),
                proc_macro2::Span::mixed_site(),
            );
            let property = &binding.property;
            let expr = &binding.expr;
            let body = if property == "TextContent"
                && let Some(capacity) = &cmd.text_capacity
            {
                direct_format_args_call(&binding.format_expr, &quote! { #var }, capacity)
                    .unwrap_or_else(|| {
                        quote! {
                            mirui::ui::property::apply_bounded_text::<{ #capacity }>(
                                #var,
                                ::core::format_args!("{}", #expr),
                            );
                        }
                    })
            } else {
                let value = if property == "TextContent" {
                    quote! { mirui::ui::property::IntoText::into_text(__v) }
                } else {
                    quote! { ::core::convert::Into::into(__v) }
                };
                quote! {
                    let __v = #expr;
                    mirui::ui::property::apply::<mirui::ui::property::prop::#property>(#var, #value);
                }
            };
            tokens.extend(quote! {
                let #prepared = mirui::__macro_support::prepare_widget_effect(#var, move || {
                    #body
                });
            });
            prepared_bindings.push(prepared);
        }

        let enchants = &cmd.enchants;
        for enchant in enchants {
            tokens.extend(quote! {
                (#world).insert(#var, #enchant);
            });
        }
        if let Some(id_lit) = &cmd.id {
            tokens.extend(quote! {
                (#world).insert(#var, mirui::ui::NamedId(#id_lit));
            });
        }
        tokens.extend(quote! {
            let __widget_parent = #parent_var;
            ::mirui::__macro_support::finish_widget_static(#world, __widget_parent, #var);
        });

        for prepared in prepared_bindings {
            tokens.extend(quote! { #prepared.install(#world); });
        }

        if !cmd.layout_binds.is_empty() {
            let mut dependencies = Vec::new();
            for binding in &cmd.layout_binds {
                for dependency in &binding.value.dependencies {
                    if !dependencies
                        .iter()
                        .any(|candidate: &crate::layout_reactive::Dependency| {
                            candidate.same_source(dependency)
                        })
                    {
                        dependencies.push(dependency.clone());
                    }
                }
            }
            if dependencies.len() > 4 {
                tokens.extend(
                    syn::Error::new(
                        cmd.layout_binds[0].property.span(),
                        "layout-responsive properties on one widget use more than four distinct dependencies",
                    )
                    .to_compile_error(),
                );
            } else {
                let emitted_dependencies = dependencies.iter().map(|dependency| dependency.emit());
                let mut applies = Vec::new();
                for binding in &cmd.layout_binds {
                    let mut value = binding.value.clone();
                    match value.emit_apply(&binding.property, &dependencies) {
                        Ok(apply) => applies.push(apply),
                        Err(error) => tokens.extend(error.to_compile_error()),
                    }
                }
                tokens.extend(quote! {
                    (#world).insert(
                        #var,
                        ::mirui::__macro_support::LayoutBinding::new(
                            const { &[#(#emitted_dependencies),*] },
                            |__layout_world, __layout_target, __layout_values| {
                                let mut __layout_changed = false;
                                #(#applies)*
                                __layout_changed
                            },
                        ),
                    );
                });
            }
        }

        if !on_handlers.is_empty() {
            tokens.extend(Self::emit_on_dispatch(
                &cmd.name.to_string(),
                &on_handlers,
                &cmd.var,
                world,
            ));
        }

        let var_ts = quote! { #var };
        tokens.extend(
            Self::emit_siblings(&cmd.children, world, &var_ts)
                .collect::<proc_macro2::TokenStream>(),
        );

        tokens
    }

    fn emit_match(
        cmd: &MatchCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd.reactive {
            let read = reactive_read(&cmd.scrutinee);
            Self::emit_match_reactive(cmd, &read, world, parent_var)
        } else {
            Self::emit_match_static(cmd, world, parent_var)
        }
    }

    fn emit_match_reactive(
        cmd: &MatchCmd,
        read: &proc_macro2::TokenStream,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd
            .arms
            .iter()
            .any(|arm| Self::has_uncontained_niche(&arm.body))
        {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "slot operations directly inside a reactive `match` arm need a widget parent",
            )
            .to_compile_error();
        }
        if cmd
            .arms
            .iter()
            .any(|arm| Self::has_uncontained_reactive_child(&arm.body))
        {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "reactive control flow directly inside a reactive `match` arm needs a widget parent",
            )
            .to_compile_error();
        }
        if cmd.arms.iter().any(|arm| Self::pattern_binds(&arm.pat)) {
            return Self::emit_match_reactive_dynamic(cmd, read, world, parent_var);
        }
        Self::emit_match_reactive_cached(cmd, read, world, parent_var)
    }

    // Unknown pattern forms take the dynamic path, so an arm body can always
    // use any values its pattern introduces.
    fn pattern_binds(pat: &syn::Pat) -> bool {
        match pat {
            syn::Pat::Const(_)
            | syn::Pat::Lit(_)
            | syn::Pat::Path(_)
            | syn::Pat::Range(_)
            | syn::Pat::Rest(_)
            | syn::Pat::Wild(_) => false,
            syn::Pat::Or(pat) => pat.cases.iter().any(Self::pattern_binds),
            syn::Pat::Paren(pat) => Self::pattern_binds(&pat.pat),
            syn::Pat::Reference(pat) => Self::pattern_binds(&pat.pat),
            syn::Pat::Slice(pat) => pat.elems.iter().any(Self::pattern_binds),
            syn::Pat::Struct(pat) => pat
                .fields
                .iter()
                .any(|field| Self::pattern_binds(&field.pat)),
            syn::Pat::Tuple(pat) => pat.elems.iter().any(Self::pattern_binds),
            syn::Pat::TupleStruct(pat) => pat.elems.iter().any(Self::pattern_binds),
            syn::Pat::Type(pat) => Self::pattern_binds(&pat.pat),
            // syn represents the prelude's bare `None` variant as an identifier.
            syn::Pat::Ident(pat)
                if pat.ident == "None"
                    && pat.by_ref.is_none()
                    && pat.mutability.is_none()
                    && pat.subpat.is_none() =>
            {
                false
            }
            syn::Pat::Ident(_) | syn::Pat::Macro(_) | syn::Pat::Verbatim(_) => true,
            _ => true,
        }
    }

    fn emit_match_reactive_cached(
        cmd: &MatchCmd,
        read: &proc_macro2::TokenStream,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let build_branches = cmd.arms.iter().map(|arm| {
            let body = Self::emit_branch_body_inline(&arm.body, world, &quote! { __parent_e });
            quote! {
                {
                    let __start = (#world)
                        .get::<mirui::ui::Children>(__parent_e)
                        .map_or(0, |children| children.0.len());
                    #body
                    (#world)
                        .get::<mirui::ui::Children>(__parent_e)
                        .map_or_else(mirui::__macro_support::__Vec::new, |children| children.0[__start..].to_vec())
                }
            }
        });
        let select_arms = cmd.arms.iter().enumerate().map(|(index, arm)| {
            let pat = &arm.pat;
            quote! { #pat => #index, }
        });

        quote! {
            {
                let __parent_e = #parent_var;
                let __branches = [#(#build_branches),*];
                mirui::ui::branch::prepare(#world, __parent_e, &__branches);
                let __selected = mirui::__macro_support::__Cell::new(None::<usize>);
                mirui::__macro_support::with_world_scope(#world, || {
                    mirui::core::reactive::effect_with_widget(__parent_e, move || {
                        let __next = Some(match #read {
                            #(#select_arms)*
                        });
                        let __previous = __selected.get();
                        if __previous != __next {
                            mirui::__macro_support::with_world(|__w| {
                                mirui::ui::branch::select(__w, &__branches, __previous, __next);
                                __selected.set(__next);
                            });
                        }
                    });
                });
            }
        }
    }

    // A binding is only in scope while its selected arm is built. Bound arms
    // keep structural rebuild behavior, with all top-level roots managed.
    fn emit_match_reactive_dynamic(
        cmd: &MatchCmd,
        read: &proc_macro2::TokenStream,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let select_arms = cmd.arms.iter().map(|arm| {
            let pat = &arm.pat;
            let body =
                Self::emit_branch_body_inline(&arm.body, &quote! { __w }, &quote! { __parent_e });
            quote! { #pat => { #body }, }
        });
        quote! {
            {
                let __mounted = mirui::__macro_support::__RefCell::new(mirui::__macro_support::__Vec::<mirui::ecs::Entity>::new());
                let __parent_e = #parent_var;
                let __mount_index = (#world)
                    .get::<mirui::ui::Children>(__parent_e)
                    .map_or(0, |children| children.0.len());
                mirui::__macro_support::with_world_scope(#world, || {
                    mirui::core::reactive::effect_with_widget(__parent_e, move || {
                        let __sel = #read;
                        mirui::__macro_support::with_world(|__w| {
                            let mut __mounted = __mounted.borrow_mut();
                            let __idx = __mounted.first().and_then(|o| {
                                __w.get::<mirui::ui::Children>(__parent_e)
                                    .and_then(|c| c.0.iter().position(|e| e == o))
                            }).unwrap_or(__mount_index);
                            for __o in __mounted.drain(..) {
                                mirui::ui::despawn_subtree(__w, __o);
                            }
                            let __start = __w
                                .get::<mirui::ui::Children>(__parent_e)
                                .map_or(0, |children| children.0.len());
                            match __sel {
                                #(#select_arms)*
                            }
                            if let Some(children) = __w.get_mut::<mirui::ui::Children>(__parent_e) {
                                __mounted.extend(children.0.drain(__start..));
                                let __pos = __idx.min(children.0.len());
                                for (offset, &root) in __mounted.iter().enumerate() {
                                    children.0.insert(__pos + offset, root);
                                }
                            }
                        });
                    });
                });
            }
        }
    }

    fn emit_match_static(
        cmd: &MatchCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let scrutinee = &cmd.scrutinee;
        let arm_tokens = cmd.arms.iter().map(|arm| {
            let pat = &arm.pat;
            let body_tokens: proc_macro2::TokenStream =
                Self::emit_siblings(&arm.body, world, parent_var).collect();
            quote! { #pat => { #body_tokens } }
        });

        quote! {
            match #scrutinee {
                #(#arm_tokens)*
            }
        }
    }

    fn emit_niche(
        cmd: &NicheCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd.is_declaration {
            if let Some(slot_var) = &cmd.mold_slot_var {
                let slot_name = cmd.name.to_string();
                let slot_var_ts = quote! { #slot_var };
                let fallback: proc_macro2::TokenStream =
                    Self::emit_siblings(&cmd.body, world, &slot_var_ts).collect();
                return quote! {
                    let __slot_parent = #parent_var;
                    let #slot_var = ::mirui::__macro_support::WidgetBuilder::new(#world).id();
                    ::mirui::__macro_support::finish_widget_static(#world, __slot_parent, #slot_var);
                    __mold_niche_map.insert(#slot_name, #slot_var);
                    #fallback
                };
            }
            let msg = format!(
                "ui!: `@@{}` is a slot declaration; use it inside `ui!(compose ...)` declarations — call sites fill slots with the single-`@` form `@{}` {{ ... }}",
                cmd.name, cmd.name,
            );
            return quote! {
                compile_error!(#msg);
            };
        }

        let niche_name = cmd.name.to_string();
        let niche_var = syn::Ident::new(
            &format!("__niche_{}", cmd.name),
            proc_macro2::Span::call_site(),
        );
        let niche_var_ts = quote! { #niche_var };

        let slot_check = if let Some(pw) = &cmd.parent_widget {
            let slot_method = syn::Ident::new(&format!("__slot_{}", cmd.name), cmd.name.span());
            quote! {
                let _ = <#pw>::#slot_method;
            }
        } else {
            quote! {}
        };

        let body_tokens: proc_macro2::TokenStream =
            Self::emit_siblings(&cmd.body, world, &niche_var_ts).collect();

        let widget_label = quote! { stringify!(#parent_var) };
        quote! {
            #slot_check
            let #niche_var = match (#world).get::<mirui::ui::NicheMap>(#parent_var) {
                Some(map) => match map.get(#niche_name) {
                    Some(e) => e,
                    None => panic!(
                        "ui!: niche '{}' not registered on widget {}",
                        #niche_name,
                        #widget_label,
                    ),
                },
                None => panic!(
                    "ui!: widget {} has no NicheMap (missing auto_attach?)",
                    #widget_label
                ),
            };
            {
                let __existing: ::mirui::__macro_support::__Vec<::mirui::ecs::Entity> = (#world)
                    .get::<::mirui::ui::Children>(#niche_var_ts)
                    .map(|c| c.0.clone())
                    .unwrap_or_default();
                for __e in __existing {
                    ::mirui::ui::despawn_subtree(#world, __e);
                }
            }
            #body_tokens
        }
    }

    fn emit_iter(
        cmd: &IterCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd.reactive {
            Self::emit_iter_reactive(cmd, world, parent_var)
        } else {
            Self::emit_iter_static(cmd, world, parent_var)
        }
    }

    fn emit_iter_static(
        cmd: &IterCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let iterable = &cmd.iterable;
        let variable = &cmd.variable;

        let body_tokens: proc_macro2::TokenStream =
            Self::emit_siblings(&cmd.body, world, parent_var).collect();

        quote! {
            for #variable in #iterable {
                #body_tokens
            }
        }
    }

    fn emit_if(
        cmd: &IfCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd.reactive {
            Self::emit_if_reactive(cmd, world, parent_var)
        } else {
            Self::emit_if_static(cmd, world, parent_var)
        }
    }

    fn emit_branch_body_inline(
        body: &[Cmd],
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        Self::emit_siblings(body, world, parent_var).collect()
    }

    fn emit_if_static(
        cmd: &IfCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let mut chain = proc_macro2::TokenStream::new();
        for (i, br) in cmd.branches.iter().enumerate() {
            let body = Self::emit_branch_body_inline(&br.body, world, parent_var);
            match (&br.cond, i) {
                (Some(c), 0) => chain.extend(quote! { if #c { #body } }),
                (Some(c), _) => chain.extend(quote! { else if #c { #body } }),
                (None, _) => chain.extend(quote! { else { #body } }),
            }
        }
        chain
    }

    fn collect_children(&mut self, children: &[DsTreeRef]) -> Vec<Cmd> {
        self.stack.push(Vec::new());
        for child in children {
            decipher(child, self);
        }
        self.stack.pop().unwrap()
    }

    // A reactive walk body built on demand. The first top widget is its
    // managed mount root; later top widgets attach through the usual path.
    fn branch_body(body: &[Cmd]) -> proc_macro2::TokenStream {
        let w = quote! { __w };
        let p = quote! { __parent };
        let mut stmts = proc_macro2::TokenStream::new();
        let mut root: Option<proc_macro2::TokenStream> = None;
        for (child, statement) in body.iter().zip(Self::emit_siblings(body, &w, &p)) {
            stmts.extend(statement);
            let child_var = match child {
                Cmd::Widget(cw) => Some(&cw.var),
                Cmd::Compose(compose) => Some(&compose.var),
                _ => None,
            };
            if let Some(cv) = child_var
                && root.is_none()
            {
                stmts.extend(quote! {
                    if let Some(children) = (#w).get_mut::<mirui::ui::Children>(#p) {
                        if let Some(index) = children.0.iter().position(|entity| *entity == #cv) {
                            children.0.remove(index);
                        }
                    }
                });
                root = Some(quote! { #cv });
            }
        }
        let ret = match root {
            Some(v) => quote! { Some(#v) },
            None => quote! { None },
        };
        quote! { #stmts #ret }
    }

    fn has_uncontained_reactive_child(body: &[Cmd]) -> bool {
        body.iter().any(|child| match child {
            Cmd::If(branch) => {
                branch.reactive
                    || branch
                        .branches
                        .iter()
                        .any(|arm| Self::has_uncontained_reactive_child(&arm.body))
            }
            Cmd::Match(branch) => {
                branch.reactive
                    || branch
                        .arms
                        .iter()
                        .any(|arm| Self::has_uncontained_reactive_child(&arm.body))
            }
            Cmd::Iter(iter) => iter.reactive || Self::has_uncontained_reactive_child(&iter.body),
            Cmd::Niche(niche) => Self::has_uncontained_reactive_child(&niche.body),
            Cmd::Widget(_) | Cmd::Compose(_) | Cmd::CodeBlock(_) => false,
        })
    }

    fn has_uncontained_niche(body: &[Cmd]) -> bool {
        body.iter().any(|child| match child {
            Cmd::Niche(_) => true,
            Cmd::If(branch) => branch
                .branches
                .iter()
                .any(|arm| Self::has_uncontained_niche(&arm.body)),
            Cmd::Match(branch) => branch
                .arms
                .iter()
                .any(|arm| Self::has_uncontained_niche(&arm.body)),
            Cmd::Iter(iter) => Self::has_uncontained_niche(&iter.body),
            Cmd::Widget(_) | Cmd::Compose(_) | Cmd::CodeBlock(_) => false,
        })
    }

    fn emit_if_reactive(
        cmd: &IfCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd
            .branches
            .iter()
            .any(|branch| Self::has_uncontained_niche(&branch.body))
        {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "slot operations directly inside a cached `if` branch need a widget parent",
            )
            .to_compile_error();
        }
        if cmd
            .branches
            .iter()
            .any(|branch| Self::has_uncontained_reactive_child(&branch.body))
        {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "reactive control flow directly inside a cached `if` branch needs a widget parent",
            )
            .to_compile_error();
        }
        let build_branches = cmd.branches.iter().map(|branch| {
            let body = Self::emit_branch_body_inline(&branch.body, world, &quote! { __parent_e });
            quote! {
                {
                    let __start = (#world)
                        .get::<mirui::ui::Children>(__parent_e)
                        .map_or(0, |children| children.0.len());
                    #body
                    (#world)
                        .get::<mirui::ui::Children>(__parent_e)
                        .map_or_else(mirui::__macro_support::__Vec::new, |children| children.0[__start..].to_vec())
                }
            }
        });

        let mut cascade = proc_macro2::TokenStream::new();
        let mut first = true;
        let mut has_else = false;
        for (index, br) in cmd.branches.iter().enumerate() {
            match &br.cond {
                Some(cond) => {
                    let read = reactive_read(cond);
                    let head = if first { quote!(if) } else { quote!(else if) };
                    cascade.extend(quote! { #head (#read) { Some(#index) } });
                    first = false;
                }
                None => {
                    cascade.extend(quote! { else { Some(#index) } });
                    has_else = true;
                }
            }
        }
        if !has_else {
            cascade.extend(quote! { else { None } });
        }

        quote! {
            {
                let __parent_e = #parent_var;
                let __branches = [#(#build_branches),*];
                mirui::ui::branch::prepare(#world, __parent_e, &__branches);
                let __selected = mirui::__macro_support::__Cell::new(None::<usize>);
                mirui::__macro_support::with_world_scope(#world, || {
                    mirui::core::reactive::effect_with_widget(__parent_e, move || {
                        let __next: ::core::option::Option<usize> = #cascade;
                        let __previous = __selected.get();
                        if __previous != __next {
                            mirui::__macro_support::with_world(|__w| {
                                mirui::ui::branch::select(__w, &__branches, __previous, __next);
                                __selected.set(__next);
                            });
                        }
                    });
                });
            }
        }
    }

    fn emit_iter_reactive(
        cmd: &IterCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if cmd.key.is_some() {
            Self::emit_iter_reactive_keyed(cmd, world, parent_var)
        } else {
            Self::emit_iter_reactive_indexed(cmd, world, parent_var)
        }
    }

    fn emit_iter_reactive_indexed(
        cmd: &IterCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let iterable = &cmd.iterable;
        let variable = &cmd.variable;
        let row_body = Self::branch_body(&cmd.body);

        quote! {
            {
                let __rows = mirui::__macro_support::__RefCell::new(
                    mirui::__macro_support::__Vec::<mirui::ecs::Entity>::new(),
                );
                let __parent_e = #parent_var;
                mirui::__macro_support::with_world_scope(#world, || {
                    mirui::core::reactive::effect_with_widget(__parent_e, move || {
                        let __items = (#iterable).into_iter();
                        mirui::__macro_support::with_world(|__w| {
                            let __n_old = __rows.borrow().len();
                            let mut __n_new = 0;
                            for #variable in __items {
                                __n_new += 1;
                                if __n_new <= __n_old {
                                    continue;
                                }
                                let __r = (|__w: &mut mirui::ecs::World, __parent: mirui::ecs::Entity| -> Option<mirui::ecs::Entity> {
                                    #row_body
                                })(__w, __parent_e);
                                if let Some(__r) = __r {
                                    if let Some(children) =
                                        __w.get_mut::<mirui::ui::Children>(__parent_e)
                                    {
                                        children.0.push(__r);
                                    }
                                    __rows.borrow_mut().push(__r);
                                }
                            }
                            if __n_new < __n_old {
                                for __e in __rows.borrow_mut().drain(__n_new..) {
                                    let __pos = __w
                                        .get::<mirui::ui::Children>(__parent_e)
                                        .and_then(|c| c.0.iter().position(|&x| x == __e));
                                    if let Some(__p) = __pos {
                                        if let Some(children) =
                                            __w.get_mut::<mirui::ui::Children>(__parent_e)
                                        {
                                            children.0.remove(__p);
                                        }
                                    }
                                    mirui::ui::despawn_subtree(__w, __e);
                                }
                            }
                        });
                    });
                });
            }
        }
    }

    fn emit_iter_reactive_keyed(
        cmd: &IterCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let iterable = &cmd.iterable;
        let variable = &cmd.variable;
        let key_expr = cmd.key.as_ref().unwrap();
        let row_body = Self::branch_body(&cmd.body);

        quote! {
            {
                let __rows = mirui::__macro_support::__RefCell::new((
                    mirui::__macro_support::__Vec::new(),
                    mirui::__macro_support::__Vec::new(),
                ));
                let __parent_e = #parent_var;
                mirui::__macro_support::with_world_scope(#world, || {
                    mirui::core::reactive::effect_with_widget(__parent_e, move || {
                        let __items = (#iterable).into_iter();
                        mirui::__macro_support::with_world(|__w| {
                            let mut __buffers = __rows.borrow_mut();
                            let (__old, __new) = &mut *__buffers;
                            __new.clear();
                            for #variable in __items {
                                let __k = #key_expr;
                                if let Some(__i) = __old.iter().position(|(__ok, _)| *__ok == __k) {
                                    let (_, __e) = __old.remove(__i);
                                    __new.push((__k, __e));
                                } else {
                                    let __r = (|__w: &mut mirui::ecs::World, __parent: mirui::ecs::Entity| -> Option<mirui::ecs::Entity> {
                                        #row_body
                                    })(__w, __parent_e);
                                    if let Some(__r) = __r {
                                        __new.push((__k, __r));
                                    }
                                }
                            }
                            for (_, __e) in __old.drain(..) {
                                mirui::ui::despawn_subtree(__w, __e);
                            }
                            // retain-then-append reorders the rows to match the new
                            // key order while leaving any static siblings in place.
                            if let Some(children) = __w.get_mut::<mirui::ui::Children>(__parent_e) {
                                children.0.retain(|e| !__new.iter().any(|(_, row)| row == e));
                                for (_, __e) in __new.iter() {
                                    children.0.push(*__e);
                                }
                            }
                            __old.reserve(__new.len());
                            ::core::mem::swap(__old, __new);
                        });
                    });
                });
            }
        }
    }
}

impl DsRune for MiruiRune {
    fn inscribe_root(&mut self, _parent_expr: &syn::Expr) {}

    fn inscribe_widget(
        &mut self,
        name: &syn::Ident,
        attrs: &[DsAttr],
        enchants: &[syn::Expr],
        on_handlers: &[xrune::ds_node::ds_on::DsOn],
        children: &[DsTreeRef],
    ) {
        let kind = classify_widget_name(name);
        let is_compose_call = kind == WidgetKind::IllegalLowercase
            && attrs.iter().all(|attr| attr.name.is_none())
            && enchants.is_empty()
            && on_handlers.is_empty()
            && children.is_empty();
        if is_compose_call {
            let mut args = Vec::new();
            let mut errors = Vec::new();
            let mut id_lookups = Vec::new();
            for attr in attrs {
                if let Some(attr_name) = &attr.name {
                    errors.push(
                        syn::Error::new(
                            attr_name.span(),
                            "compose calls accept positional arguments only",
                        )
                        .to_compile_error(),
                    );
                    continue;
                }
                if attr.reactive {
                    errors.push(
                        syn::Error::new(
                            syn::spanned::Spanned::span(&attr.value),
                            "compose call arguments cannot use reactive binding syntax",
                        )
                        .to_compile_error(),
                    );
                    continue;
                }
                let mut value = attr.value.clone();
                syn::visit_mut::VisitMut::visit_expr_mut(
                    &mut crate::visit_id::IdRewriter {
                        captured: &mut id_lookups,
                    },
                    &mut value,
                );
                args.push(value);
            }
            let var = self.next_var();
            self.stack
                .last_mut()
                .unwrap()
                .push(Cmd::Compose(ComposeCmd {
                    function: name.clone(),
                    var,
                    args,
                    errors,
                    id_lookups,
                }));
            return;
        }
        let var = self.next_var();
        let mut parsed = self.parse_attrs(attrs, &name.to_string(), kind);
        if !parsed.user_set_direction {
            let widget_name_str = name.to_string();
            match widget_name_str.as_str() {
                "Row" => {
                    parsed.layout_fields.insert(
                        0,
                        quote! { direction: mirui::ui::layout::FlexDirection::Row },
                    );
                }
                "Column" | "Scroll" => {
                    parsed.layout_fields.insert(
                        0,
                        quote! { direction: mirui::ui::layout::FlexDirection::Column },
                    );
                }
                _ => {}
            }
        }
        let mut id_lookups = parsed.id_lookups;
        let mut enchant_tokens: Vec<proc_macro2::TokenStream> = parsed.component_inserts;
        for e in enchants.iter() {
            let mut rewritten = e.clone();
            syn::visit_mut::VisitMut::visit_expr_mut(
                &mut crate::visit_id::IdRewriter {
                    captured: &mut id_lookups,
                },
                &mut rewritten,
            );
            enchant_tokens.push(quote! { #rewritten });
        }

        let mut collected_on_handlers = Vec::with_capacity(on_handlers.len());
        for handler in on_handlers {
            let mut options = handler.get_args().to_vec();
            let callback = if let Some(body) = handler.get_body() {
                syn::parse_quote!(move |event| {
                    let _ = event;
                    #body
                })
            } else {
                let Some(callback) = options.pop() else {
                    enchant_tokens.push(
                        syn::Error::new(
                            handler.get_name().span(),
                            "event bindings require a block or callback argument",
                        )
                        .to_compile_error(),
                    );
                    continue;
                };
                callback
            };
            collected_on_handlers.push(OnCmd {
                qualifier: handler.get_qualifier().cloned(),
                name: handler.get_name().clone(),
                options,
                callback,
            });
        }

        self.stack.push(Vec::new());
        let pushed_widget_kind = kind == WidgetKind::Component;
        if pushed_widget_kind {
            self.widget_kind_stack.push(Some(name.clone()));
        }
        for child in children {
            decipher(child, self);
        }
        if pushed_widget_kind {
            self.widget_kind_stack.pop();
        }
        let my_children = self.stack.pop().unwrap();

        let cmd = Cmd::Widget(WidgetCmd {
            name: name.clone(),
            kind,
            var,
            attrs: parsed.builder_calls,
            layout_fields: parsed.layout_fields,
            errors: parsed.errors,
            enchants: enchant_tokens,
            on_handlers: collected_on_handlers,
            component_fields: parsed.component_fields,
            text_tuple_value: parsed.text_tuple_value,
            text_capacity: parsed.text_capacity,
            text_paragraph_value: parsed.text_paragraph_value,
            id: parsed.id,
            id_lookups,
            reactive_binds: parsed.reactive_binds,
            layout_binds: parsed.layout_binds,
            children: my_children,
        });

        self.stack.last_mut().unwrap().push(cmd);
    }

    fn inscribe_if(
        &mut self,
        condition: &syn::Expr,
        reactive: bool,
        children: &[DsTreeRef],
        else_branch: Option<&DsTreeRef>,
    ) {
        use xrune::ds_node::node_enum::DsNode;

        let mut branches = vec![Branch {
            cond: Some(condition.clone()),
            body: self.collect_children(children),
        }];

        let mut next = else_branch.cloned();
        while let Some(tree) = next {
            let node_children = tree.borrow().get_children().to_vec();
            let chain_next = tree.borrow().get_else_branch().cloned();
            let cond = match tree.borrow().get_node() {
                DsNode::If(if_node) => Some(if_node.get_condition().clone()),
                _ => None,
            };
            branches.push(Branch {
                cond,
                body: self.collect_children(&node_children),
            });
            next = chain_next;
        }

        let cmd = Cmd::If(IfCmd { branches, reactive });
        self.stack.last_mut().unwrap().push(cmd);
    }

    fn inscribe_iter(
        &mut self,
        iterable: &syn::Expr,
        variable: &syn::Ident,
        reactive: bool,
        key: Option<&syn::Expr>,
        children: &[DsTreeRef],
    ) {
        self.stack.push(Vec::new());
        for child in children {
            decipher(child, self);
        }
        let body = self.stack.pop().unwrap();

        let cmd = Cmd::Iter(IterCmd {
            iterable: quote! { #iterable },
            variable: variable.clone(),
            body,
            reactive,
            key: key.map(|k| quote! { #k }),
        });

        self.stack.last_mut().unwrap().push(cmd);
    }

    fn inscribe_niche(&mut self, name: &syn::Ident, is_declaration: bool, children: &[DsTreeRef]) {
        let parent_widget = self.widget_kind_stack.last().and_then(|w| w.clone());

        self.stack.push(Vec::new());
        for child in children {
            decipher(child, self);
        }
        let body = self.stack.pop().unwrap();

        let mold_slot_var = if is_declaration && self.mold_mode {
            Some(syn::Ident::new(
                &format!("__mold_slot_{}", name),
                name.span(),
            ))
        } else {
            None
        };

        let cmd = Cmd::Niche(NicheCmd {
            name: name.clone(),
            is_declaration,
            mold_slot_var,
            parent_widget,
            body,
        });
        self.stack.last_mut().unwrap().push(cmd);
    }

    fn inscribe_code_block(&mut self, tokens: &proc_macro2::TokenStream) {
        self.stack
            .last_mut()
            .unwrap()
            .push(Cmd::CodeBlock(tokens.clone()));
    }

    fn inscribe_match(
        &mut self,
        scrutinee: &syn::Expr,
        reactive: bool,
        arms: &[xrune::ds_node::ds_match::DsMatchArm],
    ) {
        let mut arm_cmds = Vec::with_capacity(arms.len());
        for arm in arms {
            self.stack.push(Vec::new());
            for child in arm.get_children() {
                decipher(child, self);
            }
            let body = self.stack.pop().unwrap();
            arm_cmds.push(MatchArm {
                pat: arm.get_pat().clone(),
                body,
            });
        }

        let cmd = Cmd::Match(MatchCmd {
            scrutinee: scrutinee.clone(),
            reactive,
            arms: arm_cmds,
        });
        self.stack.last_mut().unwrap().push(cmd);
    }

    fn seal(self) -> proc_macro2::TokenStream {
        let world = &self.world_expr;
        let parent_entity = &self.parent_expr;
        let mut tokens = proc_macro2::TokenStream::new();

        let root_cmds = &self.stack[0];
        let mut last_var = None;
        for (cmd, statement) in
            root_cmds
                .iter()
                .zip(Self::emit_siblings(root_cmds, world, parent_entity))
        {
            tokens.extend(statement);
            match cmd {
                Cmd::Widget(w) => {
                    last_var = Some(w.var.clone());
                }
                Cmd::Compose(compose) => last_var = Some(compose.var.clone()),
                _ => {}
            }
        }

        // Return the top-level widget entity
        if let Some(var) = last_var {
            quote! { { #tokens #var } }
        } else {
            quote! { { #tokens } }
        }
    }
}

/// Compose a widget tree with an implicit UI scope.
///
/// Declare bound Model parameters with `model!(T)` or `model!(optional T)`.
/// The caller supplies the Model instance; composition does not create or look
/// up an instance. Model bindings are shared with reactive properties, event
/// closures, and child compositions.
///
/// ```no_run
/// # use mirui::prelude::*;
/// # use mirui::ui::widgets::{Button, Text};
/// #[model]
/// struct Counter {
///     #[observe]
///     count: u32,
/// }
///
/// #[model]
/// impl Counter {
///     fn increment(&mut self) {
///         self.count += 1;
///     }
/// }
///
/// #[compose]
/// fn counter_panel(counter: model!(Counter)) -> Entity {
///     ui! {
///         Column () {
///             Text (text: ${ counter.count().to_string() })
///             Button (text: "Increment") on Tap { counter.increment(); }
///         }
///     }
/// }
/// ```
///
/// Calling the function creates a stack-owned composition closure without executing its body. Use `app.compose(parent, counter_panel(counter))` to execute it in the application's construction scope. Parameter patterns, generic bounds, and concrete result types belong to the execution body. A function used as a child node must return its root `Entity`; `ui!(helper(...))` executes the closure in the current scope and forwards its result.
///
/// Resource expressions borrow the current application's storage through `res!(T).with(...)`, `res!(optional T).with(...)`, or `res!(mut T).with(...)`. Closure outputs cannot retain resource references. `res!().insert(value)` and `res!().remove::<T>()` perform immediate setup operations; callback `commands!` remains deferred.
///
/// Compute resource-backed values in the Rust body before passing them to `ui!` or another macro. Resource expressions do not expand inside another macro's token stream.
///
/// The attribute takes no arguments and applies to synchronous functions without a receiver. The parameter name `cx` is reserved for the generated scope. Use `ui!(compose Name { ... })` to declare a reusable Niche widget instead of a composition function.
#[proc_macro_attribute]
pub fn compose(attr: TokenStream, item: TokenStream) -> TokenStream {
    compose_attr::expand(attr.into(), item.into()).into()
}

/// Build widget trees, call composition functions, or declare Niche widgets.
///
/// The macro accepts three forms, distinguished by the shape of the input:
///
/// - **Tree form** (`ui! { <tree> }`) builds widgets in the enclosing
///   composition. Lowercase composition functions can appear directly as
///   child nodes; each returns a root entity attached at that position:
///
///   ```no_run
///   # use mirui::prelude::*;
///   # use mirui::ui::widgets::Text;
///   #[compose]
///   fn header(title: &'static str) -> Entity {
///       ui! {
///           Text (title)
///       }
///   }
///
///   #[compose]
///   fn build_root() -> Entity {
///       ui! {
///           Column (grow: 1.0) {
///               header("Status")
///               Text("hello")
///           }
///       }
///   }
///   ```
///
/// - **Function-call form** (`ui!(func(args))`) calls a composition function
///   using the current scope, without adding a surrounding widget:
///
///   ```no_run
///   # use mirui::prelude::*;
///   # use mirui::ui::widgets::Text;
///   #[compose]
///   fn menu_row(title: &'static str) {
///       ui! {
///           Text (title)
///       };
///   }
///
///   #[compose]
///   fn menu() {
///       ui!(menu_row("File"));
///       ui!(menu_row("Edit"));
///   }
///   ```
///
///   Function names start with a lowercase letter or underscore. Uppercase
///   names such as `Card(...)` and `Column(...)` denote widget nodes.
///
/// - **Niche form** (`ui!(compose Name { ... })`) declares a reusable widget
///   with named content slots. `@@body` declares a slot; `@body` fills it in
///   an instance. Register its descriptor with `ui!(compose Name)`:
///
///   ```no_run
///   # use mirui::prelude::*;
///   # use mirui::ui::widgets::Text;
///   ui!(compose Card {
///       Column () { @@body }
///   });
///
///   #[compose]
///   fn card_content() -> Entity {
///       ui! {
///           Card () {
///               @body { Text ("Status") }
///           }
///       }
///   }
///
///   let mut app = App::headless(320, 240);
///   app.with_widget(ui!(compose Card));
///   ```
#[proc_macro]
pub fn ui(input: TokenStream) -> TokenStream {
    let input2: proc_macro2::TokenStream = input.into();
    let (captures, input2) = match strip_bound_captures(input2) {
        Ok(value) => value,
        Err(error) => return error.to_compile_error().into(),
    };

    if let Some(after) = try_strip_compose_keyword(&input2) {
        return mold::expand(after).into();
    }

    if let Some(call) = try_parse_fn_call_form(&input2) {
        return expand_ui_fn_call(call).into();
    }

    let input2 = match crate::layout_reactive::preprocess(input2) {
        Ok(input) => input,
        Err(error) => return error.to_compile_error().into(),
    };
    let tree = match syn::parse2::<DsTree>(input2) {
        Ok(tree) => tree.into_ref(),
        Err(e) => return e.to_compile_error().into(),
    };
    let mut rune = MiruiRune::new();

    rune.world_expr = scope_world_expr();
    rune.parent_expr = quote! { __mirui_parent };

    decipher(&tree, &mut rune);
    let body = rune.seal();

    let parent_bind = quote! {
        let __mirui_parent: ::mirui::ecs::Entity = cx.parent();
    };

    let output = quote! {
        {
            #parent_bind
            #body
        }
    };
    if captures.is_empty() {
        return output.into();
    }
    let mut output: syn::Expr = match syn::parse2(output) {
        Ok(output) => output,
        Err(error) => return error.to_compile_error().into(),
    };
    BoundCaptureRewriter { captures }.visit_expr_mut(&mut output);
    quote!(#output).into()
}

fn strip_bound_captures(
    input: proc_macro2::TokenStream,
) -> syn::Result<(Vec<syn::Ident>, proc_macro2::TokenStream)> {
    use proc_macro2::{Delimiter, TokenTree};
    let mut iter = input.clone().into_iter();
    let Some(TokenTree::Ident(marker)) = iter.next() else {
        return Ok((Vec::new(), input));
    };
    if marker != "__mirui_bind" {
        return Ok((Vec::new(), input));
    }
    let Some(TokenTree::Group(group)) = iter.next() else {
        return Err(syn::Error::new_spanned(marker, "expected bound names"));
    };
    if group.delimiter() != Delimiter::Parenthesis {
        return Err(syn::Error::new_spanned(group, "expected bound names"));
    }
    let names = syn::parse::Parser::parse2(
        syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated,
        group.stream(),
    )?;
    let Some(TokenTree::Punct(end)) = iter.next() else {
        return Err(syn::Error::new_spanned(
            marker,
            "expected `;` after bound names",
        ));
    };
    if end.as_char() != ';' {
        return Err(syn::Error::new_spanned(
            end,
            "expected `;` after bound names",
        ));
    }
    Ok((names.into_iter().collect(), iter.collect()))
}

struct BoundCaptureRewriter {
    captures: Vec<syn::Ident>,
}

impl BoundCaptureRewriter {
    fn wrap(&self, expr: &mut syn::Expr) {
        let captures: Vec<_> = self
            .captures
            .iter()
            .filter(|name| FreeCaptureUse::contains(expr, name))
            .collect();
        if captures.is_empty() {
            return;
        }
        let original = core::mem::replace(expr, syn::parse_quote!(()));
        *expr = syn::parse_quote!({
            #(let #captures = ::mirui::core::model::SharedValue::share(&#captures);)*
            #original
        });
    }

    fn wrap_event(&self, expr: &mut syn::Expr) {
        let captures: Vec<_> = self
            .captures
            .iter()
            .filter(|name| FreeCaptureUse::contains(expr, name))
            .collect();
        if captures.is_empty() {
            return;
        }
        let owners: Vec<_> = captures
            .iter()
            .map(|name| {
                quote::format_ident!(
                    "__mirui_capture_owner_{name}",
                    span = proc_macro2::Span::mixed_site()
                )
            })
            .collect();
        let original = core::mem::replace(expr, syn::parse_quote!(()));
        let callback = syn::Ident::new("__mirui_bound_callback", proc_macro2::Span::mixed_site());
        let registered = owners.iter().fold(
            quote!(#callback),
            |value, owner| quote!(#value.require_binding(&#owner)),
        );
        *expr = syn::parse_quote!({
            #(let #owners = ::mirui::core::model::SharedValue::share(&#captures);)*
            let #callback = {
                #(let #captures = ::mirui::core::model::SharedValue::share(&#captures);)*
                #original
            };
            #registered
        });
    }
}

struct FreeCaptureUse<'a> {
    name: &'a syn::Ident,
    found: bool,
    shadowed: bool,
}

impl<'a> FreeCaptureUse<'a> {
    fn contains(expr: &syn::Expr, name: &'a syn::Ident) -> bool {
        use syn::visit::Visit;
        let mut visitor = Self {
            name,
            found: false,
            shadowed: false,
        };
        visitor.visit_expr(expr);
        visitor.found
    }

    fn pattern_binds(&self, pat: &syn::Pat) -> bool {
        use syn::visit::Visit;

        struct BindingFinder<'a> {
            name: &'a syn::Ident,
            found: bool,
        }

        impl<'ast> Visit<'ast> for BindingFinder<'_> {
            fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
                if pat.ident == *self.name {
                    self.found = true;
                }
                syn::visit::visit_pat_ident(self, pat);
            }
        }

        let mut finder = BindingFinder {
            name: self.name,
            found: false,
        };
        finder.visit_pat(pat);
        finder.found
    }

    fn visit_condition(&mut self, condition: &syn::Expr) -> bool {
        use syn::visit::Visit;

        match condition {
            syn::Expr::Let(let_expr) => {
                self.visit_expr(&let_expr.expr);
                self.pattern_binds(&let_expr.pat)
            }
            syn::Expr::Binary(binary) if matches!(binary.op, syn::BinOp::And(_)) => {
                let left_binds = self.visit_condition(&binary.left);
                let was_shadowed = self.shadowed;
                self.shadowed |= left_binds;
                let right_binds = self.visit_condition(&binary.right);
                self.shadowed = was_shadowed;
                left_binds || right_binds
            }
            _ => {
                self.visit_expr(condition);
                false
            }
        }
    }

    fn visit_format_macro(&mut self, macro_call: &syn::Macro) -> bool {
        use syn::parse::Parser;
        use syn::visit::Visit;

        let segments = &macro_call.path.segments;
        let standard_path = segments.len() == 1
            || (macro_call.path.leading_colon.is_some()
                && segments.len() == 2
                && segments.first().is_some_and(|segment| {
                    segment.ident == "std" || segment.ident == "alloc" || segment.ident == "core"
                }));
        if !standard_path
            || !segments
                .last()
                .is_some_and(|segment| segment.ident == "format" || segment.ident == "format_args")
        {
            return false;
        }
        let parser = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
        let Ok(arguments) = parser.parse2(macro_call.tokens.clone()) else {
            return false;
        };
        let Some(syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(literal),
            ..
        })) = arguments.first()
        else {
            return false;
        };
        let mut explicitly_named = false;
        for argument in arguments.iter().skip(1) {
            if let syn::Expr::Assign(assign) = argument
                && let syn::Expr::Path(path) = &*assign.left
                && path.qself.is_none()
                && path.path.is_ident(self.name)
            {
                explicitly_named = true;
                self.visit_expr(&assign.right);
            } else {
                self.visit_expr(argument);
            }
        }
        if !self.shadowed
            && !explicitly_named
            && format_literal_uses_ident(&literal.value(), &self.name.to_string())
        {
            self.found = true;
        }
        true
    }
}

impl<'ast> syn::visit::Visit<'ast> for FreeCaptureUse<'_> {
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        if !self.shadowed && path.qself.is_none() && path.path.is_ident(self.name) {
            self.found = true;
        }
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        let was_shadowed = self.shadowed;
        for statement in &block.stmts {
            match statement {
                syn::Stmt::Local(local) => {
                    if let Some(init) = &local.init {
                        self.visit_expr(&init.expr);
                        if let Some((_, diverge)) = &init.diverge {
                            self.visit_expr(diverge);
                        }
                    }
                    self.shadowed |= self.pattern_binds(&local.pat);
                }
                syn::Stmt::Item(_) => {}
                syn::Stmt::Expr(expr, _) => self.visit_expr(expr),
                syn::Stmt::Macro(statement) => self.visit_macro(&statement.mac),
            }
        }
        self.shadowed = was_shadowed;
    }

    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        let was_shadowed = self.shadowed;
        self.shadowed |= closure.inputs.iter().any(|pat| self.pattern_binds(pat));
        self.visit_expr(&closure.body);
        self.shadowed = was_shadowed;
    }

    fn visit_expr_for_loop(&mut self, loop_expr: &'ast syn::ExprForLoop) {
        self.visit_expr(&loop_expr.expr);
        let was_shadowed = self.shadowed;
        self.shadowed |= self.pattern_binds(&loop_expr.pat);
        self.visit_block(&loop_expr.body);
        self.shadowed = was_shadowed;
    }

    fn visit_expr_if(&mut self, if_expr: &'ast syn::ExprIf) {
        let binds = self.visit_condition(&if_expr.cond);
        let was_shadowed = self.shadowed;
        self.shadowed |= binds;
        self.visit_block(&if_expr.then_branch);
        self.shadowed = was_shadowed;
        if let Some((_, else_branch)) = &if_expr.else_branch {
            self.visit_expr(else_branch);
        }
    }

    fn visit_expr_while(&mut self, while_expr: &'ast syn::ExprWhile) {
        let binds = self.visit_condition(&while_expr.cond);
        let was_shadowed = self.shadowed;
        self.shadowed |= binds;
        self.visit_block(&while_expr.body);
        self.shadowed = was_shadowed;
    }

    fn visit_expr_match(&mut self, match_expr: &'ast syn::ExprMatch) {
        self.visit_expr(&match_expr.expr);
        for arm in &match_expr.arms {
            let was_shadowed = self.shadowed;
            self.shadowed |= self.pattern_binds(&arm.pat);
            if let Some((_, guard)) = &arm.guard {
                self.visit_condition(guard);
            }
            self.visit_expr(&arm.body);
            self.shadowed = was_shadowed;
        }
    }

    fn visit_macro(&mut self, macro_call: &'ast syn::Macro) {
        use syn::parse::Parser;

        if self.visit_format_macro(macro_call) {
            return;
        }

        // `matches!` takes a pattern after its first argument.
        if macro_call
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "matches")
            && let Ok(input) = syn::parse2::<MatchesMacroInput>(macro_call.tokens.clone())
        {
            self.visit_expr(&input.value);
            if let Some(guard) = &input.guard {
                let was_shadowed = self.shadowed;
                self.shadowed |= self.pattern_binds(&input.pattern);
                self.visit_expr(guard);
                self.shadowed = was_shadowed;
            }
            return;
        }

        let arguments = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
        if let Ok(arguments) = arguments.parse2(macro_call.tokens.clone()) {
            for argument in &arguments {
                self.visit_expr(argument);
            }
        } else if !self.shadowed && token_stream_uses_ident(macro_call.tokens.clone(), self.name) {
            self.found = true;
        }
    }
}

fn format_literal_uses_ident(literal: &str, name: &str) -> bool {
    let bytes = literal.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'{' {
            index += 1;
            continue;
        }
        if bytes.get(index + 1) == Some(&b'{') {
            index += 2;
            continue;
        }
        let start = index + 1;
        let Some(close) = bytes[start..].iter().position(|byte| *byte == b'}') else {
            break;
        };
        let end = start + close;
        let field = &literal[start..end];
        let argument = field.split([':', '!']).next().unwrap_or_default();
        if argument == name {
            return true;
        }
        if let Some((_, format_spec)) = field.split_once(':') {
            for (offset, _) in format_spec.match_indices(name) {
                let prefix = &format_spec[..offset];
                let suffix = &format_spec[offset + name.len()..];
                let begins_identifier = prefix
                    .chars()
                    .next_back()
                    .is_some_and(|character| character.is_alphanumeric() || character == '_');
                if !begins_identifier && suffix.starts_with('$') {
                    return true;
                }
            }
        }
        index = end + 1;
    }
    false
}

struct MatchesMacroInput {
    value: syn::Expr,
    pattern: syn::Pat,
    guard: Option<syn::Expr>,
}

impl syn::parse::Parse for MatchesMacroInput {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let value = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let pattern = syn::Pat::parse_multi_with_leading_vert(input)?;
        let guard = if input.peek(syn::Token![if]) {
            input.parse::<syn::Token![if]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        if input.peek(syn::Token![,]) {
            input.parse::<syn::Token![,]>()?;
        }
        Ok(Self {
            value,
            pattern,
            guard,
        })
    }
}

fn token_stream_uses_ident(stream: proc_macro2::TokenStream, name: &syn::Ident) -> bool {
    use proc_macro2::TokenTree;

    let tokens: Vec<_> = stream.into_iter().collect();
    tokens.iter().enumerate().any(|(index, token)| match token {
        TokenTree::Ident(ident) if ident == name => {
            let is_member = index > 0
                && matches!(&tokens[index - 1], TokenTree::Punct(punct) if punct.as_char() == '.');
            let is_path = (index > 0
                && matches!(&tokens[index - 1], TokenTree::Punct(punct) if punct.as_char() == ':'))
                || tokens.get(index + 1).is_some_and(
                    |next| matches!(next, TokenTree::Punct(punct) if punct.as_char() == ':'),
                );
            !is_member && !is_path
        }
        TokenTree::Group(group) => token_stream_uses_ident(group.stream(), name),
        _ => false,
    })
}

impl syn::visit_mut::VisitMut for BoundCaptureRewriter {
    fn visit_local_mut(&mut self, local: &mut syn::Local) {
        syn::visit_mut::visit_local_mut(self, local);
        if !matches!(&local.pat, syn::Pat::Ident(pattern)
            if pattern.ident == "__compose_factory")
        {
            return;
        }
        let Some(initializer) = &mut local.init else {
            return;
        };
        let syn::Expr::Call(call) = &mut *initializer.expr else {
            return;
        };
        for argument in &mut call.args {
            let syn::Expr::Path(path) = argument else {
                continue;
            };
            let Some(name) = path.path.get_ident() else {
                continue;
            };
            if self.captures.contains(name) {
                *argument = syn::parse_quote!(::mirui::core::model::SharedValue::share(&#name));
            }
        }
    }

    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        syn::visit_mut::visit_expr_mut(self, expr);
        let generated_event = matches!(expr, syn::Expr::MethodCall(call)
            if matches!(call.method.to_string().as_str(),
                "__bind_callback" | "__bind_plain" | "__bind_deferred" | "__bind_params" | "__bind_params_deferred")
                && matches!(&*call.receiver, syn::Expr::Path(path)
                    if path.path.is_ident("__mirui_event_key")));
        if generated_event {
            self.wrap_event(expr);
        }
    }

    fn visit_expr_call_mut(&mut self, call: &mut syn::ExprCall) {
        syn::visit_mut::visit_expr_call_mut(self, call);
        let syn::Expr::Path(path) = &*call.func else {
            return;
        };
        if !path.path.segments.last().is_some_and(|segment| {
            segment.ident == "effect_with_widget" || segment.ident == "prepare_widget_effect"
        }) {
            return;
        }
        if let Some(closure @ syn::Expr::Closure(_)) = call.args.last_mut() {
            self.wrap(closure);
        }
    }
}

fn try_strip_compose_keyword(input: &proc_macro2::TokenStream) -> Option<proc_macro2::TokenStream> {
    let mut iter = input.clone().into_iter();
    let first = iter.next()?;
    let ident = match first {
        proc_macro2::TokenTree::Ident(i) if i == "compose" => i,
        _ => return None,
    };
    let _ = ident;
    Some(iter.collect())
}

fn try_parse_fn_call_form(input: &proc_macro2::TokenStream) -> Option<syn::ExprCall> {
    let call: syn::ExprCall = syn::parse2(input.clone()).ok()?;
    let path = match &*call.func {
        syn::Expr::Path(p) => p,
        _ => return None,
    };
    let last = path.path.segments.last()?;
    let name = last.ident.to_string();
    let first_char = name.chars().next()?;
    if first_char.is_ascii_lowercase() || first_char == '_' {
        Some(call)
    } else {
        None
    }
}

fn scope_world_expr() -> proc_macro2::TokenStream {
    quote! {{
        use ::mirui::__macro_support::ScopeWorldAccess as _;
        cx.__mirui_world()
    }}
}

fn expand_ui_fn_call(call: syn::ExprCall) -> proc_macro2::TokenStream {
    let func = &call.func;
    let args = &call.args;
    quote! {
        (#func(#args))(cx)
    }
}

#[proc_macro]
pub fn compose_backend(input: TokenStream) -> TokenStream {
    compose::expand(input.into()).into()
}

#[proc_macro]
pub fn path(input: TokenStream) -> TokenStream {
    vector::expand_path(input.into()).into()
}

#[proc_macro]
pub fn scene(input: TokenStream) -> TokenStream {
    vector::expand_scene(input.into()).into()
}

/// Declares a timer body and its schedule.
///
/// ```rust,ignore
/// #[timer(every = 3_000)]
/// fn cycle(model: model!(CycleModel), timer: entity!(), commands: commands!()) {
///     model.advance();
///     commands.insert(timer, CycleMarker);
/// }
///
/// app.add_timer(cycle::timer(model));
/// ```
#[proc_macro_attribute]
pub fn timer(args: TokenStream, item: TokenStream) -> TokenStream {
    timer_attr::expand(args.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declares the typed update body for a motion component.
///
/// ```rust,ignore
/// #[animate(component = AnimateX)]
/// fn apply_x(value: motion!(), style: component!(mut Style)) {
///     style.layout.left = Dimension::Px(value);
/// }
///
/// app.add_system(AnimateX::system());
/// ```
#[proc_macro_attribute]
pub fn animate(args: TokenStream, item: TokenStream) -> TokenStream {
    animate_attr::expand(args.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Mints unique guard idents for `trace_span!` so multiple calls in
/// the same scope don't shadow each other. Overflow at 2³² is
/// theoretical only.
static TRACE_SPAN_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn next_trace_span_id() -> u32 {
    TRACE_SPAN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

mod trace_span_input {
    use syn::parse::{Parse, ParseStream};
    use syn::{Block, LitStr, Token};

    pub enum TraceSpanInput {
        Statement(LitStr),
        Expression(LitStr, Block),
    }

    impl Parse for TraceSpanInput {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            let name: LitStr = input.parse()?;
            if input.is_empty() {
                Ok(TraceSpanInput::Statement(name))
            } else {
                input.parse::<Token![,]>()?;
                let body: Block = input.parse()?;
                Ok(TraceSpanInput::Expression(name, body))
            }
        }
    }
}

/// `trace_span!("name")` — RAII statement: guard lives until end of
/// scope. Multiple calls in one scope each get a unique binding.
///
/// `trace_span!("name", { ... })` — block expression form,
/// evaluates to the block's value.
#[proc_macro]
pub fn trace_span(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as trace_span_input::TraceSpanInput);
    match parsed {
        trace_span_input::TraceSpanInput::Statement(name) => {
            let id = next_trace_span_id();
            let ident = quote::format_ident!("__trace_span_guard_{}", id);
            quote::quote! {
                let #ident = mirui::core::perf::enter(#name);
            }
            .into()
        }
        trace_span_input::TraceSpanInput::Expression(name, body) => {
            let id = next_trace_span_id();
            let ident = quote::format_ident!("__trace_span_guard_{}", id);
            quote::quote! {{
                let #ident = mirui::core::perf::enter(#name);
                let __trace_span_value = #body;
                drop(#ident);
                __trace_span_value
            }}
            .into()
        }
    }
}

/// `#[trace_fn("name")]` — equivalent to `trace_span!("name");` as
/// the first statement of the fn body.
#[proc_macro_attribute]
pub fn trace_fn(args: TokenStream, item: TokenStream) -> TokenStream {
    let name = parse_macro_input!(args as syn::LitStr);
    let mut func = parse_macro_input!(item as syn::ItemFn);
    let stmts = &func.block.stmts;
    let id = next_trace_span_id();
    let ident = quote::format_ident!("__trace_span_guard_{}", id);
    *func.block = syn::parse_quote! {{
        let #ident = mirui::core::perf::enter(#name);
        #(#stmts)*
    }};
    quote::quote! { #func }.into()
}

/// Declare a scheduled system using explicit model, resource, query, and
/// deferred-command roles.
///
/// The generated module shares the function name and exposes `system()` for
/// registration. Use `model!(T)` for registered model instances, `res!(...)`
/// for resources, `query!(...)` for component access, and `commands!()` for
/// deferred structural changes. `order` accepts a bare `SystemSlot` variant
/// such as `Animation`; omitting it selects `Normal`. The authored
/// function body cannot access `World` directly.
#[proc_macro_attribute]
pub fn system(args: TokenStream, item: TokenStream) -> TokenStream {
    system_attr::expand(args.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declare a typed event callback with installation-time Model and Copy value
/// arguments. The generated module exposes `callback(...)`; resources, queries,
/// the payload, the current entity, and commands are provided at invocation.
/// Payload parameters use `event!(E)`. Omitting the payload role leaves the event
/// type to the binding site. `entity!()` requires an entity-capable route.
#[proc_macro_attribute]
pub fn event(args: TokenStream, item: TokenStream) -> TokenStream {
    event_attr::expand(args.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[cfg(test)]
mod typed_on_tests {
    use super::*;

    fn expand_tree(input: proc_macro2::TokenStream) -> syn::Expr {
        let (captures, input) = strip_bound_captures(input).unwrap();
        let tree = syn::parse2::<DsTree>(input).unwrap().into_ref();
        let mut rune = MiruiRune::new();
        rune.world_expr = scope_world_expr();
        rune.parent_expr = quote!(__parent);
        decipher(&tree, &mut rune);
        let body = rune.seal();
        let mut output = syn::parse2(quote!({ #body })).unwrap();
        BoundCaptureRewriter { captures }.visit_expr_mut(&mut output);
        output
    }

    #[test]
    fn selectors_options_and_callback_forms_use_one_typed_installation_path() {
        let output = expand_tree(quote! {
            Column() {
                Button("Go") on Tap(next_count(), callback::callback())
                View() [Pad] on Pad::Hit(|input| input.value > 0)
                Alias() on Tap { event.x > Fixed::ZERO }
                Row() on Tap(|input: event!(Tap), state: res!(mut State)| {
                    state.0 += input.x;
                    false
                })
            }
        });
        let output = quote!(#output).to_string();
        assert_eq!(output.matches("install_entity_event").count(), 4);
        assert_eq!(output.matches("next_count").count(), 1);
        assert!(output.contains("Button :: Tap"));
        assert!(output.contains("Pad :: Hit"));
        assert!(output.contains("Alias :: Tap"));
        assert!(output.contains("mirui :: ui :: Widget :: Tap"));
        assert!(output.contains("__bind_plain"));
        assert!(output.contains("__bind_params"));
        assert!(output.contains("__bind_callback"));
        assert!(output.contains("NodeContext"));
        assert!(output.contains("callback :: callback ()"));
        assert!(!output.contains("GestureHandler"));
        assert!(!output.contains("BusinessCallback"));
        assert!(!output.contains("EventCommandQueue"));
    }

    #[test]
    fn declared_lexical_model_captures_are_shared_and_registered_before_invocation() {
        let output = expand_tree(quote! {
            __mirui_bind(game, other);
            Column() {
                Button("Go") on Tap { game.increment(); }
                Button("Role") on Tap(|input: event!(Tap), state: res!(mut State)| {
                    state.0 += input.x;
                    game.increment();
                })
                Button("Shadow") on Tap(|game| { let _ = game; })
                Button("Named") on Tap(handler::callback(other.clone()))
            }
        });
        let output = quote!(#output).to_string();
        assert_eq!(output.matches("__mirui_capture_owner_game =").count(), 2);
        assert_eq!(
            output
                .matches("require_binding (& __mirui_capture_owner_game)")
                .count(),
            2
        );
        assert_eq!(output.matches("__mirui_capture_owner_other =").count(), 1);
        assert!(output.contains("handler :: callback (other . clone ())"));
        assert!(!output.contains("__mirui_event_world"));
    }

    #[test]
    fn child_blocks_precede_callback_forms_on_the_same_widget() {
        let output = expand_tree(quote! {
            View() [Pad] {
                child_panel()
            } on Tap(|input: event!(Tap), rows: query!(Pad), node: entity!()| {
                rows.get(node).is_some() && input.x > Fixed::ZERO
            }) on DragStart(|input| input.x > Fixed::ZERO)
        });
        let output = quote!(#output).to_string();
        assert_eq!(output.matches("install_entity_event").count(), 2);
        assert!(output.contains("child_panel"));
        assert!(output.contains("__bind_params"));
        assert!(output.contains("__bind_plain"));
        assert!(!output.contains(". configure"));
    }

    #[test]
    fn compose_children_share_declared_models_before_factory_execution() {
        let output = expand_tree(quote! {
            __mirui_bind(game);
            Column() {
                child_panel(game, label)
                child_panel(game, label)
            }
        });
        let output = quote!(#output).to_string();
        assert_eq!(
            output
                .matches("__compose_factory = child_panel (:: mirui :: core :: model :: SharedValue :: share (& game) , label)")
                .count(),
            2,
        );
        assert_eq!(
            output
                .matches("__compose_factory (& mut __compose_cx)")
                .count(),
            2,
        );
        assert!(!output.contains("child_panel (& mut __compose_cx"));
    }

    #[test]
    fn compose_call_form_executes_factory_and_preserves_result_expression() {
        let call = syn::parse2(quote!(helpers::value::<u8>(source, 7))).unwrap();
        let output = expand_ui_fn_call(call);
        let expression: syn::Expr = syn::parse2(output).unwrap();
        assert_eq!(
            quote!(#expression).to_string(),
            quote!((helpers::value::<u8>(source, 7))(cx)).to_string(),
        );
    }
}

#[cfg(test)]
mod bound_capture_tests {
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
}

#[cfg(test)]
mod widget_kind_tests {
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
}
