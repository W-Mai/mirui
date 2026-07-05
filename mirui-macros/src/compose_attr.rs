use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;

pub fn expand(_attr: TokenStream, item: TokenStream) -> TokenStream {
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

    let cx_param: syn::FnArg = syn::parse_quote! {
        cx: &mut ::mirui::ui::scope::UiScope<'_>
    };
    f.sig.inputs.insert(0, cx_param);

    quote! { #f }
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
