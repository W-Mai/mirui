//! Attribute routing for generated callback bodies and registration modules.
use syn::{Attribute, Meta, Token};

pub(crate) struct FunctionAttributes {
    pub(crate) helper: Vec<Attribute>,
    pub(crate) module: Vec<Attribute>,
}

pub(crate) fn classify_function_attributes(
    attributes: Vec<Attribute>,
    context: &str,
) -> syn::Result<FunctionAttributes> {
    let mut helper = Vec::new();
    let mut module = Vec::new();
    for attribute in attributes {
        if attribute.path().is_ident("cfg") {
            helper.push(attribute.clone());
            module.push(attribute);
        } else if attribute.path().is_ident("cfg_attr") {
            let nested = attribute.parse_args_with(
                syn::punctuated::Punctuated::<Meta, Token![,]>::parse_terminated,
            )?;
            let mut nested = nested.into_iter();
            let Some(condition) = nested.next() else {
                return Err(syn::Error::new_spanned(
                    attribute,
                    "`cfg_attr` requires a condition and at least one attribute",
                ));
            };
            let nested = nested.collect::<Vec<_>>();
            if nested.is_empty() {
                return Err(syn::Error::new_spanned(
                    attribute,
                    "`cfg_attr` requires at least one conditional attribute",
                ));
            }
            let mut helper_nested = Vec::new();
            let mut module_nested = Vec::new();
            for meta in nested {
                if meta.path().is_ident("cfg_attr") {
                    return Err(syn::Error::new_spanned(
                        meta,
                        format!("nested `cfg_attr` is not supported on {context}"),
                    ));
                }
                if meta.path().is_ident("cfg") {
                    helper_nested.push(meta.clone());
                    module_nested.push(meta);
                } else if meta.path().is_ident("doc") || meta.path().is_ident("deprecated") {
                    module_nested.push(meta);
                } else {
                    helper_nested.push(meta);
                }
            }
            if !helper_nested.is_empty() {
                helper.push(syn::parse_quote!(
                    #[cfg_attr(#condition, #(#helper_nested),*)]
                ));
            }
            if !module_nested.is_empty() {
                module.push(syn::parse_quote!(
                    #[cfg_attr(#condition, #(#module_nested),*)]
                ));
            }
        } else if attribute.path().is_ident("doc") || attribute.path().is_ident("deprecated") {
            module.push(attribute);
        } else {
            helper.push(attribute);
        }
    }
    Ok(FunctionAttributes { helper, module })
}
