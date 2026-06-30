extern crate proc_macro;

mod compose;
mod diag;
mod vector;
mod visit_id;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

use xrune::ds_node::ds_attr::DsAttr;
use xrune::ds_node::{DsRoot, DsTreeRef};
use xrune::ds_rune::DsRune;
use xrune::ds_rune::decipher::decipher;

const LAYOUT_ATTRS: &[&str] = &[
    "width",
    "height",
    "grow",
    "direction",
    "justify",
    "align",
    "padding",
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
];

const RESERVED_LAYOUT_NAMES: &[&str] = &["View", "Row", "Column"];

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
        "Text" => Some("text"),
        "Image" => Some("texture"),
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
        "Tap" | "LongPress" | "DragStart" | "DragMove" | "DragEnd" | "Pinch" | "Rotate"
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
];

fn lookup_business_event(widget: &str, event: &str) -> Option<&'static BusinessEventEntry> {
    FIRST_PARTY_BUSINESS_EVENTS
        .iter()
        .find(|e| e.widget == widget && e.event == event)
}

fn gesture_event_fields(name: &str) -> &'static [&'static str] {
    match name {
        "Tap" | "LongPress" | "DragStart" => &["x", "y", "target"],
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
        let body = &on_cmd.body;
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

    let bodies = group.iter().map(|h| &h.body);
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
        let body = &h.body;
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
    Iter(IterCmd),
    If(IfCmd),
    Niche(NicheCmd),
    Match(MatchCmd),
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
    setter: syn::Ident,
    expr: proc_macro2::TokenStream,
}

fn reactive_read(value: &syn::Expr) -> proc_macro2::TokenStream {
    match value {
        syn::Expr::Path(p) => quote! { #p.get() },
        syn::Expr::Block(b) => quote! { #b },
        other => quote! { #other },
    }
}

fn reactive_setter(attr: &str) -> Option<&'static str> {
    match attr {
        "text" => Some("reactive_set_text"),
        "bg_color" => Some("reactive_set_bg_color"),
        "text_color" => Some("reactive_set_text_color"),
        "width" => Some("reactive_set_width"),
        "height" => Some("reactive_set_height"),
        _ => None,
    }
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
    id_registrations: Vec<proc_macro2::TokenStream>,
    id_lookups: Vec<(syn::Ident, String)>,
    reactive_binds: Vec<ReactiveBind>,
    children: Vec<Cmd>,
}

struct ParsedAttrs {
    builder_calls: Vec<proc_macro2::TokenStream>,
    layout_fields: Vec<proc_macro2::TokenStream>,
    errors: Vec<proc_macro2::TokenStream>,
    component_inserts: Vec<proc_macro2::TokenStream>,
    component_fields: Vec<proc_macro2::TokenStream>,
    text_tuple_value: Option<proc_macro2::TokenStream>,
    id_registrations: Vec<proc_macro2::TokenStream>,
    id_lookups: Vec<(syn::Ident, String)>,
    reactive_binds: Vec<ReactiveBind>,
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
    counter: usize,
}

impl MiruiRune {
    fn new() -> Self {
        Self {
            world_expr: quote! { __world },
            parent_expr: quote! { __parent },
            stack: vec![Vec::new()],
            counter: 0,
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
        let component_inserts = Vec::new();
        let mut component_fields = Vec::new();
        let mut text_tuple_value: Option<proc_macro2::TokenStream> = None;
        let mut id_registrations = Vec::new();
        let mut id_lookups: Vec<(syn::Ident, String)> = Vec::new();
        let mut reactive_binds: Vec<ReactiveBind> = Vec::new();
        let mut user_set_direction = false;

        let is_text_widget = widget_name == "Text";
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

            // Must run before the Text / Component `text` routes below, or a
            // reactive `text` gets frozen as static content instead of bound.
            if attr.reactive {
                match reactive_setter(&name) {
                    Some(setter) => {
                        reactive_binds.push(ReactiveBind {
                            setter: syn::Ident::new(setter, attr_span),
                            expr: reactive_read(value),
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

            if is_text_widget && name == "text" {
                text_tuple_value = Some(quote! { #value });
                continue;
            }

            if is_text_input_widget && TEXT_INPUT_FIELDS.contains(&name.as_str()) {
                let field_ident = syn::Ident::new(&name, attr_span);
                component_fields.push(quote! { #field_ident: (#value).into() });
                continue;
            }

            if widget_kind == WidgetKind::Component && name == "text" {
                let field_ident = syn::Ident::new(&name, attr_span);
                component_fields.push(quote! { #field_ident: (#value).into() });
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
                "width" => layout_fields.push(quote! { width: mirui::types::Dimension::Px(mirui::types::Fixed::from_int(#value as i32)) }),
                "height" => layout_fields.push(quote! { height: mirui::types::Dimension::Px(mirui::types::Fixed::from_int(#value as i32)) }),
                "grow" => layout_fields.push(quote! { grow: mirui::types::Fixed::from_f32(#value) }),
                "direction" => {
                    user_set_direction = true;
                    layout_fields.push(quote! { direction: #value });
                }
                "justify" => layout_fields.push(quote! { justify: #value }),
                "align" => layout_fields.push(quote! { align: #value }),
                "padding" => layout_fields.push(quote! { padding: #value }),
                "position" => layout_fields.push(quote! { position: #value }),
                "left" => layout_fields.push(quote! { left: mirui::types::Dimension::Px(mirui::types::Fixed::from_int(#value)) }),
                "top" => layout_fields.push(quote! { top: mirui::types::Dimension::Px(mirui::types::Fixed::from_int(#value)) }),
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
                        component_fields.push(quote! { #field_ident: (#value).into() });
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
            id_registrations,
            id_lookups,
            reactive_binds,
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

    /// Emit a Cmd, returning generated tokens.
    /// `parent_var` is the variable name of the parent widget that children attach to.
    fn emit_cmd(
        cmd: &Cmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        match cmd {
            Cmd::Widget(w) => Self::emit_widget(w, world),
            Cmd::Iter(i) => Self::emit_iter(i, world, parent_var),
            Cmd::If(i) => Self::emit_if(i, world, parent_var),
            Cmd::Niche(n) => Self::emit_niche(n, world, parent_var),
            Cmd::Match(m) => Self::emit_match(m, world, parent_var),
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
                             DragMove, DragEnd, Pinch, Rotate) or a business event"
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

        let var_ts = quote! { #var };
        let mut child_vars = Vec::new();
        let mut deferred_iters = Vec::new();
        let on_handlers: Vec<&OnCmd> = cmd.on_handlers.iter().collect();

        for child in &cmd.children {
            match child {
                Cmd::Widget(w) => {
                    tokens.extend(Self::emit_widget(w, world));
                    child_vars.push(&w.var);
                }
                Cmd::Iter(_) | Cmd::If(_) | Cmd::Niche(_) | Cmd::Match(_) => {
                    deferred_iters.push(child);
                }
            }
        }

        // Create this widget with static children
        let layout_call = if layout_fields.is_empty() {
            quote! {}
        } else {
            quote! { .layout(mirui::ui::layout::LayoutStyle { #(#layout_fields,)* ..Default::default() }) }
        };

        let child_calls: Vec<proc_macro2::TokenStream> =
            child_vars.iter().map(|c| quote! { .child(#c) }).collect();

        tokens.extend(quote! {
            let #var = mirui::ui::builder::WidgetBuilder::new(#world)
                #(#attrs)*
                #layout_call
                #(#child_calls)*
                .id();
        });

        // Must precede the reactive-bind injection: an effect's first run writes
        // into the component, so seeding it after would clobber that value.
        if cmd.kind == WidgetKind::Component {
            let comp_name = &cmd.name;
            if cmd.name == "Text" {
                let init = match &cmd.text_tuple_value {
                    Some(text_value) => {
                        quote! { ::core::convert::Into::<#comp_name>::into(#text_value) }
                    }
                    None => quote! { #comp_name::Owned(::mirui::__Cow::Borrowed("")) },
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
            }

            tokens.extend(quote! {
                mirui::input::event::widget_input::attach_handlers_for(#world, #var);
            });
        }

        if !cmd.reactive_binds.is_empty() {
            let injections: Vec<proc_macro2::TokenStream> = cmd
                .reactive_binds
                .iter()
                .map(|b| {
                    let setter = &b.setter;
                    let expr = &b.expr;
                    quote! {
                        mirui::core::reactive::effect_with_widget(#var, move || {
                            let __v = #expr;
                            mirui::ui::reactive_attr::#setter(#var, __v);
                        });
                    }
                })
                .collect();
            // with_world_scope makes the effects' first run apply the initial
            // value now, at construction (outside the per-frame flush).
            tokens.extend(quote! {
                mirui::core::reactive::with_world_scope(#world, || { #(#injections)* });
            });
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

        // Now emit iter children — they attach dynamically to this widget
        for iter_cmd in deferred_iters {
            tokens.extend(Self::emit_cmd(iter_cmd, world, &var_ts));
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
        let select_arms = cmd.arms.iter().map(|arm| {
            let pat = &arm.pat;
            let builder = Self::build_branch(&arm.body);
            quote! { #pat => (#builder)(__w, __parent_e), }
        });
        quote! {
            {
                let __mounted = mirui::__Rc::new(mirui::__Cell::new(
                    ::core::option::Option::<mirui::ecs::Entity>::None,
                ));
                let __parent_e = #parent_var;
                mirui::core::reactive::with_world_scope(#world, || {
                    mirui::core::reactive::effect_with_widget(__parent_e, move || {
                        let __sel = #read;
                        mirui::core::reactive::with_world(|__w| {
                            let __old = __mounted.take();
                            let __idx = __old.and_then(|o| {
                                __w.get::<mirui::ui::Children>(__parent_e)
                                    .and_then(|c| c.0.iter().position(|&e| e == o))
                            });
                            if let Some(__o) = __old {
                                mirui::ui::despawn_subtree(__w, __o);
                            }
                            let __root: ::core::option::Option<mirui::ecs::Entity> = match __sel {
                                #(#select_arms)*
                            };
                            if let Some(__r) = __root {
                                if let Some(children) =
                                    __w.get_mut::<mirui::ui::Children>(__parent_e)
                                {
                                    let __pos = __idx.unwrap_or(children.0.len()).min(children.0.len());
                                    children.0.insert(__pos, __r);
                                }
                            }
                            __mounted.set(__root);
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
        let niche_name = cmd.name.to_string();
        let niche_var = syn::Ident::new(
            &format!("__niche_{}", cmd.name),
            proc_macro2::Span::call_site(),
        );
        let niche_var_ts = quote! { #niche_var };

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

    // A reactive control-flow branch built on demand: returns the first top
    // widget (single-root) as the mount root, parented but NOT pushed into the
    // parent's Children — the effect inserts it positionally so a branch swap
    // keeps its index. Non-root top widgets push normally.
    fn branch_body(body: &[Cmd]) -> proc_macro2::TokenStream {
        let w = quote! { __w };
        let p = quote! { __parent };
        let mut stmts = proc_macro2::TokenStream::new();
        let mut root: Option<proc_macro2::TokenStream> = None;
        for child in body {
            stmts.extend(Self::emit_cmd(child, &w, &p));
            if let Cmd::Widget(cw) = child {
                let cv = &cw.var;
                if root.is_none() {
                    stmts.extend(quote! {
                        (#w).insert(#cv, mirui::ui::Parent(#p));
                    });
                    root = Some(quote! { #cv });
                } else {
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

    fn build_branch(body: &[Cmd]) -> proc_macro2::TokenStream {
        let inner = Self::branch_body(body);
        quote! {
            |__w: &mut mirui::ecs::World, __parent: mirui::ecs::Entity|
                -> Option<mirui::ecs::Entity> {
                #inner
            }
        }
    }

    fn emit_if_reactive(
        cmd: &IfCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let builder_idents: Vec<syn::Ident> = (0..cmd.branches.len())
            .map(|i| syn::Ident::new(&format!("__br{i}"), proc_macro2::Span::call_site()))
            .collect();
        let let_builders = cmd.branches.iter().zip(&builder_idents).map(|(b, id)| {
            let builder = Self::build_branch(&b.body);
            quote! { let #id = #builder; }
        });

        let mut cascade = proc_macro2::TokenStream::new();
        let mut first = true;
        let mut has_else = false;
        for (br, id) in cmd.branches.iter().zip(&builder_idents) {
            match &br.cond {
                Some(cond) => {
                    let read = reactive_read(cond);
                    let head = if first { quote!(if) } else { quote!(else if) };
                    cascade.extend(quote! { #head (#read) { (#id)(__w, __parent_e) } });
                    first = false;
                }
                None => {
                    cascade.extend(quote! { else { (#id)(__w, __parent_e) } });
                    has_else = true;
                }
            }
        }
        if !has_else {
            cascade.extend(quote! { else { ::core::option::Option::None } });
        }

        quote! {
            {
                let __mounted = mirui::__Rc::new(mirui::__Cell::new(
                    ::core::option::Option::<mirui::ecs::Entity>::None,
                ));
                let __parent_e = #parent_var;
                #(#let_builders)*
                mirui::core::reactive::with_world_scope(#world, || {
                    mirui::core::reactive::effect_with_widget(__parent_e, move || {
                        mirui::core::reactive::with_world(|__w| {
                            let __old = __mounted.take();
                            let __idx = __old.and_then(|o| {
                                __w.get::<mirui::ui::Children>(__parent_e)
                                    .and_then(|c| c.0.iter().position(|&e| e == o))
                            });
                            if let Some(__o) = __old {
                                mirui::ui::despawn_subtree(__w, __o);
                            }
                            let __root: ::core::option::Option<mirui::ecs::Entity> = #cascade;
                            if let Some(__r) = __root {
                                if let Some(children) =
                                    __w.get_mut::<mirui::ui::Children>(__parent_e)
                                {
                                    let __pos = __idx.unwrap_or(children.0.len()).min(children.0.len());
                                    children.0.insert(__pos, __r);
                                }
                            }
                            __mounted.set(__root);
                        });
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
                "Column" => {
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
        for child in children {
            decipher(child, self);
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
            id_registrations: parsed.id_registrations,
            id_lookups,
            reactive_binds: parsed.reactive_binds,
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

    fn inscribe_niche(&mut self, name: &syn::Ident, children: &[DsTreeRef]) {
        self.stack.push(Vec::new());
        for child in children {
            decipher(child, self);
        }
        let body = self.stack.pop().unwrap();

        let cmd = Cmd::Niche(NicheCmd {
            name: name.clone(),
            body,
        });
        self.stack.last_mut().unwrap().push(cmd);
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
        for cmd in root_cmds {
            tokens.extend(Self::emit_cmd(cmd, world, parent_entity));
        }

        // Attach top-level widgets to parent (iter attaches inside its own loop)
        let mut last_var = None;
        for cmd in root_cmds {
            if let Cmd::Widget(w) = cmd {
                let var = &w.var;
                last_var = Some(var.clone());
                tokens.extend(quote! {
                    {
                        use mirui::ui::{Children, Parent};
                        (#world).insert(#var, Parent(#parent_entity));
                        if let Some(children) = (#world).get_mut::<Children>(#parent_entity) {
                            children.0.push(#var);
                        }
                    }
                });
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

#[proc_macro]
pub fn ui(input: TokenStream) -> TokenStream {
    let root = parse_macro_input!(input as DsRoot);
    let mut rune = MiruiRune::new();

    let context_attrs = root.get_context_attrs();
    if let Some(world_attr) = context_attrs
        .iter()
        .find(|a| a.name.as_ref().is_some_and(|n| n == "world"))
    {
        let world_expr = &world_attr.value;
        rune.world_expr = quote! { #world_expr };
    } else {
        return syn::Error::new(proc_macro2::Span::call_site(), "missing `world` in context")
            .to_compile_error()
            .into();
    }

    let parent = root.get_parent();
    rune.parent_expr = quote! { #parent };

    rune.inscribe_root(&root.get_parent());
    let content = root.get_content();
    decipher(&content, &mut rune);
    TokenStream::from(rune.seal())
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
                pub fn system() -> fn(&mut mirui::ecs::World) {
                    fn __sys(world: &mut mirui::ecs::World) {
                        mirui::anim::run_motion::<#name>(world, #closure);
                    }
                    __sys
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
    use syn::{Expr, Ident, LitStr, Token, Type, bracketed, punctuated::Punctuated};

    pub struct SystemArgs {
        pub name: Option<LitStr>,
        pub order: Option<Expr>,
        /// Component type(s) gating this system. Empty = always runs.
        /// Multiple entries are OR-combined (any present triggers run).
        pub expect: Vec<Type>,
    }

    impl Parse for SystemArgs {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            let mut name: Option<LitStr> = None;
            let mut order: Option<Expr> = None;
            let mut expect: Vec<Type> = Vec::new();
            while !input.is_empty() {
                let key: Ident = input.parse()?;
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
                                "unknown #[system] arg `{other}`; expected `name`, `order`, or `expect`",
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
            })
        }
    }
}

/// Attach perf-aware metadata to a `fn(&mut World)`.
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
/// constant or a literal `i32`.
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

#[proc_macro_derive(Component)]
pub fn derive_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    quote! {
        impl #impl_generics ::mirui::ecs::Component for #name #ty_generics #where_clause {}
    }
    .into()
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
    }

    #[test]
    fn other_capital_names_are_component() {
        assert_eq!(classify_widget_name(&id("Button")), WidgetKind::Component);
        assert_eq!(classify_widget_name(&id("MyCard")), WidgetKind::Component);
        assert_eq!(classify_widget_name(&id("Checkbox")), WidgetKind::Component);
    }
}
