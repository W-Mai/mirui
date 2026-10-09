use crate::binding::{
    BindingAccess, BindingCardinality, BindingSource, BindingSpec, COMPOSE_ROLES,
    ComposeResourceExpression, LogicalType, ModelSource, RoleKind,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::visit_mut::VisitMut;

pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new_spanned(
            attr,
            "`#[compose]` does not accept arguments; declare Model parameters as `name: model!(T)`",
        )
        .to_compile_error();
    }

    let parsed: syn::Item = match syn::parse2(item) {
        Ok(item) => item,
        Err(error) => return error.to_compile_error(),
    };
    let mut function = match parsed {
        syn::Item::Fn(function) => function,
        other => {
            return syn::Error::new(
                other.span(),
                "`#[compose]` applies to functions only; use `ui!(compose Name { ... })` for Niche Views",
            )
            .to_compile_error();
        }
    };

    if function.sig.asyncness.is_some() {
        return syn::Error::new(
            function.sig.asyncness.span(),
            "`#[compose]` does not support async functions",
        )
        .to_compile_error();
    }
    let mut captures = Vec::new();
    for input in &mut function.sig.inputs {
        let syn::FnArg::Typed(parameter) = input else {
            return syn::Error::new_spanned(
                input,
                "`#[compose]` functions cannot have a receiver; use a named parameter",
            )
            .to_compile_error();
        };

        if let syn::Pat::Ident(pattern) = &*parameter.pat
            && pattern.ident == "cx"
        {
            return syn::Error::new(
                pattern.ident.span(),
                "`#[compose]` cannot have a parameter named `cx`; the attribute provides it",
            )
            .to_compile_error();
        }

        let role = match COMPOSE_ROLES.classify(&parameter.ty, "#[compose]") {
            Ok(role) => role,
            Err(error) => return error.to_compile_error(),
        };
        let Some((role, _)) = role else {
            continue;
        };
        debug_assert_eq!(role, RoleKind::Model);

        if let Some(attribute) = parameter.attrs.first() {
            return syn::Error::new_spanned(
                attribute,
                "Compose Model parameters do not accept attributes",
            )
            .to_compile_error();
        }
        let syn::Pat::Ident(pattern) = &*parameter.pat else {
            return syn::Error::new_spanned(
                &parameter.pat,
                "Compose Model parameters must use a plain identifier",
            )
            .to_compile_error();
        };
        if pattern.by_ref.is_some() || pattern.mutability.is_some() || pattern.subpat.is_some() {
            return syn::Error::new_spanned(
                &parameter.pat,
                "Compose Model parameters must use a plain identifier",
            )
            .to_compile_error();
        }

        let binding = match BindingSpec::parse_model_type_role(pattern.ident.clone(), &parameter.ty)
        {
            Ok(Some(binding)) => binding,
            Ok(None) => {
                return syn::Error::new_spanned(
                    &parameter.ty,
                    "Compose Model parameters must use a direct `model!(...)` role",
                )
                .to_compile_error();
            }
            Err(error) => return error.to_compile_error(),
        };
        let model_type = match (&binding.source, &binding.logical_type) {
            (
                BindingSource::Model(ModelSource::Registered { .. }),
                LogicalType::Explicit(model_type),
            ) => model_type.clone(),
            (BindingSource::Model(ModelSource::ViewComponentField { .. }), _) => {
                return syn::Error::new(
                    binding.syntax.span(),
                    "`#[compose]` Model parameters select registered instances; use `model!(T)` or `model!(optional T)`",
                )
                .to_compile_error();
            }
            _ => {
                return syn::Error::new(
                    binding.syntax.span(),
                    "Compose Model parameters must use `model!(T)` or `model!(optional T)`",
                )
                .to_compile_error();
            }
        };

        *parameter.ty = match binding.cardinality {
            BindingCardinality::Required => syn::parse_quote!(
                <#model_type as ::mirui::core::model::BindType>::Shared
            ),
            BindingCardinality::Optional => syn::parse_quote!(
                ::core::option::Option<
                    <#model_type as ::mirui::core::model::BindType>::Shared
                >
            ),
            BindingCardinality::Many => {
                return syn::Error::new(
                    binding.syntax.span(),
                    "`#[compose]` Model parameters cannot have many cardinality",
                )
                .to_compile_error();
            }
        };
        captures.push(binding.name);
    }

    if !captures.is_empty() {
        let checks = captures
            .iter()
            .map(|name| syn::parse_quote!(cx.assert_binding(&#name);))
            .collect::<Vec<syn::Stmt>>();
        BoundUiCalls { captures }.visit_block_mut(&mut function.block);
        function.block.stmts.splice(0..0, checks);
    }

    let mut resources = ResourceExpressions { error: None };
    resources.visit_block_mut(&mut function.block);
    if let Some(error) = resources.error {
        return error.to_compile_error();
    }

    let inputs = function.sig.inputs.clone();
    let output = function.sig.output.clone();
    let block = &function.block;
    let block = if function.sig.unsafety.is_some() {
        quote!({ unsafe { #block } })
    } else {
        quote!(#block)
    };
    let execution = quote! {
        |cx: &mut ::mirui::ui::scope::UiScope<'_>, #inputs| #output #block
    };

    let mut arguments = Vec::with_capacity(function.sig.inputs.len());
    for (index, input) in function.sig.inputs.iter_mut().enumerate() {
        let syn::FnArg::Typed(parameter) = input else {
            unreachable!("compose receiver validation already completed");
        };
        let argument = format_ident!(
            "__mirui_compose_arg_{index}",
            span = proc_macro2::Span::mixed_site(),
        );
        parameter.pat = syn::parse_quote!(#argument);
        let attributes = crate::function_attrs::cfg_attrs(&parameter.attrs);
        arguments.push(quote!(#(#attributes)* #argument));
    }
    function.sig.output = syn::parse_quote! {
        -> impl ::core::ops::FnOnce(&mut ::mirui::ui::scope::UiScope<'_>) #output
    };
    function.block = syn::parse_quote! {
        {
            move |cx| (#execution)(cx, #(#arguments),*)
        }
    };
    quote! { #function }
}

struct ResourceExpressions {
    error: Option<syn::Error>,
}

pub(crate) fn prepare_event_composition(
    body: &mut syn::Block,
    captures: Vec<syn::Ident>,
) -> syn::Result<()> {
    if !captures.is_empty() {
        BoundUiCalls { captures }.visit_block_mut(body);
    }
    let mut resources = ResourceExpressions { error: None };
    resources.visit_block_mut(body);
    resources.error.map_or(Ok(()), Err)
}

impl VisitMut for ResourceExpressions {
    fn visit_expr_mut(&mut self, expression: &mut syn::Expr) {
        if self.error.is_some() {
            return;
        }
        if let syn::Expr::MethodCall(call) = expression
            && let Err(error) = lower_resource_receiver(&mut call.receiver)
        {
            self.error = Some(error);
            return;
        }
        syn::visit_mut::visit_expr_mut(self, expression);
    }

    fn visit_item_mut(&mut self, _item: &mut syn::Item) {}

    fn visit_macro_mut(&mut self, _macro_call: &mut syn::Macro) {}
}

fn lower_resource_receiver(receiver: &mut syn::Expr) -> syn::Result<()> {
    match receiver {
        syn::Expr::Paren(parenthesized) => lower_resource_receiver(&mut parenthesized.expr),
        syn::Expr::Group(grouped) => lower_resource_receiver(&mut grouped.expr),
        syn::Expr::Macro(expression) => {
            let Some(resource) =
                BindingSpec::parse_compose_resource_expression(expression.clone())?
            else {
                return Ok(());
            };
            let attributes = &expression.attrs;
            let lowered = match resource {
                ComposeResourceExpression::Gateway(syntax) => {
                    quote_spanned!(syntax.span()=> #(#attributes)* cx.__resources())
                }
                ComposeResourceExpression::Resource(binding) => {
                    let BindingSource::Resource(source) = &binding.source else {
                        unreachable!("Compose resource parsing produced a non-resource binding");
                    };
                    let ty = &source.ty;
                    let method = match (binding.access, binding.cardinality) {
                        (BindingAccess::Read, BindingCardinality::Required) => "__res_read",
                        (BindingAccess::Read, BindingCardinality::Optional) => "__res_optional",
                        (BindingAccess::Write, BindingCardinality::Required) => "__res_write",
                        _ => unreachable!("ResourceRole admitted unsupported Compose access"),
                    };
                    let method = syn::Ident::new(method, binding.syntax.span());
                    quote_spanned!(binding.syntax.span()=> #(#attributes)* cx.#method::<#ty>())
                }
            };
            *receiver = syn::parse2(lowered)?;
            Ok(())
        }
        _ => Ok(()),
    }
}

struct BoundUiCalls {
    captures: Vec<syn::Ident>,
}

impl VisitMut for BoundUiCalls {
    fn visit_macro_mut(&mut self, macro_call: &mut syn::Macro) {
        if !is_ui_macro(&macro_call.path) {
            return;
        }
        if let Some(mut call) = parse_compose_call(&macro_call.tokens) {
            for argument in &mut call.args {
                let syn::Expr::Path(path) = argument else {
                    continue;
                };
                let Some(name) = path.path.get_ident() else {
                    continue;
                };
                if self.captures.contains(name) {
                    *argument = syn::parse_quote!(
                        ::mirui::core::model::SharedValue::share(&#name)
                    );
                }
            }
            macro_call.tokens = quote!(#call);
            return;
        }
        let original = macro_call.tokens.clone();
        let captures = &self.captures;
        macro_call.tokens = quote! {
            __mirui_bind(#(#captures),*); #original
        };
    }
}

fn is_ui_macro(path: &syn::Path) -> bool {
    let mut segments = path.segments.iter();
    match (segments.next(), segments.next(), segments.next()) {
        (Some(ui), None, None) => path.leading_colon.is_none() && ui.ident == "ui",
        (Some(root), Some(ui), None) if ui.ident == "ui" => {
            root.ident == "mirui" || (root.ident == "crate" && path.leading_colon.is_none())
        }
        _ => false,
    }
}

fn parse_compose_call(tokens: &TokenStream) -> Option<syn::ExprCall> {
    let syn::Expr::Call(call) = syn::parse2::<syn::Expr>(tokens.clone()).ok()? else {
        return None;
    };
    let syn::Expr::Path(path) = &*call.func else {
        return None;
    };
    path.path
        .segments
        .last()
        .and_then(|segment| segment.ident.to_string().chars().next())
        .filter(|first| first.is_ascii_lowercase() || *first == '_')
        .map(|_| call)
}

#[cfg(test)]
mod tests {
    use super::expand;
    use quote::quote;

    fn contains_compile_error(tokens: &proc_macro2::TokenStream) -> bool {
        tokens.to_string().contains("compile_error")
    }

    fn execution_body(factory: &syn::ItemFn) -> &syn::ExprClosure {
        let syn::Stmt::Expr(syn::Expr::Closure(factory), _) = &factory.block.stmts[0] else {
            panic!("the factory must return a closure");
        };
        let syn::Expr::Call(call) = &*factory.body else {
            panic!("the factory must invoke its execution closure");
        };
        let syn::Expr::Paren(function) = &*call.func else {
            panic!("the execution closure must be parenthesized");
        };
        let syn::Expr::Closure(body) = &*function.expr else {
            panic!("the execution body must retain its lexical scope");
        };
        body
    }

    fn execution_block(factory: &syn::ItemFn) -> &syn::Block {
        let syn::Expr::Block(body) = &*execution_body(factory).body else {
            panic!("the execution body must be a block");
        };
        &body.block
    }

    #[test]
    fn returns_a_stack_factory_with_execution_local_parameters() {
        let output = expand(
            quote! {},
            quote! {
                fn build<T, const N: usize>(
                    _unused: T,
                    (left, right): (u8, u8),
                    mut total: u8,
                    ref label: String,
                    entered: &Cell<bool>,
                ) -> [u8; N] {
                    entered.set(true);
                    total += left + right;
                    let _ = label;
                    [total; N]
                }
            },
        );
        let factory: syn::ItemFn = syn::parse2(output).unwrap();
        assert_eq!(factory.sig.inputs.len(), 5);
        let body = execution_body(&factory);
        assert_eq!(body.inputs.len(), 6);
        let body = quote!(#body).to_string();
        assert!(body.contains("_unused : T"));
        assert!(body.contains("(left , right) : (u8 , u8)"));
        assert!(body.contains("mut total : u8"));
        assert!(body.contains("ref label : String"));
        assert_eq!(factory.sig.generics.params.len(), 2);
        let factory = quote!(#factory).to_string();
        assert!(factory.contains("impl :: core :: ops :: FnOnce"));
        assert!(factory.contains("move | cx |"));
        for index in 0..5 {
            assert_eq!(
                factory
                    .matches(&format!("__mirui_compose_arg_{index}"))
                    .count(),
                2,
            );
        }
    }

    #[test]
    fn injects_cx_on_simple_function() {
        let output = expand(quote! {}, quote! { fn build() {} });
        let rendered = output.to_string();
        assert!(rendered.contains("cx") && rendered.contains("UiScope"));
        assert!(!contains_compile_error(&output));
    }

    #[test]
    fn lowers_all_resource_operations_into_same_scope_receivers() {
        let output = expand(
            quote! {},
            quote! {
                fn build<T: 'static>(value: T) {
                    let required = res!(T).with(|resource| resource.inspect());
                    let optional = res!(optional T).with(|resource| resource.is_some());
                    res!(mut T).with(|resource| resource.update());
                    res!().insert(value);
                    let removed = res!().remove::<T>();
                }
            },
        );
        let function: syn::ItemFn = syn::parse2(output).unwrap();
        let body = execution_block(&function);
        assert_eq!(
            quote!(#body).to_string(),
            quote!({
                let required = cx.__res_read::<T>().with(|resource| resource.inspect());
                let optional = cx.__res_optional::<T>().with(|resource| resource.is_some());
                cx.__res_write::<T>().with(|resource| resource.update());
                cx.__resources().insert(value);
                let removed = cx.__resources().remove::<T>();
            })
            .to_string(),
        );
        assert_eq!(function.sig.generics.params.len(), 1);
        assert_eq!(function.sig.inputs.len(), 1);
        assert_eq!(execution_body(&function).inputs.len(), 2);
    }

    #[test]
    fn resource_operations_lower_through_blocks_and_synchronous_closures() {
        let output = expand(
            quote! {},
            quote! {
                fn build() {
                    for value in values {
                        if value.ready() {
                            (res!()).insert(value);
                        }
                    }
                    let mut install = || res!().insert(Resource::new());
                    install();
                    let count = match present {
                        true => res!(optional Resource).with(|resource| resource.is_some()),
                        false => false,
                    };
                }
            },
        );
        let rendered = output.to_string();
        assert_eq!(rendered.matches("cx . __resources ()").count(), 2);
        assert!(rendered.contains("(cx . __resources ()) . insert (value)"));
        assert!(rendered.contains("cx . __resources () . insert (Resource :: new ())"));
        assert!(rendered.contains("cx . __res_optional :: < Resource > () . with"));
        assert!(!rendered.contains("res !"));
        assert!(!contains_compile_error(&output));
    }

    #[test]
    fn resource_lowering_leaves_nested_items_and_macro_tokens_to_their_contexts() {
        let output = expand(
            quote! {},
            quote! {
                fn build(game: model!(Game)) {
                    fn independent() {
                        res!().insert(Resource);
                    }
                    #[compose]
                    fn child() {
                        res!(Resource).with(inspect);
                    }
                    let now = res!(Clock).with(now);
                    ui! {
                        Button () on Tap(|resource: res!(mut Resource)| {
                            commands!().insert_resource(Resource);
                        })
                    };
                    foreign! { res!().insert(Resource) };
                    ui!(child(game));
                }
            },
        );
        let function: syn::ItemFn = syn::parse2(output).unwrap();
        let first = &execution_block(&function).stmts[0];
        assert_eq!(
            quote!(#first).to_string(),
            quote!(cx.assert_binding(&game);).to_string()
        );
        let rendered = quote!(#function).to_string();
        assert_eq!(rendered.matches("cx . __res_read").count(), 1);
        assert!(!rendered.contains("cx . __resources"));
        assert!(rendered.contains("res ! () . insert (Resource)"));
        assert!(rendered.contains("res ! (Resource) . with (inspect)"));
        assert!(rendered.contains("resource : res ! (mut Resource)"));
        assert!(rendered.contains("commands ! () . insert_resource (Resource)"));
        assert_eq!(rendered.matches("__mirui_bind").count(), 1);
        assert_eq!(rendered.matches("SharedValue :: share").count(), 1);
    }

    #[test]
    fn compose_resource_expressions_reject_unsupported_access_without_extending_parameters() {
        for role in [
            quote!(res!(optional mut Resource)),
            quote!(res!(mut optional Resource)),
            quote!(res!(Resource, Other)),
            quote!(res![Resource]),
        ] {
            let output = expand(quote! {}, quote!(fn build() { #role.with(inspect); }));
            assert!(contains_compile_error(&output));
        }
        let parameter = expand(
            quote! {},
            quote!(
                fn build(resource: res!(Resource)) {}
            ),
        );
        assert!(contains_compile_error(&parameter));
        assert!(parameter.to_string().contains("not available"));
    }

    #[test]
    fn lowers_required_and_optional_model_roles_only() {
        let output = expand(
            quote! {},
            quote! {
                fn build(
                    game: model!(Game),
                    selection: model!(optional Selection),
                    title: &str,
                ) {
                    let _ = (game, selection, title);
                }
            },
        );
        let rendered = output.to_string();
        assert!(rendered.contains("Game as :: mirui :: core :: model :: BindType"));
        assert!(rendered.contains(
            ":: core :: option :: Option < < Selection as :: mirui :: core :: model :: BindType"
        ));
        assert!(rendered.contains("title : & str"));
        assert!(!contains_compile_error(&output));
    }

    #[test]
    fn validates_every_model_role_before_the_authored_body() {
        let output = expand(
            quote! {},
            quote! {
                fn build(game: model!(Game), other: model!(optional Other), plain: u8) {
                    body(game, other, plain);
                }
            },
        );
        let function: syn::ItemFn = syn::parse2(output).unwrap();
        let block = execution_block(&function);
        assert_eq!(block.stmts.len(), 3);
        let checks = &block.stmts[..2];
        assert_eq!(
            quote!(#(#checks)*).to_string(),
            quote!(cx.assert_binding(&game); cx.assert_binding(&other);).to_string(),
        );
        let body = &block.stmts[2];
        assert_eq!(
            quote!(#body).to_string(),
            quote!(body(game, other, plain);).to_string()
        );
    }

    #[test]
    fn marks_ui_trees_and_shares_direct_calls() {
        let output = expand(
            quote! {},
            quote! {
                fn build(game: model!(Game)) {
                    ui! {
                        Row () {
                            child (game)
                            child (game)
                        }
                    };
                    ui!(child(game));
                }
            },
        );
        let rendered = output.to_string();
        assert_eq!(rendered.matches("__mirui_bind").count(), 1);
        assert_eq!(rendered.matches("SharedValue :: share").count(), 1);
        assert!(
            rendered.contains(
                "ui ! (child (:: mirui :: core :: model :: SharedValue :: share (& game)))"
            )
        );
    }

    #[test]
    fn only_bare_model_arguments_are_shared_in_direct_calls() {
        let output = expand(
            quote! {},
            quote! {
                fn build(game: model!(Game), plain: String) {
                    ui!(child(game, plain, game.clone(), &game));
                }
            },
        );
        let rendered = output.to_string();
        assert_eq!(rendered.matches("SharedValue :: share").count(), 1);
        assert!(rendered.contains("plain , game . clone () , & game"));
    }

    #[test]
    fn niche_declarations_remain_niche_declarations() {
        let output = expand(
            quote! {},
            quote! {
                fn build(game: model!(Game)) {
                    ui!(compose Card { @@body });
                }
            },
        );
        let rendered = output.to_string();
        assert!(rendered.contains("__mirui_bind (game) ; compose Card"));
        assert!(!rendered.contains("SharedValue :: share"));
    }

    #[test]
    fn recognizes_only_supported_qualified_ui_paths() {
        let output = expand(
            quote! {},
            quote! {
                fn build(game: model!(Game)) {
                    ui! {
                        Button () on Tap { game.advance(); }
                    };
                    mirui::ui! {
                        Button () on Tap { game.advance(); }
                    };
                    crate::ui!(child(game));
                    ::mirui::ui! {
                        Button () on Tap { game.advance(); }
                    };
                    other::ui! {
                        Button () on Tap { game.advance(); }
                    };
                }
            },
        );
        let rendered = output.to_string();
        assert_eq!(rendered.matches("__mirui_bind").count(), 3);
        assert_eq!(rendered.matches("SharedValue :: share").count(), 1);
        assert!(rendered.contains("other :: ui ! { Button () on Tap"));
    }

    #[test]
    fn rejects_attribute_arguments() {
        let output = expand(quote! { unexpected }, quote! { fn build() {} });
        assert!(contains_compile_error(&output));
        assert!(output.to_string().contains("does not accept arguments"));
    }

    #[test]
    fn rejects_roles_not_available_to_compose() {
        let output = expand(quote! {}, quote! { fn build(time: res!(Time)) {} });
        assert!(contains_compile_error(&output));
        assert!(output.to_string().contains("not available"));
    }

    #[test]
    fn rejects_model_field_projections() {
        let output = expand(
            quote! {},
            quote! { fn build(game: model!(component.game)) {} },
        );
        assert!(contains_compile_error(&output));
        assert!(output.to_string().contains("registered instances"));
    }

    #[test]
    fn rejects_non_plain_model_parameter_patterns() {
        let output = expand(quote! {}, quote! { fn build((game,): model!(Game)) {} });
        assert!(contains_compile_error(&output));
        assert!(output.to_string().contains("plain identifier"));
    }

    #[test]
    fn rejects_async_functions() {
        let output = expand(quote! {}, quote! { async fn build() {} });
        assert!(contains_compile_error(&output));
        assert!(output.to_string().contains("async"));
    }

    #[test]
    fn preserves_generic_functions() {
        let output = expand(
            quote! {},
            quote! { fn build<T: Copy>(value: T) -> T { value } },
        );
        let rendered = output.to_string();
        assert!(rendered.contains("fn build < T : Copy >"));
        assert!(rendered.contains("value : T"));
        assert!(!contains_compile_error(&output));
    }

    #[test]
    fn rejects_non_functions() {
        let output = expand(quote! {}, quote! { struct Card; });
        assert!(contains_compile_error(&output));
        assert!(output.to_string().contains("functions only"));
    }

    #[test]
    fn rejects_existing_cx_parameter() {
        let output = expand(quote! {}, quote! { fn build(cx: i32) {} });
        assert!(contains_compile_error(&output));
        assert!(output.to_string().contains("cx"));
    }
}
