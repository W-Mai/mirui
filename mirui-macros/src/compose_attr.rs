use crate::binding::{
    BindingCardinality, BindingSource, BindingSpec, COMPOSE_ROLES, LogicalType, ModelSource,
    RoleKind,
};
use proc_macro2::TokenStream;
use quote::quote;
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
            (BindingSource::Model(ModelSource::Projection { .. }), _) => {
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

    let cx_parameter: syn::FnArg = syn::parse_quote! {
        cx: &mut ::mirui::ui::scope::UiScope<'_>
    };
    function.sig.inputs.insert(0, cx_parameter);

    if !captures.is_empty() {
        BoundUiCalls { captures }.visit_block_mut(&mut function.block);
    }

    quote! { #function }
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

    #[test]
    fn injects_cx_on_simple_function() {
        let output = expand(quote! {}, quote! { fn build() {} });
        let rendered = output.to_string();
        assert!(rendered.contains("cx") && rendered.contains("UiScope"));
        assert!(!contains_compile_error(&output));
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
    fn marks_ui_trees_and_shares_direct_calls() {
        let output = expand(
            quote! {},
            quote! {
                fn build(game: model!(Game)) {
                    ui! { Row { child(game) child(game) } };
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
                    ui! { Button() on Tap { game.advance(); } };
                    mirui::ui! { Button() on Tap { game.advance(); } };
                    crate::ui!(child(game));
                    ::mirui::ui! { Button() on Tap { game.advance(); } };
                    other::ui! { Button() on Tap { game.advance(); } };
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
        let output = expand(quote! {}, quote! { fn build(game: model!(board.game)) {} });
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
