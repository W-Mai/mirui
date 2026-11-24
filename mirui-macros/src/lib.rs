extern crate proc_macro;

mod animate_attr;
mod binding;
mod callback_lowering;
mod capture;
mod child_scope;
mod component_attr;
mod compose;
mod compose_attr;
mod compose_input;
mod diag;
mod event_attr;
mod function_attrs;
mod layout_reactive;
mod style;
mod ui;

use capture::{FreeCaptureUse, capture_output};
use ui::{MiruiRune, reactive_read, scope_world_expr};

#[cfg(test)]
use capture::strip_bound_captures;
#[cfg(test)]
use ui::{WidgetKind, binding_role, classify_widget_name, expand_ui_fn_call, responsive_property};
#[cfg(test)]
use xrune::ds_node::DsTree;
#[cfg(test)]
use xrune::ds_rune::{DsRune, decipher::decipher};
mod vector;
mod view_attr;
mod visit_id;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::parse_macro_input;

mod model_attr;
mod system_attr;
mod timer_attr;

/// Declare a typed, reusable group of widget style fields.
#[proc_macro]
pub fn style(input: TokenStream) -> TokenStream {
    style::expand(input.into())
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

#[doc(hidden)]
#[proc_macro]
pub fn __style_fields(_input: TokenStream) -> TokenStream {
    style::emit_schema().into()
}

/// Declare entity data with optional Model fields and output dependencies.
///
/// `#[depends_on(T)]` on an `Entity` or `Option<Entity>` field requests that source's current output. On the component type it refers to the owner. Type-level `#[retain_previous(T)]` requests owner history. Declarations activate when a View filtered by this component is registered; ordinary fields and component instances retain their authored types.
///
/// On a named custom component field, `#[property(P)]` binds a reactive `field: ${ expression }` through the declared [`Property`](https://docs.rs/mirui/latest/mirui/ui/property/trait.Property.html). The getter's result converts to `P::Value`. The display binding starts after View attachment, pauses in inactive branches, and is retired with the widget. Static field values initialize the component directly.
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
///
/// `#[model(persistence)]` and field-level `#[persist]` generate a typed field codec; `persistence(key = "settings")` adds root key metadata. Fields match by name and archive type, independently of `#[observe]`. Codec support is compiled according to mirui's `persistence` feature, not the application's feature names.
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
/// A nonempty function body supplies the paint callback. An empty body omits painting while retaining declared systems, attachment, input, and model subscriptions. The generated `function_name::view()` factory creates its widget descriptor.
///
/// `attach = initialize::callback()` invokes a typed `#[event]` callback during entity assembly, after static components, parent links, and IDs are installed. Initialization needs no event payload; `entity!()` identifies the new node. Deferred commands commit before the next attachment runs.
///
/// `input = handler::callback()` registers a typed `#[event]` callback; a tuple registers multiple callbacks. Gesture callbacks precede authored node listeners, while Key and TextCommitted callbacks follow authored node listeners. Prepared callbacks are shared by matching entities in the registering App.
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
/// A `prop!(T)` parameter accepts a static value or a shared read-only getter. UI calls pass `${expression}` to retain a getter; intermediate compositions forward the prop directly. A widget's `${name}` reads a declared prop in that widget's binding, subscribing to the original sources. Ordinary Rust callers can pass `Prop::new(move || expression)`. Getters do not create intermediate signals or effects.
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
    ui::expand(input.into()).into()
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
                let #ident = mirui::core::perf::is_enabled()
                    .then(|| mirui::core::perf::enter(#name));
            }
            .into()
        }
        trace_span_input::TraceSpanInput::Expression(name, body) => {
            let id = next_trace_span_id();
            let ident = quote::format_ident!("__trace_span_guard_{}", id);
            quote::quote! {{
                let #ident = mirui::core::perf::is_enabled()
                    .then(|| mirui::core::perf::enter(#name));
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
        let #ident = mirui::core::perf::is_enabled()
            .then(|| mirui::core::perf::enter(#name));
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
#[path = "../tests/unit/binding_role_tests.rs"]
mod binding_role_tests;
#[cfg(test)]
#[path = "../tests/unit/bound_capture_tests.rs"]
mod bound_capture_tests;
#[cfg(test)]
#[path = "../tests/unit/typed_on_tests.rs"]
mod typed_on_tests;
#[cfg(test)]
#[path = "../tests/unit/widget_kind_tests.rs"]
mod widget_kind_tests;
