//! Attribute routing for callback bodies, declarations, and conditional items.
use syn::{Attribute, Meta, Token};

pub(crate) fn cfg_attrs(attrs: &[Attribute]) -> Vec<Attribute> {
    fn presence_meta(meta: &Meta) -> Option<Meta> {
        if meta.path().is_ident("cfg") {
            return Some(meta.clone());
        }
        if !meta.path().is_ident("cfg_attr") {
            return None;
        }
        let Meta::List(list) = meta else {
            return Some(meta.clone());
        };
        let Ok(nested) =
            list.parse_args_with(syn::punctuated::Punctuated::<Meta, Token![,]>::parse_terminated)
        else {
            return Some(meta.clone());
        };
        let mut nested = nested.into_iter();
        let Some(condition) = nested.next() else {
            return Some(meta.clone());
        };
        let presence: Vec<_> = nested.filter_map(|meta| presence_meta(&meta)).collect();
        if presence.is_empty() {
            None
        } else {
            Some(syn::parse_quote!(cfg_attr(#condition, #(#presence),*)))
        }
    }

    attrs
        .iter()
        .filter_map(|attr| presence_meta(&attr.meta))
        .map(|meta| syn::parse_quote!(#[#meta]))
        .collect()
}

pub(crate) struct FunctionAttributes {
    pub(crate) helper: Vec<Attribute>,
    pub(crate) declaration: Vec<Attribute>,
    pub(crate) conditional: Vec<Attribute>,
}

pub(crate) fn classify_function_attributes(
    attributes: Vec<Attribute>,
    context: &str,
) -> syn::Result<FunctionAttributes> {
    let mut helper = Vec::new();
    let mut declaration = Vec::new();
    let mut conditional = Vec::new();
    for attribute in attributes {
        if attribute.path().is_ident("cfg") {
            helper.push(attribute.clone());
            conditional.push(attribute);
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
            let mut declaration_nested = Vec::new();
            let mut conditional_nested = Vec::new();
            for meta in nested {
                if meta.path().is_ident("cfg_attr") {
                    return Err(syn::Error::new_spanned(
                        meta,
                        format!("nested `cfg_attr` is not supported on {context}"),
                    ));
                }
                if meta.path().is_ident("cfg") {
                    helper_nested.push(meta.clone());
                    conditional_nested.push(meta);
                } else if meta.path().is_ident("doc") || meta.path().is_ident("deprecated") {
                    declaration_nested.push(meta);
                } else {
                    helper_nested.push(meta);
                }
            }
            if !helper_nested.is_empty() {
                helper.push(syn::parse_quote!(
                    #[cfg_attr(#condition, #(#helper_nested),*)]
                ));
            }
            if !declaration_nested.is_empty() {
                declaration.push(syn::parse_quote!(
                    #[cfg_attr(#condition, #(#declaration_nested),*)]
                ));
            }
            if !conditional_nested.is_empty() {
                conditional.push(syn::parse_quote!(
                    #[cfg_attr(#condition, #(#conditional_nested),*)]
                ));
            }
        } else if attribute.path().is_ident("doc") || attribute.path().is_ident("deprecated") {
            declaration.push(attribute);
        } else {
            helper.push(attribute);
        }
    }
    Ok(FunctionAttributes {
        helper,
        declaration,
        conditional,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn separates_body_declaration_and_conditional_attributes() {
        let function: syn::ItemFn = syn::parse_quote! {
            #[cfg(all())]
            #[doc = "A callback."]
            #[inline]
            #[cfg_attr(all(), cfg(any()), doc = "Conditional docs.", inline)]
            fn callback() {}
        };
        let FunctionAttributes {
            helper,
            declaration,
            conditional,
        } = classify_function_attributes(function.attrs, "callbacks").unwrap();
        assert_eq!(
            quote!(#(#helper)*).to_string(),
            quote!(
                #[cfg(all())]
                #[inline]
                #[cfg_attr(all(), cfg(any()), inline)]
            )
            .to_string()
        );
        assert_eq!(
            quote!(#(#declaration)*).to_string(),
            quote!(
                #[doc = "A callback."]
                #[cfg_attr(all(), doc = "Conditional docs.")]
            )
            .to_string()
        );
        assert_eq!(
            quote!(#(#conditional)*).to_string(),
            quote!(
                #[cfg(all())]
                #[cfg_attr(all(), cfg(any()))]
            )
            .to_string()
        );
    }

    #[test]
    fn rejects_invalid_conditional_attributes_in_every_callback_lane() {
        for context in [
            "systems",
            "event callbacks",
            "timers",
            "views",
            "animations",
        ] {
            for function in [
                syn::parse_quote!(
                    #[cfg_attr()]
                    fn callback() {}
                ),
                syn::parse_quote!(
                    #[cfg_attr(all())]
                    fn callback() {}
                ),
                syn::parse_quote!(
                    #[cfg_attr(all(), cfg_attr(all(), inline))]
                    fn callback() {}
                ),
            ] {
                let function: syn::ItemFn = function;
                assert!(classify_function_attributes(function.attrs, context).is_err());
            }
        }
    }
}
