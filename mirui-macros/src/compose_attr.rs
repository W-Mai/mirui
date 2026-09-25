use proc_macro2::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit_mut::VisitMut;

pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let bindings = match parse_bindings(attr) {
        Ok(bindings) => bindings,
        Err(error) => return error.to_compile_error(),
    };
    let parsed: syn::Item = match syn::parse2(item) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error(),
    };

    let mut f = match parsed {
        syn::Item::Fn(f) => f,
        other => {
            return syn::Error::new(
                other.span(),
                "`#[compose]` applies to functions only; use `ui!(Name { ... })` macro form for widget classes",
            )
            .to_compile_error();
        }
    };

    if f.sig.asyncness.is_some() {
        return syn::Error::new(
            f.sig.asyncness.span(),
            "`#[compose]` does not support async fn in v0.40",
        )
        .to_compile_error();
    }

    if !f.sig.generics.params.is_empty() {
        return syn::Error::new(
            f.sig.generics.span(),
            "`#[compose]` does not support generic fn in v0.40",
        )
        .to_compile_error();
    }

    for input in f.sig.inputs.iter() {
        if let syn::FnArg::Typed(pat_ty) = input
            && let syn::Pat::Ident(pat_ident) = &*pat_ty.pat
            && pat_ident.ident == "cx"
        {
            return syn::Error::new(
                pat_ident.ident.span(),
                "`#[compose]` fn cannot have a parameter named `cx` — the attribute injects one for you",
            )
            .to_compile_error();
        }
    }

    for binding in &bindings {
        let Some(param) = f.sig.inputs.iter_mut().find_map(|input| {
            let syn::FnArg::Typed(param) = input else {
                return None;
            };
            let syn::Pat::Ident(pat) = &*param.pat else {
                return None;
            };
            (pat.ident == *binding).then_some(param)
        }) else {
            return syn::Error::new_spanned(binding, "bound name is not a function parameter")
                .to_compile_error();
        };
        let declared = param.ty.clone();
        *param.ty = syn::parse_quote!(
            <#declared as ::mirui::core::model::BindType>::Shared
        );
    }

    let cx_param: syn::FnArg = syn::parse_quote! {
        cx: &mut ::mirui::ui::scope::UiScope<'_>
    };
    f.sig.inputs.insert(0, cx_param);

    if !bindings.is_empty() {
        BoundUiCalls { bindings }.visit_block_mut(&mut f.block);
    }

    quote! { #f }
}

struct BoundUiCalls {
    bindings: Vec<syn::Ident>,
}

impl VisitMut for BoundUiCalls {
    fn visit_macro_mut(&mut self, macro_call: &mut syn::Macro) {
        if !macro_call.path.is_ident("ui") || is_compose_call(&macro_call.tokens) {
            return;
        }
        let original = macro_call.tokens.clone();
        let bindings = &self.bindings;
        macro_call.tokens = quote! {
            __mirui_bind(#(#bindings),*); #original
        };
    }
}

fn is_compose_call(tokens: &TokenStream) -> bool {
    let Ok(syn::Expr::Call(call)) = syn::parse2::<syn::Expr>(tokens.clone()) else {
        return false;
    };
    let syn::Expr::Path(path) = *call.func else {
        return false;
    };
    path.path
        .segments
        .last()
        .and_then(|segment| segment.ident.to_string().chars().next())
        .is_some_and(|first| first.is_ascii_lowercase() || first == '_')
}

fn parse_bindings(attr: TokenStream) -> syn::Result<Vec<syn::Ident>> {
    if attr.is_empty() {
        return Ok(Vec::new());
    }
    let syn::Meta::List(meta) = syn::parse2::<syn::Meta>(attr)? else {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected bind(name, ...)",
        ));
    };
    if !meta.path.is_ident("bind") {
        return Err(syn::Error::new_spanned(
            meta.path,
            "expected bind(name, ...)",
        ));
    }
    let names = meta.parse_args_with(Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated)?;
    if names.is_empty() {
        return Err(syn::Error::new(
            meta.span(),
            "bind requires at least one parameter",
        ));
    }
    let mut unique = Vec::new();
    for name in names {
        if unique.contains(&name) {
            return Err(syn::Error::new_spanned(name, "duplicate bound parameter"));
        }
        unique.push(name);
    }
    Ok(unique)
}

#[cfg(test)]
mod tests {
    use super::expand;
    use quote::quote;

    fn contains_compile_error(ts: &proc_macro2::TokenStream) -> bool {
        ts.to_string().contains("compile_error")
    }

    #[test]
    fn injects_cx_on_simple_fn() {
        let out = expand(quote! {}, quote! { fn build() {} });
        let s = out.to_string();
        assert!(
            s.contains("cx") && s.contains("UiScope"),
            "expected injected cx: &mut UiScope, got: {s}"
        );
        assert!(!contains_compile_error(&out));
    }

    #[test]
    fn preserves_other_params() {
        let out = expand(quote! {}, quote! { fn build(label: &str, count: i32) {} });
        let s = out.to_string();
        assert!(s.contains("label") && s.contains("count"));
        assert!(s.contains("cx") && s.contains("UiScope"));
    }

    #[test]
    fn maps_only_named_bound_params() {
        let out = expand(
            quote! { bind(model) },
            quote! { fn build(model: Game, title: &str) { let _ = (model, title); } },
        );
        let rendered = out.to_string();
        assert!(rendered.contains("Game as :: mirui :: core :: model :: BindType"));
        assert!(rendered.contains("title : & str"));
    }

    #[test]
    fn marks_ui_trees_for_shared_bound_captures() {
        let out = expand(
            quote! { bind(model) },
            quote! {
                fn build(model: Game) {
                    ui! { Row { Button() on Tap { model.increment(); } } };
                    ui!(child(cx, model));
                }
            },
        );
        let rendered = out.to_string();
        assert_eq!(rendered.matches("__mirui_bind").count(), 1);
        assert!(rendered.contains("ui ! (child (cx , model))"));
    }

    #[test]
    fn rejects_async_fn() {
        let out = expand(quote! {}, quote! { async fn build() {} });
        assert!(contains_compile_error(&out));
        assert!(out.to_string().contains("async"));
    }

    #[test]
    fn rejects_generic_fn() {
        let out = expand(quote! {}, quote! { fn build<T>() {} });
        assert!(contains_compile_error(&out));
        assert!(out.to_string().contains("generic"));
    }

    #[test]
    fn rejects_struct() {
        let out = expand(quote! {}, quote! { struct Card; });
        assert!(contains_compile_error(&out));
        assert!(out.to_string().contains("functions only"));
    }

    #[test]
    fn rejects_existing_cx_param() {
        let out = expand(quote! {}, quote! { fn build(cx: i32) {} });
        assert!(contains_compile_error(&out));
        assert!(out.to_string().contains("cx"));
    }
}
