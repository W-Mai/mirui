extern crate proc_macro;

mod compose;
mod diag;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

use xrune::ds_node::ds_attr::DsAttr;
use xrune::ds_node::{DsRoot, DsTreeRef};
use xrune::ds_rune::DsRune;
use xrune::ds_rune::decipher::decipher;

enum Cmd {
    Widget(WidgetCmd),
    Iter(IterCmd),
    If(IfCmd),
}

struct WidgetCmd {
    var: syn::Ident,
    attrs: Vec<proc_macro2::TokenStream>,
    layout_fields: Vec<proc_macro2::TokenStream>,
    errors: Vec<proc_macro2::TokenStream>,
    enchants: Vec<proc_macro2::TokenStream>,
    id_registrations: Vec<proc_macro2::TokenStream>,
    children: Vec<Cmd>,
}

struct ParsedAttrs {
    builder_calls: Vec<proc_macro2::TokenStream>,
    layout_fields: Vec<proc_macro2::TokenStream>,
    errors: Vec<proc_macro2::TokenStream>,
    component_inserts: Vec<proc_macro2::TokenStream>,
    id_registrations: Vec<proc_macro2::TokenStream>,
}

struct IterCmd {
    iterable: proc_macro2::TokenStream,
    variable: syn::Ident,
    body: Vec<Cmd>,
}

struct IfCmd {
    condition: proc_macro2::TokenStream,
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

    fn parse_attrs(attrs: &[DsAttr]) -> ParsedAttrs {
        let mut builder_calls = Vec::new();
        let mut layout_fields = Vec::new();
        let mut errors = Vec::new();
        let component_inserts = Vec::new();
        let mut id_registrations = Vec::new();

        for attr in attrs {
            let name = attr.name.to_string();
            let value = &attr.value;
            match name.as_str() {
                "bg_color" => builder_calls.push(quote! { .bg_color(#value) }),
                "text" => builder_calls.push(quote! { .text(#value) }),
                "text_color" => builder_calls.push(quote! { .text_color(#value) }),
                "border_radius" => builder_calls.push(quote! { .border_radius(#value) }),
                "clip_children" => builder_calls.push(quote! { .clip_children(#value) }),
                "border_color" => builder_calls.push(quote! { .border(#value, 1) }),
                "border_width" => builder_calls.push(quote! { .border_width(#value) }),
                "width" => layout_fields.push(quote! { width: mirui::types::Dimension::Px(mirui::types::Fixed::from_int(#value as i32)) }),
                "height" => layout_fields.push(quote! { height: mirui::types::Dimension::Px(mirui::types::Fixed::from_int(#value as i32)) }),
                "grow" => layout_fields.push(quote! { grow: mirui::types::Fixed::from_f32(#value) }),
                "direction" => layout_fields.push(quote! { direction: #value }),
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
                unknown => {
                    let msg = format!("unknown widget attribute `{unknown}`");
                    errors.push(syn::Error::new(attr.name.span(), msg).to_compile_error());
                }
            }
        }
        ParsedAttrs {
            builder_calls,
            layout_fields,
            errors,
            component_inserts,
            id_registrations,
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
        }
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

        // Emit static widget children first (post-order)
        let var_ts = quote! { #var };
        let mut child_vars = Vec::new();
        let mut deferred_iters = Vec::new();

        for child in &cmd.children {
            match child {
                Cmd::Widget(w) => {
                    tokens.extend(Self::emit_widget(w, world));
                    child_vars.push(&w.var);
                }
                Cmd::Iter(_) | Cmd::If(_) => {
                    deferred_iters.push(child);
                }
            }
        }

        // Create this widget with static children
        let layout_call = if layout_fields.is_empty() {
            quote! {}
        } else {
            quote! { .layout(mirui::layout::LayoutStyle { #(#layout_fields,)* ..Default::default() }) }
        };

        let child_calls: Vec<proc_macro2::TokenStream> =
            child_vars.iter().map(|c| quote! { .child(#c) }).collect();

        tokens.extend(quote! {
            let #var = mirui::widget::builder::WidgetBuilder::new(#world)
                #(#attrs)*
                #layout_call
                #(#child_calls)*
                .id();
        });

        // Emit enchants — insert components
        let enchants = &cmd.enchants;
        for enchant in enchants {
            tokens.extend(quote! {
                (#world).insert(#var, #enchant);
            });
        }

        for id_lit in &cmd.id_registrations {
            tokens.extend(quote! {
                (#world).insert(#var, mirui::widget::NamedId(#id_lit));
                if let Some(__map) = (#world).resource_mut::<mirui::widget::IdMap>() {
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

    fn emit_iter(
        cmd: &IterCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let iterable = &cmd.iterable;
        let variable = &cmd.variable;

        // Generate loop body — each widget in body attaches to parent_var
        let mut body_tokens = proc_macro2::TokenStream::new();
        for child in &cmd.body {
            body_tokens.extend(Self::emit_cmd(child, world, parent_var));
            // Attach each top-level widget in loop body to parent
            if let Cmd::Widget(w) = child {
                let child_var = &w.var;
                body_tokens.extend(quote! {
                    {
                        use mirui::widget::{Children, Parent};
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
        let condition = &cmd.condition;

        let mut body_tokens = proc_macro2::TokenStream::new();
        for child in &cmd.body {
            body_tokens.extend(Self::emit_cmd(child, world, parent_var));
            if let Cmd::Widget(w) = child {
                let child_var = &w.var;
                body_tokens.extend(quote! {
                    {
                        use mirui::widget::{Children, Parent};
                        (#world).insert(#child_var, Parent(#parent_var));
                        if let Some(children) = (#world).get_mut::<Children>(#parent_var) {
                            children.0.push(#child_var);
                        }
                    }
                });
            }
        }

        quote! {
            if #condition {
                #body_tokens
            }
        }
    }
}

impl DsRune for MiruiRune {
    fn inscribe_root(&mut self, _parent_expr: &syn::Expr) {}

    fn inscribe_widget(
        &mut self,
        _name: &syn::Ident,
        attrs: &[DsAttr],
        enchants: &[syn::Expr],
        children: &[DsTreeRef],
    ) {
        let var = self.next_var();
        let parsed = Self::parse_attrs(attrs);
        let mut enchant_tokens: Vec<proc_macro2::TokenStream> = parsed.component_inserts;
        enchant_tokens.extend(enchants.iter().map(|e| quote! { #e }));

        self.stack.push(Vec::new());
        for child in children {
            decipher(child, self);
        }
        let my_children = self.stack.pop().unwrap();

        let cmd = Cmd::Widget(WidgetCmd {
            var,
            attrs: parsed.builder_calls,
            layout_fields: parsed.layout_fields,
            errors: parsed.errors,
            enchants: enchant_tokens,
            id_registrations: parsed.id_registrations,
            children: my_children,
        });

        self.stack.last_mut().unwrap().push(cmd);
    }

    fn inscribe_if(&mut self, condition: &syn::Expr, children: &[DsTreeRef]) {
        self.stack.push(Vec::new());
        for child in children {
            decipher(child, self);
        }
        let body = self.stack.pop().unwrap();

        let cmd = Cmd::If(IfCmd {
            condition: quote! { #condition },
            body,
        });

        self.stack.last_mut().unwrap().push(cmd);
    }

    fn inscribe_iter(
        &mut self,
        iterable: &syn::Expr,
        variable: &syn::Ident,
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
        });

        self.stack.last_mut().unwrap().push(cmd);
    }

    fn inscribe_niche(&mut self, _name: &syn::Ident, _children: &[DsTreeRef]) {
        unimplemented!("@niche emission is not yet supported in this build");
    }

    fn inscribe_match(
        &mut self,
        _scrutinee: &syn::Expr,
        _arms: &[xrune::ds_node::ds_match::DsMatchArm],
    ) {
        unimplemented!("ui! match emission is not yet supported in this build");
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
                        use mirui::widget::{Children, Parent};
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
    if let Some(world_attr) = context_attrs.iter().find(|a| a.name == "world") {
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
            Schedule::After(p) => quote! { mirui::timer::Timer::after(#p, __cb) },
            Schedule::Every(p) => quote! { mirui::timer::Timer::every(#p, __cb) },
            Schedule::Repeat { times, period } => {
                quote! { mirui::timer::Timer::repeat(#times, #period, __cb) }
            }
            Schedule::Until { deadline, period } => {
                quote! { mirui::timer::Timer::until(#deadline, #period, __cb) }
            }
        };

        quote! {
            pub struct #name;
            impl #name {
                pub fn install(world: &mut mirui::ecs::World) -> mirui::ecs::Entity {
                    let __cb: fn(&mut mirui::ecs::World, mirui::ecs::Entity) = #closure;
                    let e = world.spawn();
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
///     mirui::widget::set_position(world, entity, value, Fixed::from_int(2));
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
                let #ident = mirui::perf::enter(#name);
            }
            .into()
        }
        trace_span_input::TraceSpanInput::Expression(name, body) => {
            let id = next_trace_span_id();
            let ident = quote::format_ident!("__trace_span_guard_{}", id);
            quote::quote! {{
                let #ident = mirui::perf::enter(#name);
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
        let #ident = mirui::perf::enter(#name);
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
