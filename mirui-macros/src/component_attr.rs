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
                    // A cfg-gated field may name a type that does not exist in
                    // the other configuration. Its bound must be supplied by
                    // the declaration when the field is enabled.
                    if !field.attrs.iter().any(|attr| may_remove_field(&attr.meta)) {
                        item.generics
                            .make_where_clause()
                            .predicates
                            .push(syn::parse_quote!(#declared: ::mirui::core::model::BindType));
                    }
                    field.ty = syn::parse_quote!(
                        <#declared as ::mirui::core::model::BindType>::Shared
                    );
                }
                let clone_requested = take_clone_derive(&mut item.attrs)?;
                clone_requested.then(|| {
                    let clone_fields: Vec<_> = fields
                        .named
                        .iter()
                        .map(|field| {
                            let name = field.ident.as_ref().expect("named field");
                            let conditional_attrs = field
                                .attrs
                                .iter()
                                .filter(|attr| may_remove_field(&attr.meta));
                            quote!(#(#conditional_attrs)* #name: self.#name.clone())
                        })
                        .collect();
                    let mut clone_generics = item.generics.clone();
                    for field in &fields.named {
                        if !field.attrs.iter().any(|attr| may_remove_field(&attr.meta)) {
                            let field_type = &field.ty;
                            clone_generics
                                .make_where_clause()
                                .predicates
                                .push(syn::parse_quote!(#field_type: ::core::clone::Clone));
                        }
                    }
                    let (impl_generics, type_generics, where_clause) =
                        clone_generics.split_for_impl();
                    let name = &item.ident;
                    quote! {
                        impl #impl_generics ::core::clone::Clone for #name #type_generics #where_clause {
                            fn clone(&self) -> Self {
                                Self { #(#clone_fields),* }
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
    let marker = marker_impl(&name, &generics);
    Ok(quote! {
        #item
        #marker
        #clone_impl
    })
}

pub(crate) fn marker_impl(name: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let (_, type_generics, _) = generics.split_for_impl();
    let mut component_generics = generics.clone();
    component_generics
        .make_where_clause()
        .predicates
        .push(syn::parse_quote!(#name #type_generics: 'static));
    let (impl_generics, _, where_clause) = component_generics.split_for_impl();
    quote! {
        impl #impl_generics ::mirui::ecs::Component for #name #type_generics #where_clause {}
    }
}

fn may_remove_field(meta: &Meta) -> bool {
    if meta.path().is_ident("cfg") {
        return true;
    }
    if !meta.path().is_ident("cfg_attr") {
        return false;
    }
    let Meta::List(list) = meta else {
        return true;
    };
    list.parse_args_with(Punctuated::<Meta, syn::Token![,]>::parse_terminated)
        .map(|nested| nested.iter().skip(1).any(may_remove_field))
        .unwrap_or(true)
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
            if is_clone_derive(&path) {
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

fn is_clone_derive(path: &Path) -> bool {
    if path.is_ident("Clone") {
        return true;
    }
    let segments: Vec<_> = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    matches!(segments.as_slice(), [root, module, name]
        if (root == "core" || root == "std") && module == "clone" && name == "Clone")
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
    use super::{expand, may_remove_field};
    use quote::quote;
    use syn::parse_quote;

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

    #[test]
    fn rejects_unsupported_component_shapes() {
        assert!(expand(quote!(), quote!(union Bits { value: u8 })).is_err());
        assert!(
            expand(
                quote!(bind(model)),
                quote!(
                    struct Board(Model);
                )
            )
            .is_err()
        );
        assert!(
            expand(
                quote!(bind(model)),
                quote!(
                    enum Board {
                        Empty,
                    }
                )
            )
            .is_err()
        );
    }

    #[test]
    fn custom_clone_accepts_qualified_derive_and_preserves_conditional_fields() {
        let output = expand(
            quote!(bind(model)),
            quote! {
                #[derive(::core::clone::Clone)]
                struct Board {
                    #[cfg(any())]
                    model: Model,
                    label: u8,
                }
            },
        )
        .unwrap()
        .to_string();
        assert!(!output.contains("derive"));
        assert_eq!(output.matches("cfg (any ())").count(), 2);
        assert!(!output.contains("Model : :: mirui :: core :: model :: BindType"));
        assert!(output.contains("impl :: core :: clone :: Clone for Board"));
    }

    #[test]
    fn conditional_cfg_attr_controls_clone_fields_and_bounds() {
        let nested: syn::Attribute = parse_quote!(
            #[cfg_attr(feature = "extra", cfg_attr(feature = "nested", cfg(feature = "other")))]
        );
        let documentation_only: syn::Attribute =
            parse_quote!(#[cfg_attr(feature = "extra", doc = "note")]);
        assert!(may_remove_field(&nested.meta));
        assert!(!may_remove_field(&documentation_only.meta));

        let output = expand(
            quote!(bind(model, stable)),
            quote! {
                #[derive(Clone)]
                struct Board {
                    #[cfg_attr(feature = "extra", cfg(feature = "other"))]
                    model: ConditionalModel,
                    #[cfg_attr(feature = "extra", doc = "note")]
                    stable: StableShared,
                }
            },
        )
        .unwrap()
        .to_string();
        assert_eq!(
            output
                .matches("cfg_attr (feature = \"extra\" , cfg")
                .count(),
            2
        );
        assert_eq!(
            output
                .matches("cfg_attr (feature = \"extra\" , doc")
                .count(),
            1
        );
        assert!(!output.contains("ConditionalModel : :: mirui :: core :: model :: BindType"));
        assert!(output.contains("StableShared : :: mirui :: core :: model :: BindType"));
        assert!(output.contains("StableShared as :: mirui :: core :: model :: BindType"));
    }
}
