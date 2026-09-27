extern crate proc_macro;

mod component_attr;
mod compose;
mod compose_attr;
mod diag;
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

#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    let item = proc_macro2::TokenStream::from(item);
    component_attr::expand(attr, item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
mod mold;

#[proc_macro_attribute]
pub fn model(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    let item = proc_macro2::TokenStream::from(item);
    model_attr::expand(attr, item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

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

struct EventComponentReads;

impl VisitMut for EventComponentReads {
    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        if let syn::Expr::Macro(mac) = expr
            && mac
                .mac
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "com")
            && let Ok(ComponentRead { entity, component }) =
                syn::parse2::<ComponentRead>(mac.mac.tokens.clone())
        {
            *expr = syn::parse_quote!(ctx.component::<#component>(#entity));
            return;
        }
        syn::visit_mut::visit_expr_mut(self, expr);
    }
}

fn event_component_reads(body: &syn::Block) -> syn::Block {
    let mut body = body.clone();
    EventComponentReads.visit_block_mut(&mut body);
    body
}

/// Read one component from the active `#[compose]` or `on Event` context.
/// The read does not subscribe to later changes.
#[proc_macro]
pub fn com(input: TokenStream) -> TokenStream {
    let ComponentRead { entity, component } = parse_macro_input!(input as ComponentRead);
    quote! { cx.component::<#component>(#entity) }.into()
}

use xrune::ds_node::ds_attr::DsAttr;
use xrune::ds_node::{DsRoot, DsTreeRef};
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

#[allow(dead_code)]
const BUILTIN_COMPONENT_NAMES: &[&str] = &[
    "Button",
    "Checkbox",
    "ProgressBar",
    "Slider",
    "Switch",
    "TabBar",
    "TextInput",
    "Image",
    "Icon",
    "Text",
    "LazyList",
    "MirrorOf",
    "BackgroundBlur",
    "TemporalMix",
];

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

fn is_known_gesture_event(name: &str) -> bool {
    matches!(
        name,
        "Tap"
            | "LongPress"
            | "DragStart"
            | "DragMove"
            | "DragEnd"
            | "DragCancel"
            | "Pinch"
            | "Rotate"
    )
}

struct BusinessEventEntry {
    widget: &'static str,
    event: &'static str,
    handler_component: &'static str,
    event_path: &'static str,
    fields: &'static [&'static str],
}

const FIRST_PARTY_BUSINESS_EVENTS: &[BusinessEventEntry] = &[
    BusinessEventEntry {
        widget: "Slider",
        event: "ValueChanged",
        handler_component: "::mirui::ui::widgets::slider::SliderHandler",
        event_path: "::mirui::ui::widgets::slider::SliderEvent::ValueChanged",
        fields: &["new", "old"],
    },
    BusinessEventEntry {
        widget: "Slider",
        event: "DragStarted",
        handler_component: "::mirui::ui::widgets::slider::SliderHandler",
        event_path: "::mirui::ui::widgets::slider::SliderEvent::DragStarted",
        fields: &[],
    },
    BusinessEventEntry {
        widget: "Slider",
        event: "DragEnded",
        handler_component: "::mirui::ui::widgets::slider::SliderHandler",
        event_path: "::mirui::ui::widgets::slider::SliderEvent::DragEnded",
        fields: &[],
    },
    BusinessEventEntry {
        widget: "Switch",
        event: "Toggled",
        handler_component: "::mirui::ui::widgets::switch::SwitchHandler",
        event_path: "::mirui::ui::widgets::switch::SwitchEvent::Toggled",
        fields: &["now"],
    },
    BusinessEventEntry {
        widget: "Checkbox",
        event: "Toggled",
        handler_component: "::mirui::ui::widgets::checkbox::CheckboxHandler",
        event_path: "::mirui::ui::widgets::checkbox::CheckboxEvent::Toggled",
        fields: &["now"],
    },
    BusinessEventEntry {
        widget: "ProgressBar",
        event: "ValueChanged",
        handler_component: "::mirui::ui::widgets::progress_bar::ProgressBarHandler",
        event_path: "::mirui::ui::widgets::progress_bar::ProgressBarEvent::ValueChanged",
        fields: &["new", "old"],
    },
    BusinessEventEntry {
        widget: "TabBar",
        event: "SelectionChanged",
        handler_component: "::mirui::ui::widgets::tabbar::TabBarHandler",
        event_path: "::mirui::ui::widgets::tabbar::TabBarEvent::SelectionChanged",
        fields: &["new", "old"],
    },
    BusinessEventEntry {
        widget: "TextInput",
        event: "Changed",
        handler_component: "::mirui::ui::widgets::text_input::TextInputHandler",
        event_path: "::mirui::ui::widgets::text_input::TextInputEvent::Changed",
        fields: &["len"],
    },
];

fn lookup_business_event(widget: &str, event: &str) -> Option<&'static BusinessEventEntry> {
    FIRST_PARTY_BUSINESS_EVENTS
        .iter()
        .find(|e| e.widget == widget && e.event == event)
}

fn gesture_event_fields(name: &str) -> &'static [&'static str] {
    match name {
        "Tap" | "LongPress" | "DragStart" | "DragCancel" => &["x", "y", "target"],
        "DragMove" => &["x", "y", "dx", "dy", "target"],
        "DragEnd" => &["x", "y", "vx", "vy", "target"],
        "Pinch" => &["x", "y", "scale_delta", "target"],
        "Rotate" => &["x", "y", "angle", "target"],
        _ => &[],
    }
}

fn emit_business_handler(
    widget_var: &syn::Ident,
    handler_component: &str,
    group: &[(&'static BusinessEventEntry, &OnCmd)],
    world: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let handler_path: syn::Path = syn::parse_str(handler_component).unwrap();

    let mut arms = proc_macro2::TokenStream::new();
    for (entry, on_cmd) in group {
        let body = event_component_reads(&on_cmd.body);
        let event_path: syn::Path = syn::parse_str(entry.event_path).unwrap();
        let field_idents: Vec<syn::Ident> = entry
            .fields
            .iter()
            .map(|f| syn::Ident::new(f, proc_macro2::Span::call_site()))
            .collect();
        let pattern = if field_idents.is_empty() {
            quote! { #event_path }
        } else {
            quote! { #event_path { #(#field_idents),* } }
        };
        let bindings = if field_idents.is_empty() {
            quote! {}
        } else {
            quote! { let _ = ( #(#field_idents),* ); }
        };
        arms.extend(quote! {
            #pattern => {
                #bindings
                #[allow(unused_mut, unused_variables)]
                let mut ctx = ::mirui::input::event::HandlerCtx {
                    world: __world,
                    entity: __entity,
                    event: __event,
                };
                let __consumed: bool =
                    ::mirui::input::event::HandlerReturn::into_consumed({ #body });
                __consumed
            },
        });
    }

    let event_ty: syn::Path = {
        let mut ty: syn::Path = syn::parse_str(group[0].0.event_path).unwrap();
        ty.segments.pop();
        if let Some(pair) = ty.segments.pop() {
            ty.segments.push_value(pair.into_value());
        }
        ty
    };

    quote! {
        (#world).insert(
            #widget_var,
            #handler_path {
                on_event: ::mirui::input::event::BusinessCallback::Closure(::mirui::__Rc::new(
                    move |__world: &mut ::mirui::ecs::World,
                          __entity: ::mirui::ecs::Entity,
                          __event: &#event_ty|
                          -> bool {
                        match __event {
                            #arms
                            #[allow(unreachable_patterns)]
                            _ => false,
                        }
                    },
                )),
            },
        );
    }
}

fn emit_event_arm(event_name: &str, group: &[&OnCmd]) -> proc_macro2::TokenStream {
    let event_ident = syn::Ident::new(event_name, proc_macro2::Span::call_site());
    let fields = gesture_event_fields(event_name);
    let field_idents: Vec<syn::Ident> = fields
        .iter()
        .map(|f| syn::Ident::new(f, proc_macro2::Span::call_site()))
        .collect();

    if event_name == "Tap" && group.iter().any(|h| !h.args.is_empty()) {
        return emit_tap_with_count(group, &field_idents);
    }

    let bodies = group.iter().map(|h| event_component_reads(&h.body));
    let used_idents: Vec<&syn::Ident> = field_idents.iter().collect();

    quote! {
        ::mirui::input::event::gesture::GestureEvent::#event_ident { #(#field_idents),* } => {
            let _ = ( #( #used_idents ),* );
            #(
                {
                    #[allow(unused_mut, unused_variables)]
                    let mut ctx = ::mirui::input::event::HandlerCtx {
                        world: __world,
                        entity: __entity,
                        event: __event,
                    };
                    let __consumed: bool =
                        ::mirui::input::event::HandlerReturn::into_consumed({ #bodies });
                    if __consumed { return true; }
                }
            )*
            false
        },
    }
}

fn emit_tap_with_count(group: &[&OnCmd], field_idents: &[syn::Ident]) -> proc_macro2::TokenStream {
    let mut count_arms = proc_macro2::TokenStream::new();
    let mut default_arm: Option<proc_macro2::TokenStream> = None;
    for h in group {
        let body = event_component_reads(&h.body);
        if h.args.is_empty() {
            default_arm = Some(quote! {
                _ => {
                    #[allow(unused_mut, unused_variables)]
                    let mut ctx = ::mirui::input::event::HandlerCtx {
                        world: __world,
                        entity: __entity,
                        event: __event,
                    };
                    let __consumed: bool =
                        ::mirui::input::event::HandlerReturn::into_consumed({ #body });
                    return __consumed;
                }
            });
        } else if h.args.len() == 1 {
            let arg = &h.args[0];
            count_arms.extend(quote! {
                __c if __c == (#arg as u8) => {
                    #[allow(unused_mut, unused_variables)]
                    let mut ctx = ::mirui::input::event::HandlerCtx {
                        world: __world,
                        entity: __entity,
                        event: __event,
                    };
                    let __consumed: bool =
                        ::mirui::input::event::HandlerReturn::into_consumed({ #body });
                    return __consumed;
                },
            });
        }
    }
    let default = default_arm.unwrap_or_else(|| quote! { _ => return false });

    quote! {
        ::mirui::input::event::gesture::GestureEvent::Tap { #(#field_idents),* } => {
            let _ = ( #(#field_idents),* );
            let __count = ::mirui::input::event::multi_tap::current_count(__world, __entity);
            match __count {
                #count_arms
                #default
            }
        },
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
    args: Vec<syn::Expr>,
    body: syn::Block,
}

impl OnCmd {
    fn synthesised_body_from_callback(callback: &syn::Expr) -> syn::Block {
        syn::parse_quote! {
            {
                (#callback)(&mut ctx)
            }
        }
    }
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
        "normal_color" if widget == "Button" => Some("ButtonNormalColor"),
        "render_key" => Some("RenderKey"),
        "font_size" => Some("FontSize"),
        "direction" => Some("Direction"),
        "width" => Some("Width"),
        "height" => Some("Height"),
        "paragraph" if widget == "Text" => Some("Paragraph"),
        "path" if widget == "Text" => Some("TextPath"),
        "value" if widget == "ProgressBar" => Some("ProgressValue"),
        "value" if widget == "Slider" => Some("SliderValue"),
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
    id_registrations: Vec<proc_macro2::TokenStream>,
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
    id_registrations: Vec<proc_macro2::TokenStream>,
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
        let mut id_registrations = Vec::new();
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
                    Some(s) => id_registrations.push(quote! { #s }),
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
            id_registrations,
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
    ) -> proc_macro2::TokenStream {
        match cmd {
            Cmd::Widget(w) => Self::emit_widget(w, world),
            Cmd::Compose(c) => Self::emit_compose(c, world, parent_var),
            Cmd::Iter(i) => Self::emit_iter(i, world, parent_var),
            Cmd::If(i) => Self::emit_if(i, world, parent_var),
            Cmd::Niche(n) => Self::emit_niche(n, world, parent_var),
            Cmd::Match(m) => Self::emit_match(m, world, parent_var),
            Cmd::CodeBlock(ts) => quote! { #ts },
        }
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
                let mut __compose_cx = mirui::ui::UiScope::new(#world, __compose_parent);
                #function(&mut __compose_cx #(, #args)*)
            };
        }
    }

    fn emit_on_dispatch(
        widget_name: &str,
        handlers: &[&OnCmd],
        widget_var: &syn::Ident,
        world: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let mut errors = proc_macro2::TokenStream::new();
        let mut gesture_arms: std::collections::BTreeMap<String, Vec<&OnCmd>> =
            std::collections::BTreeMap::new();
        let mut business_arms: std::collections::BTreeMap<
            &'static str,
            Vec<(&'static BusinessEventEntry, &OnCmd)>,
        > = std::collections::BTreeMap::new();

        for h in handlers {
            let event_name = h.name.to_string();

            if let Some(q) = h.qualifier.as_ref() {
                let q_str = q.to_string();
                if q_str != widget_name {
                    errors.extend(
                        syn::Error::new(
                            q.span(),
                            format!(
                                "qualified `on {q_str}::{event_name}` does not match enclosing \
                                 widget `{widget_name}`; drop the qualifier or rename the widget"
                            ),
                        )
                        .to_compile_error(),
                    );
                    continue;
                }
                match lookup_business_event(widget_name, &event_name) {
                    Some(entry) => {
                        business_arms
                            .entry(entry.handler_component)
                            .or_default()
                            .push((entry, *h));
                    }
                    None => {
                        errors.extend(
                            syn::Error::new(
                                h.name.span(),
                                format!(
                                    "no business event named `{event_name}` is registered for \
                                     widget `{widget_name}`"
                                ),
                            )
                            .to_compile_error(),
                        );
                    }
                }
                continue;
            }

            if let Some(entry) = lookup_business_event(widget_name, &event_name) {
                if !h.args.is_empty() {
                    errors.extend(
                        syn::Error::new(
                            h.name.span(),
                            format!("business event `{event_name}` does not take arguments"),
                        )
                        .to_compile_error(),
                    );
                    continue;
                }
                business_arms
                    .entry(entry.handler_component)
                    .or_default()
                    .push((entry, *h));
                continue;
            }

            if !is_known_gesture_event(&event_name) {
                errors.extend(
                    syn::Error::new(
                        h.name.span(),
                        format!(
                            "unknown event `{event_name}` for widget `{widget_name}`; \
                             expected a GestureEvent variant (Tap, LongPress, DragStart, \
                             DragMove, DragEnd, DragCancel, Pinch, Rotate) or a business event"
                        ),
                    )
                    .to_compile_error(),
                );
                continue;
            }
            if !h.args.is_empty() && event_name != "Tap" {
                errors.extend(
                    syn::Error::new(
                        h.name.span(),
                        format!(
                            "parameters on `on {event_name}(...)` are reserved for a \
                             later release; only `on Tap(n)` is parameterised in v0.27.x"
                        ),
                    )
                    .to_compile_error(),
                );
                continue;
            }
            gesture_arms.entry(event_name).or_default().push(*h);
        }

        let mut tokens = proc_macro2::TokenStream::new();
        tokens.extend(errors);

        if !gesture_arms.is_empty() {
            let mut arms = proc_macro2::TokenStream::new();
            for (event_name, group) in &gesture_arms {
                arms.extend(emit_event_arm(event_name, group));
            }
            // A closure (not a fn item) so the body can capture handles like a
            // Signal from the surrounding scope.
            tokens.extend(quote! {
                (#world).insert(#widget_var, ::mirui::ui::HitTarget);
                (#world).insert(
                    #widget_var,
                    ::mirui::input::event::GestureHandler {
                        on_gesture: ::mirui::input::event::GestureCallback::Closure(::mirui::__Rc::new(
                            move |__world: &mut ::mirui::ecs::World,
                                  __entity: ::mirui::ecs::Entity,
                                  __event: &::mirui::input::event::gesture::GestureEvent|
                                  -> bool {
                                match __event {
                                    #arms
                                    _ => false,
                                }
                            },
                        )),
                    },
                );
            });
        }

        for (handler_component, group) in &business_arms {
            tokens.extend(emit_business_handler(
                widget_var,
                handler_component,
                group,
                world,
            ));
        }

        tokens
    }

    fn emit_widget(cmd: &WidgetCmd, world: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
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

        tokens.extend(quote! {
            let #var = mirui::ui::builder::WidgetBuilder::new(#world)
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

            tokens.extend(quote! {
                mirui::input::event::widget_input::attach_handlers_for(#world, #var);
            });
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
                mirui::core::reactive::with_world_scope(#world, || {
                    #apply
                });
            });
        }

        if !cmd.reactive_binds.is_empty() {
            let injections: Vec<proc_macro2::TokenStream> = cmd
                .reactive_binds
                .iter()
                .map(|b| {
                    let property = &b.property;
                    let expr = &b.expr;
                    if property == "TextContent"
                        && let Some(capacity) = &cmd.text_capacity
                    {
                        let apply = direct_format_args_call(
                            &b.format_expr,
                            &quote! { #var },
                            capacity,
                        )
                        .unwrap_or_else(|| quote! {
                            mirui::ui::property::apply_bounded_text::<{ #capacity }>(
                                #var,
                                ::core::format_args!("{}", #expr),
                            );
                        });
                        quote! {
                            mirui::core::reactive::effect_with_widget(#var, move || {
                                #apply
                            });
                        }
                    } else {
                        let value = if property == "TextContent" {
                            quote! { mirui::ui::property::IntoText::into_text(__v) }
                        } else {
                            quote! { ::core::convert::Into::into(__v) }
                        };
                        quote! {
                            mirui::core::reactive::effect_with_widget(#var, move || {
                                let __v = #expr;
                                mirui::ui::property::apply::<mirui::ui::property::prop::#property>(#var, #value);
                            });
                        }
                    }
                })
                .collect();
            // with_world_scope makes the effects' first run apply the initial
            // value now, at construction (outside the per-frame flush).
            tokens.extend(quote! {
                mirui::core::reactive::with_world_scope(#world, || { #(#injections)* });
            });
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
                        ::mirui::ui::LayoutBinding::new(
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

        // Emit enchants — insert components
        let enchants = &cmd.enchants;
        for enchant in enchants {
            tokens.extend(quote! {
                (#world).insert(#var, #enchant);
            });
        }

        if !on_handlers.is_empty() {
            tokens.extend(Self::emit_on_dispatch(
                &cmd.name.to_string(),
                &on_handlers,
                &cmd.var,
                world,
            ));
        }

        for id_lit in &cmd.id_registrations {
            tokens.extend(quote! {
                (#world).insert(#var, mirui::ui::NamedId(#id_lit));
                if let Some(__map) = (#world).resource_mut::<mirui::ui::IdMap>() {
                    __map.insert(#id_lit, #var);
                }
            });
        }

        let var_ts = quote! { #var };
        for child in &cmd.children {
            tokens.extend(Self::emit_cmd(child, world, &var_ts));
            if let Cmd::Widget(widget) = child {
                let child_var = &widget.var;
                tokens.extend(quote! {
                    {
                        use mirui::ui::{Children, Parent};
                        (#world).insert(#child_var, Parent(#var));
                        if let Some(children) = (#world).get_mut::<Children>(#var) {
                            children.0.push(#child_var);
                        }
                    }
                });
            }
        }

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
                        .map_or_else(mirui::__Vec::new, |children| children.0[__start..].to_vec())
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
                let __selected = mirui::__Cell::new(None::<usize>);
                mirui::core::reactive::with_world_scope(#world, || {
                    mirui::core::reactive::effect_with_widget(__parent_e, move || {
                        let __next = Some(match #read {
                            #(#select_arms)*
                        });
                        let __previous = __selected.get();
                        if __previous != __next {
                            mirui::core::reactive::with_world(|__w| {
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
                let __mounted = mirui::__RefCell::new(mirui::__Vec::<mirui::ecs::Entity>::new());
                let __parent_e = #parent_var;
                let __mount_index = (#world)
                    .get::<mirui::ui::Children>(__parent_e)
                    .map_or(0, |children| children.0.len());
                mirui::core::reactive::with_world_scope(#world, || {
                    mirui::core::reactive::effect_with_widget(__parent_e, move || {
                        let __sel = #read;
                        mirui::core::reactive::with_world(|__w| {
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
            let mut body_tokens = proc_macro2::TokenStream::new();
            for child in &arm.body {
                body_tokens.extend(Self::emit_cmd(child, world, parent_var));
                if let Cmd::Widget(w) = child {
                    let child_var = &w.var;
                    body_tokens.extend(quote! {
                        {
                            use mirui::ui::{Children, Parent};
                            (#world).insert(#child_var, Parent(#parent_var));
                            if let Some(children) = (#world).get_mut::<Children>(#parent_var) {
                                children.0.push(#child_var);
                            }
                        }
                    });
                }
            }
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
                let mut fallback = proc_macro2::TokenStream::new();
                for child in &cmd.body {
                    fallback.extend(Self::emit_cmd(child, world, &slot_var_ts));
                    if let Cmd::Widget(w) = child {
                        let child_var = &w.var;
                        fallback.extend(quote! {
                            {
                                use mirui::ui::{Children, Parent};
                                (#world).insert(#child_var, Parent(#slot_var_ts));
                                if let Some(children) = (#world).get_mut::<Children>(#slot_var_ts) {
                                    children.0.push(#child_var);
                                }
                            }
                        });
                    }
                }
                return quote! {
                    let __slot_parent = #parent_var;
                    let #slot_var = ::mirui::ui::builder::WidgetBuilder::new(#world).id();
                    (#world).insert(#slot_var, ::mirui::ui::Parent(__slot_parent));
                    if let Some(children) = (#world).get_mut::<::mirui::ui::Children>(__slot_parent) {
                        children.0.push(#slot_var);
                    } else {
                        (#world).insert(__slot_parent, ::mirui::ui::Children(::mirui::__Vec::from([#slot_var])));
                    }
                    __mold_niche_map.insert(#slot_name, #slot_var);
                    #fallback
                };
            }
            let msg = format!(
                "ui!: `@@{}` is a slot declaration; use it inside `mold!` bodies, not `ui!` — call sites fill slots with the single-`@` form `@{}` {{ ... }}",
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

        let mut body_tokens = proc_macro2::TokenStream::new();
        for child in &cmd.body {
            body_tokens.extend(Self::emit_cmd(child, world, &niche_var_ts));
            if let Cmd::Widget(w) = child {
                let child_var = &w.var;
                body_tokens.extend(quote! {
                    {
                        use mirui::ui::{Children, Parent};
                        (#world).insert(#child_var, Parent(#niche_var_ts));
                        if let Some(children) = (#world).get_mut::<Children>(#niche_var_ts) {
                            children.0.push(#child_var);
                        }
                    }
                });
            }
        }

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
                let __existing: ::mirui::__Vec<::mirui::ecs::Entity> = (#world)
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

        let mut body_tokens = proc_macro2::TokenStream::new();
        for child in &cmd.body {
            body_tokens.extend(Self::emit_cmd(child, world, parent_var));
            if let Cmd::Widget(w) = child {
                let child_var = &w.var;
                body_tokens.extend(quote! {
                    {
                        use mirui::ui::{Children, Parent};
                        (#world).insert(#child_var, Parent(#parent_var));
                        if let Some(children) = (#world).get_mut::<Children>(#parent_var) {
                            children.0.push(#child_var);
                        }
                    }
                });
            }
        }

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
        let mut body_tokens = proc_macro2::TokenStream::new();
        for child in body {
            body_tokens.extend(Self::emit_cmd(child, world, parent_var));
            if let Cmd::Widget(w) = child {
                let child_var = &w.var;
                body_tokens.extend(quote! {
                    {
                        use mirui::ui::{Children, Parent};
                        (#world).insert(#child_var, Parent(#parent_var));
                        if let Some(children) = (#world).get_mut::<Children>(#parent_var) {
                            children.0.push(#child_var);
                        }
                    }
                });
            }
        }
        body_tokens
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
        for child in body {
            stmts.extend(Self::emit_cmd(child, &w, &p));
            let child_var = match child {
                Cmd::Widget(cw) => Some((&cw.var, false)),
                Cmd::Compose(compose) => Some((&compose.var, true)),
                _ => None,
            };
            if let Some((cv, already_attached)) = child_var {
                if root.is_none() {
                    let detach = already_attached.then(|| {
                        quote! {
                            if let Some(children) = (#w).get_mut::<mirui::ui::Children>(#p) {
                                if let Some(index) = children.0.iter().position(|entity| *entity == #cv) {
                                    children.0.remove(index);
                                }
                            }
                        }
                    });
                    stmts.extend(quote! {
                        #detach
                        (#w).insert(#cv, mirui::ui::Parent(#p));
                    });
                    root = Some(quote! { #cv });
                } else if !already_attached {
                    stmts.extend(quote! {
                        {
                            use mirui::ui::{Children, Parent};
                            (#w).insert(#cv, Parent(#p));
                            if let Some(children) = (#w).get_mut::<Children>(#p) {
                                children.0.push(#cv);
                            }
                        }
                    });
                }
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
                        .map_or_else(mirui::__Vec::new, |children| children.0[__start..].to_vec())
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
                let __selected = mirui::__Cell::new(None::<usize>);
                mirui::core::reactive::with_world_scope(#world, || {
                    mirui::core::reactive::effect_with_widget(__parent_e, move || {
                        let __next: ::core::option::Option<usize> = #cascade;
                        let __previous = __selected.get();
                        if __previous != __next {
                            mirui::core::reactive::with_world(|__w| {
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
                let __rows = mirui::__Rc::new(mirui::__RefCell::new(
                    mirui::__Vec::<mirui::ecs::Entity>::new(),
                ));
                let __parent_e = #parent_var;
                mirui::core::reactive::with_world_scope(#world, || {
                    mirui::core::reactive::effect_with_widget(__parent_e, move || {
                        let __items: mirui::__Vec<_> = (#iterable).into_iter().collect();
                        mirui::core::reactive::with_world(|__w| {
                            let __n_new = __items.len();
                            let __n_old = __rows.borrow().len();
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
                            for #variable in __items.into_iter().skip(__n_old) {
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
                let __rows = mirui::__Rc::new(mirui::__RefCell::new(mirui::__Vec::new()));
                let __parent_e = #parent_var;
                mirui::core::reactive::with_world_scope(#world, || {
                    mirui::core::reactive::effect_with_widget(__parent_e, move || {
                        let __items: mirui::__Vec<_> = (#iterable).into_iter().collect();
                        mirui::core::reactive::with_world(|__w| {
                            let mut __old: mirui::__Vec<_> = __rows.borrow_mut().drain(..).collect();
                            let mut __new = mirui::__Vec::new();
                            let mut __kept: mirui::__Vec<mirui::ecs::Entity> = mirui::__Vec::new();
                            for #variable in __items {
                                let __k = #key_expr;
                                if let Some(__i) = __old.iter().position(|(__ok, _)| *__ok == __k) {
                                    let (_, __e) = __old.remove(__i);
                                    __kept.push(__e);
                                    __new.push((__k, __e));
                                } else {
                                    let __r = (|__w: &mut mirui::ecs::World, __parent: mirui::ecs::Entity| -> Option<mirui::ecs::Entity> {
                                        #row_body
                                    })(__w, __parent_e);
                                    if let Some(__r) = __r {
                                        __kept.push(__r);
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
                                children.0.retain(|e| !__kept.contains(e));
                                for __e in &__kept {
                                    children.0.push(*__e);
                                }
                            }
                            *__rows.borrow_mut() = __new;
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

        let collected_on_handlers: Vec<OnCmd> = on_handlers
            .iter()
            .map(|h| {
                let mut args = h.get_args().to_vec();
                let body = match h.get_body() {
                    Some(b) => b.clone(),
                    None => {
                        let callback = args
                            .pop()
                            .expect("DsOn parser already rejects empty args+no-body forms");
                        OnCmd::synthesised_body_from_callback(&callback)
                    }
                };
                OnCmd {
                    qualifier: h.get_qualifier().cloned(),
                    name: h.get_name().clone(),
                    args,
                    body,
                }
            })
            .collect();

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
            id_registrations: parsed.id_registrations,
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
        for cmd in root_cmds {
            tokens.extend(Self::emit_cmd(cmd, world, parent_entity));
            match cmd {
                Cmd::Widget(w) => {
                    let var = &w.var;
                    last_var = Some(var.clone());
                    tokens.extend(quote! {
                        {
                            use mirui::ui::{Children, Parent};
                            let __seal_parent = #parent_entity;
                            (#world).insert(#var, Parent(__seal_parent));
                            if let Some(children) = (#world).get_mut::<Children>(__seal_parent) {
                                children.0.push(#var);
                            }
                        }
                    });
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

/// Inject a `cx: &mut UiScope` first parameter into the annotated function.
///
/// The attribute exists to keep infrastructure out of the signature the user
/// reads:
///
/// ```ignore
/// #[compose]
/// fn counter_button(sig: Signal<i32>) {
///     ui! {
///         View (bg_color: ColorToken::Primary)
///         on Tap { sig.update(|n| *n += 1); }
///         { Text($sig) }
///     };
/// }
/// ```
///
/// Every `ui!` invocation inside the function body reads `cx.world_mut()` /
/// `cx.parent()` implicitly; callers pass their existing `cx` through the
/// paired `ui!(func(args))` form so no site ever spells out `world` and
/// `parent` again.
///
/// The attribute rejects three shapes at expansion time:
///
/// - non-function items — annotate a fn, or reach for `mold!(Name { ... })`
///   when you want a full widget class.
/// - `async fn` — cross-await UI state is a separate topic (v0.41+).
/// - generic fn — parameter monomorphisation is deferred to a later release.
///
/// It also errors out if the user already declared a parameter named `cx`,
/// so double-injection can't silently pass through.
#[proc_macro_attribute]
pub fn compose(attr: TokenStream, item: TokenStream) -> TokenStream {
    compose_attr::expand(attr.into(), item.into()).into()
}

/// Spawn a widget tree, or forward to another `#[compose]` function.
///
/// The macro accepts three forms, distinguished by the shape of the input:
///
/// - **Body form** (`ui! { <tree> }`) reads `cx` from the enclosing scope
///   and spawns the DSL tree there:
///
///   ```ignore
///   #[compose]
///   fn build_root() {
///       ui! {
///           Column (grow: 1.0) {
///               Text("hello")
///           }
///       };
///   }
///   ```
///
///   Lowercase `#[compose]` functions can appear directly as tree nodes. Their
///   returned root entity is attached at that position:
///
///   ```ignore
///   ui! {
///       Column (grow: 1.0) {
///           compose_header()
///           compose_card("Status")
///       }
///   };
///   ```
///
///   `world` and `parent` come from `cx.world_mut()` / `cx.parent()` by
///   default. The legacy header form `ui! { :( parent world :) X }` still
///   parses and takes precedence over the implicit `cx` lookup — useful
///   for tests, one-off snapshots, or any code path that isn't inside an
///   `#[compose]` fn.
///
/// - **Fn-call form** (`ui!(func(args))`) rewrites a free-fn call to
///   `func(cx, args)`, threading the enclosing scope's `cx` into the callee
///   when no surrounding DSL node supplies the parent:
///
///   ```ignore
///   #[compose]
///   fn menu() {
///       ui!(menu_row("File", &FILE_ICON));
///       ui!(menu_row("Edit", &EDIT_ICON));
///   }
///   ```
///
///   The macro routes call-syntax input to this form only when the fn
///   path segment starts with a lowercase letter or underscore. Names
///   that start uppercase (`Card(...)`, `Column(...)`) stay on the DSL
///   parsing path — the same convention the widget grammar uses.
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
    let root = match syn::parse2::<DsRoot>(input2) {
        Ok(r) => r,
        Err(e) => return e.to_compile_error().into(),
    };
    let mut rune = MiruiRune::new();

    let context_attrs = root.get_context_attrs();
    let world_override = context_attrs
        .iter()
        .find(|a| a.name.as_ref().is_some_and(|n| n == "world"))
        .map(|a| {
            let v = &a.value;
            quote! { #v }
        });
    let parent_override = context_attrs
        .iter()
        .find(|a| a.name.as_ref().is_some_and(|n| n == "parent"))
        .map(|a| {
            let v = &a.value;
            quote! { #v }
        });

    rune.world_expr = world_override
        .clone()
        .unwrap_or_else(|| quote! { cx.world_mut() });
    rune.parent_expr = quote! { __mirui_parent };

    rune.inscribe_root(&root.get_parent());
    let content = root.get_content();
    decipher(&content, &mut rune);
    let body = rune.seal();

    let parent_bind = match parent_override {
        Some(expr) => quote! { let __mirui_parent: ::mirui::ecs::Entity = #expr; },
        None => quote! { let __mirui_parent: ::mirui::ecs::Entity = cx.parent(); },
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
    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        if let syn::Expr::Closure(closure) = expr {
            syn::visit_mut::visit_expr_closure_mut(self, closure);
            let generated_event = closure.capture.is_some()
                && closure.inputs.first().is_some_and(|input| {
                    matches!(input, syn::Pat::Type(typed)
                        if matches!(&*typed.pat, syn::Pat::Ident(ident) if ident.ident == "__world"))
                });
            if generated_event {
                self.wrap(expr);
            }
            return;
        }
        syn::visit_mut::visit_expr_mut(self, expr);
    }

    fn visit_expr_call_mut(&mut self, call: &mut syn::ExprCall) {
        syn::visit_mut::visit_expr_call_mut(self, call);
        let generated_compose_call = call.args.first().is_some_and(|arg| {
            matches!(arg, syn::Expr::Reference(reference)
                if reference.mutability.is_some()
                    && matches!(&*reference.expr, syn::Expr::Path(path)
                        if path.path.is_ident("__compose_cx")))
        });
        if generated_compose_call {
            for arg in call.args.iter_mut().skip(1) {
                let syn::Expr::Path(path) = arg else {
                    continue;
                };
                let Some(name) = path.path.get_ident() else {
                    continue;
                };
                if self.captures.contains(name) {
                    *arg = syn::parse_quote!(::mirui::core::model::SharedValue::share(&#name));
                }
            }
            return;
        }
        let syn::Expr::Path(path) = &*call.func else {
            return;
        };
        if !path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "effect_with_widget")
        {
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

fn expand_ui_fn_call(call: syn::ExprCall) -> proc_macro2::TokenStream {
    let func = &call.func;
    let args = &call.args;
    quote! {
        #func(cx, #args)
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

/// ```rust,ignore
/// timer!(Cycle, every: 3_000, |world, entity| { /* ... */ });
/// // schedule: after: ms | every: ms | repeat: N every: ms | until: D every: ms
/// ```
///
/// Sugar over `Timer::after / every / repeat / until`; all four share
/// the generic `timer_system`, so N invocations don't grow the binary.
#[proc_macro]
pub fn timer(input: TokenStream) -> TokenStream {
    timer_impl::expand(input.into()).into()
}

mod timer_impl {
    use proc_macro2::TokenStream;
    use quote::quote;
    use syn::parse::{Parse, ParseStream};
    use syn::{ExprClosure, Ident, LitInt, Token, parse2};

    enum Schedule {
        After(LitInt),
        Every(LitInt),
        Repeat { times: LitInt, period: LitInt },
        Until { deadline: LitInt, period: LitInt },
    }

    struct TimerInput {
        name: Ident,
        schedule: Schedule,
        closure: ExprClosure,
    }

    impl Parse for Schedule {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            let kind: Ident = input.parse()?;
            input.parse::<Token![:]>()?;
            let first: LitInt = input.parse()?;
            match kind.to_string().as_str() {
                "after" => Ok(Schedule::After(first)),
                "every" => Ok(Schedule::Every(first)),
                "repeat" => {
                    // `repeat: N every: M`
                    let kw: Ident = input.parse()?;
                    if kw != "every" {
                        return Err(syn::Error::new(
                            kw.span(),
                            "expected `every` after `repeat: N`",
                        ));
                    }
                    input.parse::<Token![:]>()?;
                    let period: LitInt = input.parse()?;
                    Ok(Schedule::Repeat {
                        times: first,
                        period,
                    })
                }
                "until" => {
                    // `until: deadline every: period`
                    let kw: Ident = input.parse()?;
                    if kw != "every" {
                        return Err(syn::Error::new(
                            kw.span(),
                            "expected `every` after `until: D`",
                        ));
                    }
                    input.parse::<Token![:]>()?;
                    let period: LitInt = input.parse()?;
                    Ok(Schedule::Until {
                        deadline: first,
                        period,
                    })
                }
                other => Err(syn::Error::new(
                    kind.span(),
                    format!(
                        "unknown schedule keyword `{other}`; expected after / every / repeat / until"
                    ),
                )),
            }
        }
    }

    impl Parse for TimerInput {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            let name: Ident = input.parse()?;
            input.parse::<Token![,]>()?;
            let schedule: Schedule = input.parse()?;
            input.parse::<Token![,]>()?;
            let closure: ExprClosure = input.parse()?;
            Ok(Self {
                name,
                schedule,
                closure,
            })
        }
    }

    pub fn expand(input: TokenStream) -> TokenStream {
        let parsed = match parse2::<TimerInput>(input) {
            Ok(v) => v,
            Err(e) => return e.to_compile_error(),
        };

        let name = &parsed.name;
        let closure = &parsed.closure;

        let ctor = match &parsed.schedule {
            Schedule::After(p) => quote! { mirui::core::timer::Timer::after(#p, __cb) },
            Schedule::Every(p) => quote! { mirui::core::timer::Timer::every(#p, __cb) },
            Schedule::Repeat { times, period } => {
                quote! { mirui::core::timer::Timer::repeat(#times, #period, __cb) }
            }
            Schedule::Until { deadline, period } => {
                quote! { mirui::core::timer::Timer::until(#deadline, #period, __cb) }
            }
        };

        quote! {
            pub struct #name;
            impl #name {
                pub fn install(world: &mut mirui::ecs::World) -> mirui::ecs::Entity {
                    let __cb: fn(&mut mirui::ecs::World, mirui::ecs::Entity) = #closure;
                    let e = world.spawn_empty();
                    world.insert(e, #ctor);
                    e
                }
            }
        }
    }
}

/// Define a motion component (Tween or Spring) + its tick/apply system.
///
/// ```rust,ignore
/// animate!(AnimateX, |world, entity, value| {
///     mirui::ui::set_position(world, entity, value, Fixed::from_int(2));
/// });
///
/// // Generated:
/// // - struct AnimateX(pub mirui::anim::Motion)
/// // - impl MotionComponent for AnimateX
/// // - AnimateX::system() -> fn(&mut World)
/// //
/// // Usage:
/// //   app.add_system(AnimateX::system());
/// //   world.insert(e, AnimateX(Tween::ease_to(from, to, 250).into()));
/// //   world.insert(e, AnimateX(Spring::preset(from, to, SMOOTH).into()));
/// ```
#[proc_macro]
pub fn animate(input: TokenStream) -> TokenStream {
    animate_impl::expand(input.into()).into()
}

mod animate_impl {
    use proc_macro2::TokenStream;
    use quote::quote;
    use syn::parse::{Parse, ParseStream};
    use syn::{ExprClosure, Ident, Token, parse2};

    struct AnimateInput {
        name: Ident,
        closure: ExprClosure,
    }

    impl Parse for AnimateInput {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            let name: Ident = input.parse()?;
            input.parse::<Token![,]>()?;
            let closure: ExprClosure = input.parse()?;
            Ok(Self { name, closure })
        }
    }

    pub fn expand(input: TokenStream) -> TokenStream {
        let parsed = match parse2::<AnimateInput>(input) {
            Ok(v) => v,
            Err(e) => return e.to_compile_error(),
        };

        let name = &parsed.name;
        let closure = &parsed.closure;

        quote! {
            pub struct #name(pub mirui::anim::Motion);

            impl mirui::anim::MotionComponent for #name {
                fn motion(&self) -> &mirui::anim::Motion { &self.0 }
                fn motion_mut(&mut self) -> &mut mirui::anim::Motion { &mut self.0 }
            }

            impl #name {
                pub fn system() -> mirui::ecs::System {
                    fn __sys(world: &mut mirui::ecs::World) {
                        mirui::anim::run_motion::<#name>(world, #closure);
                    }
                    mirui::ecs::System::new(
                        stringify!(#name),
                        mirui::ecs::run_order::ANIMATION,
                        __sys,
                    )
                }
            }
        }
    }
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

mod system_attr {
    use syn::parse::{Parse, ParseStream};
    use syn::{Expr, Ident, LitStr, Token, Type, bracketed, parenthesized, punctuated::Punctuated};

    pub struct SystemArgs {
        pub name: Option<LitStr>,
        pub order: Option<Expr>,
        /// Component type(s) gating this system. Empty = always runs.
        /// Multiple entries are OR-combined (any present triggers run).
        pub expect: Vec<Type>,
        pub bind: Vec<Ident>,
    }

    impl Parse for SystemArgs {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            let mut name: Option<LitStr> = None;
            let mut order: Option<Expr> = None;
            let mut expect: Vec<Type> = Vec::new();
            let mut bind: Vec<Ident> = Vec::new();
            while !input.is_empty() {
                let key: Ident = input.parse()?;
                if key == "bind" {
                    let content;
                    parenthesized!(content in input);
                    let names: Punctuated<Ident, Token![,]> =
                        content.parse_terminated(Ident::parse, Token![,])?;
                    if names.is_empty() {
                        return Err(syn::Error::new(
                            key.span(),
                            "bind requires a parameter name",
                        ));
                    }
                    for name in names {
                        if bind.contains(&name) {
                            return Err(syn::Error::new(name.span(), "duplicate bound parameter"));
                        }
                        bind.push(name);
                    }
                    if !input.is_empty() {
                        input.parse::<Token![,]>()?;
                    }
                    continue;
                }
                input.parse::<Token![=]>()?;
                match key.to_string().as_str() {
                    "name" => name = Some(input.parse()?),
                    "order" => order = Some(input.parse()?),
                    "expect" => {
                        if input.peek(syn::token::Bracket) {
                            let content;
                            bracketed!(content in input);
                            let types: Punctuated<Type, Token![,]> =
                                content.parse_terminated(Type::parse, Token![,])?;
                            expect.extend(types);
                        } else {
                            expect.push(input.parse()?);
                        }
                    }
                    other => {
                        return Err(syn::Error::new(
                            key.span(),
                            format!(
                                "unknown #[system] arg `{other}`; expected `name`, `order`, `expect`, or `bind(...)`",
                            ),
                        ));
                    }
                }
                if input.is_empty() {
                    break;
                }
                input.parse::<Token![,]>()?;
            }
            Ok(Self {
                name,
                order,
                expect,
                bind,
            })
        }
    }
}

/// Attach perf-aware metadata to a system function.
///
/// Generates a sibling module sharing the fn's ident exposing
/// `system()` returning a [`mirui::ecs::System`] with the configured
/// name + run_order slot. The fn itself is left intact so direct
/// calls (tests, manual invocation) still work.
///
/// ```ignore
/// #[mirui::system(order = ANIMATION)]
/// fn spin_system(world: &mut World) { /* ... */ }
///
/// spin_system(world);                     // direct call
/// app.add_system(spin_system::system());  // scheduled
/// ```
///
/// Defaults: `name` derives from the fn ident; `order` defaults to
/// `run_order::NORMAL`. `order` accepts either a `run_order::*`
/// constant or a literal `i32`. With `bind(model)`, a `&Model` parameter
/// receives the registered handle and other parameters read `Copy` resources.
#[proc_macro_attribute]
pub fn system(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as system_attr::SystemArgs);
    let func = parse_macro_input!(item as syn::ItemFn);
    let fn_ident = &func.sig.ident;
    let fn_vis = &func.vis;
    let name_lit = args
        .name
        .unwrap_or_else(|| syn::LitStr::new(&fn_ident.to_string(), fn_ident.span()));
    let order_expr: syn::Expr = match args.order {
        Some(e) => e,
        None => syn::parse_quote!(mirui::ecs::run_order::NORMAL),
    };
    let expect_const_ident =
        quote::format_ident!("__MIRUI_EXPECT_{}", fn_ident.to_string().to_uppercase());
    let expect_outer = if args.expect.is_empty() {
        quote::quote! {}
    } else {
        let entries = args.expect.iter().map(|ty| {
            quote::quote! { (::core::any::TypeId::of::<#ty>) as fn() -> ::core::any::TypeId }
        });
        quote::quote! {
            #[doc(hidden)]
            #[allow(non_upper_case_globals)]
            const #expect_const_ident: &[fn() -> ::core::any::TypeId] = &[ #(#entries),* ];
        }
    };
    let with_expect_call = if args.expect.is_empty() {
        quote::quote! {}
    } else {
        quote::quote! { .with_expect(super::#expect_const_ident) }
    };
    if !args.bind.is_empty() {
        return expand_bound_system(
            func,
            args.bind,
            name_lit,
            order_expr,
            expect_outer,
            with_expect_call,
        )
        .unwrap_or_else(syn::Error::into_compile_error)
        .into();
    }
    quote::quote! {
        #func
        #expect_outer

        #[allow(non_snake_case, non_camel_case_types)]
        #fn_vis mod #fn_ident {
            #[allow(unused_imports)]
            use mirui::ecs::run_order::*;
            pub const fn system() -> mirui::ecs::System {
                mirui::ecs::System::new(#name_lit, #order_expr, super::#fn_ident) #with_expect_call
            }
        }
    }
    .into()
}

fn expand_bound_system(
    mut func: syn::ItemFn,
    bind: Vec<syn::Ident>,
    name: syn::LitStr,
    order: syn::Expr,
    expect_outer: proc_macro2::TokenStream,
    with_expect: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let fn_ident = &func.sig.ident;
    let visibility = &func.vis;
    let mut bound = Vec::new();
    let mut resources = Vec::new();
    let mut call_args = Vec::new();
    for input in &mut func.sig.inputs {
        let syn::FnArg::Typed(param) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "systems cannot have a receiver",
            ));
        };
        let syn::Pat::Ident(pattern) = &*param.pat else {
            return Err(syn::Error::new_spanned(
                &param.pat,
                "system parameters must be named",
            ));
        };
        let ident = pattern.ident.clone();
        if bind.contains(&ident) {
            let syn::Type::Reference(reference) = &*param.ty else {
                return Err(syn::Error::new_spanned(
                    &param.ty,
                    "bound models use `&Model`",
                ));
            };
            if reference.mutability.is_some() {
                return Err(syn::Error::new_spanned(
                    &param.ty,
                    "bound models cannot be mutably borrowed",
                ));
            }
            let model_type = (*reference.elem).clone();
            *param.ty = syn::parse_quote!(
                &<#model_type as ::mirui::core::model::BindType>::Shared
            );
            bound.push((ident.clone(), model_type));
            call_args.push(quote::quote!(&#ident));
        } else {
            let resource_type = &param.ty;
            if matches!(&**resource_type, syn::Type::Reference(_)) {
                return Err(syn::Error::new_spanned(
                    resource_type,
                    "system resources must be Copy values",
                ));
            }
            let optional = match &**resource_type {
                syn::Type::Path(path) => path.path.segments.last().and_then(|segment| {
                    if segment.ident != "Option" {
                        return None;
                    }
                    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                        return None;
                    };
                    match arguments.args.first() {
                        Some(syn::GenericArgument::Type(inner)) => Some(inner.clone()),
                        _ => None,
                    }
                }),
                _ => None,
            };
            if let Some(inner) = optional {
                resources.push(quote::quote! {
                    let #ident = world.resource::<#inner>().copied();
                });
            } else {
                resources.push(quote::quote! {
                    let #ident = world.resource::<#resource_type>().copied()
                        .expect(concat!("missing system resource `", stringify!(#resource_type), "`"));
                });
            }
            call_args.push(quote::quote!(#ident));
        }
    }
    let ordered: Vec<_> = bind
        .iter()
        .map(|name| {
            bound
                .iter()
                .find(|(ident, _)| ident == name)
                .ok_or_else(|| syn::Error::new(name.span(), "bound name is not a system parameter"))
        })
        .collect::<syn::Result<_>>()?;
    let (first_name, _) = ordered
        .first()
        .expect("non-empty bind list was checked by parser");
    let inputs = ordered.iter().map(|(ident, model_type)| {
        quote::quote!(#ident: <#model_type as ::mirui::core::model::BindType>::Shared)
    });
    let captures = ordered.iter().map(
        |(ident, _)| quote::quote!(let #ident = ::mirui::core::model::SharedValue::share(&#ident);),
    );
    let other_models = ordered
        .iter()
        .skip(1)
        .map(|(ident, _)| quote::quote!(.with_model(&#ident)));
    Ok(quote::quote! {
        #func
        #expect_outer

        #[allow(non_snake_case, non_camel_case_types)]
        #visibility mod #fn_ident {
            #[allow(unused_imports)]
            use super::*;
            #[allow(unused_imports)]
            use ::mirui::ecs::run_order::*;
            pub fn system(#(#inputs),*) -> ::mirui::ecs::System {
                let callback = {
                    #(#captures)*
                    move |world: &mut ::mirui::ecs::World| {
                        #(#resources)*
                        super::#fn_ident(#(#call_args),*);
                    }
                };
                ::mirui::ecs::System::bound(#name, #order, &#first_name, callback)
                    #(#other_models)* #with_expect
            }
        }
    })
}

#[proc_macro_derive(Component)]
pub fn derive_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    component_attr::marker_impl(&input.ident, &input.generics).into()
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
