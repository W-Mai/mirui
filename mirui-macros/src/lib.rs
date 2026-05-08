extern crate proc_macro;

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
    children: Vec<Cmd>,
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

    fn parse_attrs(
        attrs: &[DsAttr],
    ) -> (
        Vec<proc_macro2::TokenStream>,
        Vec<proc_macro2::TokenStream>,
        Vec<proc_macro2::TokenStream>,
        Vec<proc_macro2::TokenStream>,
    ) {
        let mut builder_calls = Vec::new();
        let mut layout_fields = Vec::new();
        let mut errors = Vec::new();
        let component_inserts = Vec::new();

        for attr in attrs {
            let name = attr.name.to_string();
            let value = &attr.value;
            match name.as_str() {
                "bg_color" => builder_calls.push(quote! { .bg_color(#value) }),
                "text" => builder_calls.push(quote! { .text(#value) }),
                "text_color" => builder_calls.push(quote! { .text_color(#value) }),
                "border_radius" => builder_calls.push(quote! { .border_radius(#value) }),
                "border_color" => builder_calls.push(quote! { .border(#value, 1) }),
                "width" => layout_fields.push(quote! { width: Some(#value) }),
                "height" => layout_fields.push(quote! { height: Some(#value) }),
                "grow" => layout_fields.push(quote! { grow: #value }),
                "direction" => layout_fields.push(quote! { direction: #value }),
                "justify" => layout_fields.push(quote! { justify: #value }),
                "align" => layout_fields.push(quote! { align: #value }),
                "padding" => layout_fields.push(quote! { padding: #value }),
                "position" => layout_fields.push(quote! { position: #value }),
                "left" => layout_fields.push(quote! { left: Some(#value) }),
                "top" => layout_fields.push(quote! { top: Some(#value) }),
                "image" => builder_calls.push(quote! { .image(#value) }),
                unknown => {
                    let msg = format!("unknown widget attribute `{}`", unknown);
                    errors.push(syn::Error::new(attr.name.span(), msg).to_compile_error());
                }
            }
        }
        (builder_calls, layout_fields, errors, component_inserts)
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
        let (builder_calls, layout_fields, errors, component_inserts) = Self::parse_attrs(attrs);
        let mut enchant_tokens: Vec<proc_macro2::TokenStream> = component_inserts;
        enchant_tokens.extend(enchants.iter().map(|e| quote! { #e }));

        self.stack.push(Vec::new());
        for child in children {
            decipher(child, self);
        }
        let my_children = self.stack.pop().unwrap();

        let cmd = Cmd::Widget(WidgetCmd {
            var,
            attrs: builder_calls,
            layout_fields,
            errors,
            enchants: enchant_tokens,
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
