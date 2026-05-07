extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

use xrune::ds_node::ds_attr::DsAttr;
use xrune::ds_node::{DsRoot, DsTreeRef};
use xrune::ds_rune::DsRune;
use xrune::ds_rune::decipher::decipher;

/// A command node in the compile-time widget tree.
struct WidgetCmd {
    var: syn::Ident,
    attrs: Vec<proc_macro2::TokenStream>,
    layout_fields: Vec<proc_macro2::TokenStream>,
    errors: Vec<proc_macro2::TokenStream>,
    children: Vec<WidgetCmd>,
}

/// Intermediate tree built during DSL traversal.
struct MiruiRune {
    world_expr: proc_macro2::TokenStream,
    parent_expr: proc_macro2::TokenStream,
    /// Stack of widget cmd trees being built. Last = current parent's children vec.
    stack: Vec<Vec<WidgetCmd>>,
    counter: usize,
}

impl MiruiRune {
    fn new() -> Self {
        Self {
            world_expr: quote! { __world },
            parent_expr: quote! { __parent },
            stack: vec![Vec::new()], // root level
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
    ) {
        let mut builder_calls = Vec::new();
        let mut layout_fields = Vec::new();
        let mut errors = Vec::new();

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
                unknown => {
                    let msg = format!("unknown widget attribute `{}`", unknown);
                    errors.push(syn::Error::new(attr.name.span(), msg).to_compile_error());
                }
            }
        }
        (builder_calls, layout_fields, errors)
    }

    /// Recursively generate code from a WidgetCmd tree (post-order).
    fn emit(cmd: &WidgetCmd, world: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        let var = &cmd.var;
        let attrs = &cmd.attrs;
        let layout_fields = &cmd.layout_fields;
        let errors = &cmd.errors;

        let mut tokens = proc_macro2::TokenStream::new();

        // Emit errors first
        for e in errors {
            tokens.extend(e.clone());
        }

        // Emit children (post-order)
        let child_vars: Vec<&syn::Ident> = cmd.children.iter().map(|c| &c.var).collect();
        for child in &cmd.children {
            tokens.extend(Self::emit(child, world));
        }

        // Then create this widget
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

        tokens
    }
}

impl DsRune for MiruiRune {
    fn inscribe_root(&mut self, _parent_expr: &syn::Expr) {
        // parent_expr is the UI parent node — we don't use it directly in codegen
        // world_expr is set separately from context attrs
    }

    fn inscribe_widget(&mut self, _name: &syn::Ident, attrs: &[DsAttr], children: &[DsTreeRef]) {
        let var = self.next_var();
        let (builder_calls, layout_fields, errors) = Self::parse_attrs(attrs);

        // Push new children level
        self.stack.push(Vec::new());

        // Process children
        for child in children {
            decipher(child, self);
        }

        // Pop children
        let my_children = self.stack.pop().unwrap();

        let cmd = WidgetCmd {
            var,
            attrs: builder_calls,
            layout_fields,
            errors,
            children: my_children,
        };

        // Add to parent's children list
        self.stack.last_mut().unwrap().push(cmd);
    }

    fn inscribe_if(&mut self, _condition: &syn::Expr, _children: &[DsTreeRef]) {
        // TODO: conditional rendering
    }

    fn inscribe_iter(
        &mut self,
        _iterable: &syn::Expr,
        _variable: &syn::Ident,
        _children: &[DsTreeRef],
    ) {
        // TODO: iteration
    }

    fn seal(self) -> proc_macro2::TokenStream {
        let world = &self.world_expr;
        let parent_entity = &self.parent_expr;
        let mut tokens = proc_macro2::TokenStream::new();

        let root_cmds = &self.stack[0];
        for cmd in root_cmds {
            tokens.extend(Self::emit(cmd, world));
        }

        // Attach top-level widgets to parent
        for cmd in root_cmds {
            let var = &cmd.var;
            tokens.extend(quote! {
                {
                    use mirui::widget::{Children, Parent};
                    #world.insert(#var, Parent(#parent_entity));
                    if let Some(children) = #world.get_mut::<Children>(#parent_entity) {
                        children.0.push(#var);
                    }
                }
            });
        }

        tokens
    }
}

#[proc_macro]
pub fn ui(input: TokenStream) -> TokenStream {
    let root = parse_macro_input!(input as DsRoot);
    let mut rune = MiruiRune::new();

    // Extract world from context attrs
    let context_attrs = root.get_context_attrs();
    if let Some(world_attr) = context_attrs.iter().find(|a| a.name == "world") {
        let world_expr = &world_attr.value;
        rune.world_expr = quote! { #world_expr };
    } else {
        return syn::Error::new(proc_macro2::Span::call_site(), "missing `world` in context")
            .to_compile_error()
            .into();
    }

    // parent is the entity to attach top-level widgets to
    let parent = root.get_parent();
    rune.parent_expr = quote! { #parent };

    rune.inscribe_root(&root.get_parent());
    let content = root.get_content();
    decipher(&content, &mut rune);
    TokenStream::from(rune.seal())
}
