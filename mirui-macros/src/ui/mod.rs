use crate::capture::{FreeCaptureUse, capture_output, strip_bound_captures};
use crate::{compose_input, mold, style};
use quote::quote;

mod attrs;
mod branches;
mod widget;

use xrune::ds_node::ds_attr::DsAttr;
use xrune::ds_node::{DsTree, DsTreeRef};
use xrune::ds_rune::DsRune;
use xrune::ds_rune::decipher::decipher;

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
pub(super) enum WidgetKind {
    IllegalLowercase,
    Layout,
    Component,
}

pub(super) fn classify_widget_name(name: &syn::Ident) -> WidgetKind {
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
pub(super) struct ReactiveBind {
    property: syn::Ident,
    expr: proc_macro2::TokenStream,
    format_expr: syn::Expr,
    static_input: Option<syn::Ident>,
}

pub(super) fn reactive_read(value: &syn::Expr) -> proc_macro2::TokenStream {
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
        "src" if widget == "Image" => Some("ImageSource"),
        "visible" => Some("Visible"),
        "render_key" => Some("RenderKey"),
        "transform" => Some("Transform"),
        "scroll_offset" => Some("ScrollOffset"),
        "path" if widget == "Text" => Some("TextPath"),
        "value" if widget == "ProgressBar" => Some("ProgressValue"),
        "value" if widget == "Slider" => Some("SliderValue"),
        "items" if widget == "LazyList" => Some("LazyListItems"),
        "page" if widget == "PageView" => Some("Page"),
        "transition" if widget == "PageView" || widget == "NavigationStack" => {
            Some("PageTransition")
        }
        "item_extent" if widget == "LazyList" => Some("LazyListExtent"),
        "on" if widget == "Switch" => Some("SwitchOn"),
        "checked" if widget == "Checkbox" => Some("CheckboxChecked"),
        _ => style::property(widget, attr, false),
    }
}

pub(super) fn binding_role(property: &syn::Ident) -> proc_macro2::TokenStream {
    let name = property.to_string();
    let role = match name.as_str() {
        "Visible" | "Page" | "PageTransition" | "LazyListItems" | "LazyListExtent" => "Maintenance",
        "TextContent" | "ImageSource" | "RenderKey" | "Transform" | "TextPath"
        | "ProgressValue" | "SliderValue" | "SwitchOn" | "CheckboxChecked" | "ScrollOffset" => {
            "Display"
        }
        _ if style::is_display_binding(&name) => "Display",
        _ => "Ordinary",
    };
    let role = syn::Ident::new(role, property.span());
    quote! { ::mirui::__macro_support::BindingRole::#role }
}

pub(super) fn responsive_property(widget: &str, attr: &str) -> Option<&'static str> {
    match attr {
        "visible" => Some("Visible"),
        _ => style::property(widget, attr, true),
    }
}

struct LayoutBind {
    property: syn::Ident,
    value: crate::layout_reactive::LayoutValue,
}

struct StyleAttr {
    value: syn::Expr,
    reactive: bool,
    static_input: Option<syn::Ident>,
}

struct WidgetCmd {
    name: syn::Ident,
    kind: WidgetKind,
    var: syn::Ident,
    attrs: Vec<proc_macro2::TokenStream>,
    layout_fields: Vec<proc_macro2::TokenStream>,
    preset: Option<StyleAttr>,
    classes: Option<StyleAttr>,
    style_fields: std::collections::BTreeSet<&'static str>,
    errors: Vec<proc_macro2::TokenStream>,
    enchants: Vec<proc_macro2::TokenStream>,
    on_handlers: Vec<OnCmd>,
    component_fields: Vec<proc_macro2::TokenStream>,
    field_bindings: Vec<syn::Ident>,
    text_tuple_value: Option<proc_macro2::TokenStream>,
    capacity: Option<proc_macro2::TokenStream>,
    preedit_capacity: Option<proc_macro2::TokenStream>,
    text_paragraph_value: Option<proc_macro2::TokenStream>,
    id: Option<syn::LitStr>,
    id_lookups: Vec<(syn::Ident, String)>,
    reactive_binds: Vec<ReactiveBind>,
    property_values: Vec<(syn::Ident, proc_macro2::TokenStream)>,
    layout_binds: Vec<LayoutBind>,
    children: Vec<Cmd>,
}

pub(super) struct ParsedAttrs {
    pub(super) builder_calls: Vec<proc_macro2::TokenStream>,
    layout_fields: Vec<proc_macro2::TokenStream>,
    preset: Option<StyleAttr>,
    classes: Option<StyleAttr>,
    style_fields: std::collections::BTreeSet<&'static str>,
    pub(super) errors: Vec<proc_macro2::TokenStream>,
    component_inserts: Vec<proc_macro2::TokenStream>,
    component_fields: Vec<proc_macro2::TokenStream>,
    field_bindings: Vec<syn::Ident>,
    text_tuple_value: Option<proc_macro2::TokenStream>,
    capacity: Option<proc_macro2::TokenStream>,
    preedit_capacity: Option<proc_macro2::TokenStream>,
    text_paragraph_value: Option<proc_macro2::TokenStream>,
    id: Option<syn::LitStr>,
    id_lookups: Vec<(syn::Ident, String)>,
    pub(super) reactive_binds: Vec<ReactiveBind>,
    pub(super) property_values: Vec<(syn::Ident, proc_macro2::TokenStream)>,
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

pub(super) struct MiruiRune {
    pub(super) world_expr: proc_macro2::TokenStream,
    pub(super) parent_expr: proc_macro2::TokenStream,
    stack: Vec<Vec<Cmd>>,
    widget_kind_stack: Vec<Option<syn::Ident>>,
    counter: usize,
    mold_mode: bool,
    pub(super) props: Vec<syn::Ident>,
}

impl MiruiRune {
    pub(super) fn new() -> Self {
        Self {
            world_expr: quote! { __world },
            parent_expr: quote! { __parent },
            stack: vec![Vec::new()],
            widget_kind_stack: Vec::new(),
            counter: 0,
            mold_mode: false,
            props: Vec::new(),
        }
    }

    pub(super) fn new_mold(
        world: proc_macro2::TokenStream,
        entity: proc_macro2::TokenStream,
    ) -> Self {
        Self {
            world_expr: world,
            parent_expr: entity,
            stack: vec![Vec::new()],
            widget_kind_stack: Vec::new(),
            counter: 0,
            mold_mode: true,
            props: Vec::new(),
        }
    }

    fn next_var(&mut self) -> syn::Ident {
        let name = format!("__w{}", self.counter);
        self.counter += 1;
        syn::Ident::new(&name, proc_macro2::Span::call_site())
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
                let mut value = if attr.reactive {
                    compose_input::getter(&attr.value, &self.props)
                } else {
                    attr.value.clone()
                };
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
            preset: parsed.preset,
            classes: parsed.classes,
            style_fields: parsed.style_fields,
            errors: parsed.errors,
            enchants: enchant_tokens,
            on_handlers: collected_on_handlers,
            component_fields: parsed.component_fields,
            field_bindings: parsed.field_bindings,
            text_tuple_value: parsed.text_tuple_value,
            capacity: parsed.capacity,
            preedit_capacity: parsed.preedit_capacity,
            text_paragraph_value: parsed.text_paragraph_value,
            id: parsed.id,
            id_lookups,
            reactive_binds: parsed.reactive_binds,
            property_values: parsed.property_values,
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
            cond: Some(if reactive {
                compose_input::normalize(condition, &self.props)
            } else {
                condition.clone()
            }),
            body: self.collect_children(children),
        }];

        let mut next = else_branch.cloned();
        while let Some(tree) = next {
            let node_children = tree.borrow().get_children().to_vec();
            let chain_next = tree.borrow().get_else_branch().cloned();
            let cond = match tree.borrow().get_node() {
                DsNode::If(if_node) => Some(if reactive {
                    compose_input::normalize(if_node.get_condition(), &self.props)
                } else {
                    if_node.get_condition().clone()
                }),
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
        let props = self.props.clone();
        self.props.retain(|name| name != variable);
        self.stack.push(Vec::new());
        for child in children {
            decipher(child, self);
        }
        let body = self.stack.pop().unwrap();
        self.props = props;

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
            let props = self.props.clone();
            self.props
                .retain(|name| !FreeCaptureUse::bound_by(name, arm.get_pat()));
            self.stack.push(Vec::new());
            for child in arm.get_children() {
                decipher(child, self);
            }
            let body = self.stack.pop().unwrap();
            self.props = props;
            arm_cmds.push(MatchArm {
                pat: arm.get_pat().clone(),
                body,
            });
        }

        let cmd = Cmd::Match(MatchCmd {
            scrutinee: if reactive {
                compose_input::normalize(scrutinee, &self.props)
            } else {
                scrutinee.clone()
            },
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

pub(super) fn expand(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let (captures, props, input2) = match strip_bound_captures(input) {
        Ok(value) => value,
        Err(error) => return error.to_compile_error(),
    };

    if let Some(after) = try_strip_compose_keyword(&input2) {
        return mold::expand(after);
    }

    if let Some(call) = compose_input::call(&input2, &props) {
        let output = expand_ui_fn_call(call);
        return capture_output(output, captures);
    }

    let input2 = match crate::layout_reactive::preprocess(input2) {
        Ok(input) => input,
        Err(error) => return error.to_compile_error(),
    };
    let tree = match syn::parse2::<DsTree>(input2) {
        Ok(tree) => tree.into_ref(),
        Err(e) => return e.to_compile_error(),
    };
    let mut rune = MiruiRune::new();
    rune.props = props;

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
    capture_output(output, captures)
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

pub(super) fn scope_world_expr() -> proc_macro2::TokenStream {
    quote! {{
        use ::mirui::__macro_support::ScopeWorldAccess as _;
        cx.__mirui_world()
    }}
}

pub(super) fn expand_ui_fn_call(call: syn::ExprCall) -> proc_macro2::TokenStream {
    let func = &call.func;
    let args = &call.args;
    quote! {
        {
            let __compose_factory = #func(#args);
            __compose_factory(cx)
        }
    }
}
