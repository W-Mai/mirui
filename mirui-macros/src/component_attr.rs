use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, Item, Meta, Path, punctuated::Punctuated, spanned::Spanned};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let bindings = parse_bindings(attr)?;
    let mut item = syn::parse2::<Item>(item)?;
    let (name, generics, clone_impl) = match &mut item {
        Item::Struct(item) => {
            reject_component_derive(&item.attrs)?;
            let clone_impl = if bindings.is_empty() {
                None
            } else {
                let Fields::Named(fields) = &mut item.fields else {
                    return Err(syn::Error::new_spanned(
                        &item.fields,
                        "bound components require named fields",
                    ));
                };
                for binding in &bindings {
                    let Some(field) = fields
                        .named
                        .iter_mut()
                        .find(|field| field.ident.as_ref() == Some(binding))
                    else {
                        return Err(syn::Error::new_spanned(
                            binding,
                            "bound name is not a component field",
                        ));
                    };
                    let declared = field.ty.clone();
                    item.generics
                        .make_where_clause()
                        .predicates
                        .push(syn::parse_quote!(#declared: ::mirui::core::model::BindType));
                    field.ty = syn::parse_quote!(
                        <#declared as ::mirui::core::model::BindType>::Shared
                    );
                }
                let clone_requested = take_clone_derive(&mut item.attrs)?;
                clone_requested.then(|| {
                    let field_names: Vec<_> = fields
                        .named
                        .iter()
                        .map(|field| field.ident.as_ref().expect("named field"))
                        .collect();
                    let field_types: Vec<_> = fields.named.iter().map(|field| &field.ty).collect();
                    let mut clone_generics = item.generics.clone();
                    for field_type in field_types {
                        clone_generics
                            .make_where_clause()
                            .predicates
                            .push(syn::parse_quote!(#field_type: ::core::clone::Clone));
                    }
                    let (impl_generics, type_generics, where_clause) =
                        clone_generics.split_for_impl();
                    let name = &item.ident;
                    quote! {
                        impl #impl_generics ::core::clone::Clone for #name #type_generics #where_clause {
                            fn clone(&self) -> Self {
                                Self { #(#field_names: self.#field_names.clone()),* }
                            }
                        }
                    }
                })
            };
            (item.ident.clone(), item.generics.clone(), clone_impl)
        }
        Item::Enum(item) if bindings.is_empty() => {
            reject_component_derive(&item.attrs)?;
            (item.ident.clone(), item.generics.clone(), None)
        }
        Item::Enum(item) => {
            return Err(syn::Error::new_spanned(
                item,
                "bound components require a struct with named fields",
            ));
        }
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "#[component] requires a struct or enum",
            ));
        }
    };
    let (_, type_generics, _) = generics.split_for_impl();
    let mut component_generics = generics.clone();
    component_generics
        .make_where_clause()
        .predicates
        .push(syn::parse_quote!(#name #type_generics: 'static));
    let (impl_generics, _, where_clause) = component_generics.split_for_impl();
    Ok(quote! {
        #item
        impl #impl_generics ::mirui::ecs::Component for #name #type_generics #where_clause {}
        #clone_impl
    })
}

fn parse_bindings(attr: TokenStream) -> syn::Result<Vec<syn::Ident>> {
    if attr.is_empty() {
        return Ok(Vec::new());
    }
    let Meta::List(meta) = syn::parse2::<Meta>(attr)? else {
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
            "bind requires at least one field",
        ));
    }
    let mut unique = Vec::new();
    for name in names {
        if unique.contains(&name) {
            return Err(syn::Error::new_spanned(name, "duplicate bound field"));
        }
        unique.push(name);
    }
    Ok(unique)
}

fn reject_component_derive(attrs: &[syn::Attribute]) -> syn::Result<()> {
    for attr in attrs {
        if derive_paths(attr)?.iter().any(|path| {
            path.segments
                .last()
                .is_some_and(|segment| segment.ident == "Component")
        }) {
            return Err(syn::Error::new_spanned(
                attr,
                "#[component] already implements Component; remove derive(Component)",
            ));
        }
    }
    Ok(())
}

fn take_clone_derive(attrs: &mut Vec<syn::Attribute>) -> syn::Result<bool> {
    let mut clone_requested = false;
    let mut remaining = Vec::with_capacity(attrs.len());
    for attr in attrs.drain(..) {
        if !attr.path().is_ident("derive") {
            remaining.push(attr);
            continue;
        }
        let paths = derive_paths(&attr)?;
        let mut retained = Vec::new();
        for path in paths {
            if path.is_ident("Clone") {
                clone_requested = true;
            } else {
                retained.push(path);
            }
        }
        if !retained.is_empty() {
            remaining.push(syn::parse_quote!(#[derive(#(#retained),*)]));
        }
    }
    *attrs = remaining;
    Ok(clone_requested)
}

fn derive_paths(attr: &syn::Attribute) -> syn::Result<Vec<Path>> {
    if !attr.path().is_ident("derive") {
        return Ok(Vec::new());
    }
    Ok(attr
        .parse_args_with(Punctuated::<Path, syn::Token![,]>::parse_terminated)?
        .into_iter()
        .collect())
}

#[cfg(test)]
mod tests {
    use super::expand;
    use quote::quote;

    #[test]
    fn rejects_unknown_and_duplicate_bound_fields() {
        assert!(
            expand(
                quote!(bind(missing)),
                quote!(
                    struct Board {
                        model: Model,
                    }
                )
            )
            .is_err()
        );
        assert!(
            expand(
                quote!(bind(model, model)),
                quote!(
                    struct Board {
                        model: Model,
                    }
                )
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_duplicate_component_marker() {
        assert!(
            expand(
                quote!(),
                quote!(
                    #[derive(Component)]
                    struct Board;
                )
            )
            .is_err()
        );
    }
}
