use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ImplItem, Item, Pat, parse_quote, spanned::Spanned};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    if !attr.is_empty() {
        return Err(syn::Error::new_spanned(
            attr,
            "model options are not supported by this declaration",
        ));
    }
    match syn::parse2::<Item>(item)? {
        Item::Struct(item) => expand_struct(item),
        Item::Impl(item) => expand_impl(item),
        other => Err(syn::Error::new_spanned(
            other,
            "#[model] requires a struct or inherent impl",
        )),
    }
}

fn expand_struct(mut item: syn::ItemStruct) -> syn::Result<TokenStream> {
    let name = &item.ident;
    let handle = format_ident!("{}Handle", name);
    let snapshot = format_ident!("{}ObservedSnapshot", name);
    let visibility = &item.vis;
    let generics = &item.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let mut observed = Vec::new();
    for field in &mut item.fields {
        let markers: Vec<_> = field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("observe"))
            .collect();
        if markers.is_empty() {
            continue;
        }
        if markers.len() > 1 {
            return Err(syn::Error::new_spanned(
                field,
                "duplicate #[observe] marker",
            ));
        }
        if !matches!(markers[0].meta, syn::Meta::Path(_)) {
            return Err(syn::Error::new_spanned(
                markers[0],
                "#[observe] does not take arguments",
            ));
        }
        let Some(field_name) = &field.ident else {
            return Err(syn::Error::new_spanned(
                field,
                "#[observe] requires a named field",
            ));
        };
        field.attrs.retain(|attr| !attr.path().is_ident("observe"));
        observed.push((field_name.clone(), field.ty.clone(), field.vis.clone()));
    }
    let observed_names: Vec<_> = observed.iter().map(|(name, _, _)| name).collect();
    let observed_types: Vec<_> = observed.iter().map(|(_, ty, _)| ty).collect();
    let source_count = observed.len();
    let mut model_generics = generics.clone();
    model_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#name #type_generics: 'static));
    for ty in &observed_types {
        model_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: Copy + Eq + 'static));
    }
    let (_, _, model_where) = model_generics.split_for_impl();

    let mut snapshot_generics = generics.clone();
    for ty in &observed_types {
        snapshot_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: Copy + Eq + 'static));
    }
    let (snapshot_impl_generics, _, snapshot_where) = snapshot_generics.split_for_impl();
    let snapshot_definition = if observed.is_empty() {
        quote!()
    } else {
        quote! {
            #visibility struct #snapshot #snapshot_generics {
                values: (#(#observed_types,)*),
                marker: ::core::marker::PhantomData<#name #type_generics>,
            }

            impl #snapshot_impl_generics ::core::marker::Copy for #snapshot #type_generics #snapshot_where {}

            impl #snapshot_impl_generics ::core::clone::Clone for #snapshot #type_generics #snapshot_where {
                fn clone(&self) -> Self { *self }
            }
        }
    };
    let snapshot_type = if observed.is_empty() {
        quote!(())
    } else {
        quote!(#snapshot #type_generics)
    };
    let snapshot_value = if observed.is_empty() {
        quote!()
    } else {
        quote!(#snapshot {
            values: (#(self.#observed_names,)*),
            marker: ::core::marker::PhantomData,
        })
    };
    let publish = observed.iter().enumerate().map(|(index, _)| {
        let index = syn::Index::from(index);
        quote! {
            if before.values.#index != after.values.#index {
                sources[#index].notify();
            }
        }
    });
    let publish_body = if observed.is_empty() {
        quote!(let _ = (sources, before, after);)
    } else {
        quote!(#(#publish)*)
    };
    let accessors = observed.iter().enumerate().map(|(index, (field, ty, vis))| {
        quote! {
            #vis fn #field(&self) -> #ty {
                ::mirui::core::model::ModelHandle::read_observed(self, #index, |data| data.#field)
            }
        }
    });

    Ok(quote! {
        #item

        #snapshot_definition

        #visibility struct #handle #generics #where_clause {
            cell: ::mirui::__Weak<::mirui::core::model::ModelCell<#name #type_generics>>,
        }

        impl #impl_generics ::core::clone::Clone for #handle #type_generics #where_clause {
            fn clone(&self) -> Self {
                Self { cell: self.cell.clone() }
            }
        }

        impl #impl_generics ::mirui::core::model::Model for #name #type_generics #model_where {
            type Handle = #handle #type_generics;
            type Snapshot = #snapshot_type;
            type Sources = [::mirui::core::reactive::ModelSource; #source_count];

            fn handle(
                cell: ::mirui::__Weak<::mirui::core::model::ModelCell<Self>>,
            ) -> Self::Handle {
                #handle { cell }
            }

            fn snapshot(&self) -> Self::Snapshot {
                #snapshot_value
            }

            fn sources() -> Self::Sources {
                ::core::array::from_fn(|_| ::mirui::core::reactive::ModelSource::new())
            }

            fn publish(sources: &Self::Sources, before: Self::Snapshot, after: Self::Snapshot) {
                #publish_body
            }
        }

        impl #impl_generics ::mirui::core::model::ModelHandle for #handle #type_generics #model_where {
            type Data = #name #type_generics;

            fn cell(&self) -> &::mirui::__Weak<::mirui::core::model::ModelCell<Self::Data>> {
                &self.cell
            }
        }

        impl #impl_generics ::mirui::core::model::BindType for #name #type_generics #model_where {
            type Shared = #handle #type_generics;
        }

        impl #impl_generics ::mirui::core::model::BindType for #handle #type_generics #model_where {
            type Shared = Self;
        }

        impl #impl_generics ::mirui::core::model::SharedValue for #handle #type_generics #model_where {}

        impl #impl_generics #handle #type_generics #model_where {
            #(#accessors)*
        }
    })
}

fn expand_impl(item: syn::ItemImpl) -> syn::Result<TokenStream> {
    if item.trait_.is_some() {
        return Err(syn::Error::new_spanned(
            item,
            "#[model] only supports inherent impl blocks",
        ));
    }
    let syn::Type::Path(self_type) = &*item.self_ty else {
        return Err(syn::Error::new_spanned(
            &item.self_ty,
            "model impl target must be a local named type",
        ));
    };
    if self_type.qself.is_some() || self_type.path.segments.len() != 1 {
        return Err(syn::Error::new_spanned(
            &item.self_ty,
            "model impl target must use its local type name",
        ));
    }
    let name = &self_type.path.segments[0].ident;
    let handle = format_ident!("{}Handle", name);
    let (impl_generics, _, where_clause) = item.generics.split_for_impl();
    let type_args = &self_type.path.segments[0].arguments;
    let mut forwards = Vec::new();
    for member in &item.items {
        let ImplItem::Fn(method) = member else {
            continue;
        };
        let Some(FnArg::Receiver(receiver)) = method.sig.inputs.first() else {
            continue;
        };
        if method.sig.asyncness.is_some()
            || method.sig.unsafety.is_some()
            || method.sig.constness.is_some()
            || receiver.reference.is_none()
        {
            return Err(syn::Error::new(
                method.sig.span(),
                "model methods must be synchronous safe methods taking &self or &mut self",
            ));
        }
        let mut argument_names = Vec::new();
        let mut arguments = Vec::new();
        for argument in method.sig.inputs.iter().skip(1) {
            let FnArg::Typed(argument) = argument else {
                return Err(syn::Error::new_spanned(argument, "unexpected receiver"));
            };
            let Pat::Ident(ident) = &*argument.pat else {
                return Err(syn::Error::new_spanned(
                    &argument.pat,
                    "model method arguments must use names",
                ));
            };
            argument_names.push(&ident.ident);
            arguments.push(argument);
        }
        let attrs = &method.attrs;
        let visibility = &method.vis;
        let method_name = &method.sig.ident;
        let output = &method.sig.output;
        let (fn_generics, _, fn_where) = method.sig.generics.split_for_impl();
        let call = quote!(data.#method_name(#(#argument_names),*));
        let invoke = if receiver.mutability.is_some() {
            quote!(::mirui::core::model::ModelHandle::update(self, |data| #call))
        } else {
            quote!(::mirui::core::model::ModelHandle::read(self, |data| #call))
        };
        forwards.push(quote! {
            #(#attrs)*
            #visibility fn #method_name #fn_generics (&self, #(#arguments),*) #output #fn_where {
                #invoke
            }
        });
    }
    Ok(quote! {
        #item

        impl #impl_generics #handle #type_args #where_clause {
            #(#forwards)*
        }
    })
}

#[cfg(test)]
mod tests {
    use super::expand;
    use quote::quote;

    #[test]
    fn observed_fields_generate_independent_sources() {
        let expanded = expand(
            quote!(),
            quote! {
                struct Counter {
                    #[observe]
                    count: u32,
                    #[observe]
                    active: bool,
                    scratch: u32,
                }
            },
        )
        .unwrap()
        .to_string();
        assert!(expanded.contains("ModelSource ; 2"));
        assert!(expanded.contains("fn count"));
        assert!(expanded.contains("fn active"));
        assert!(!expanded.contains("fn scratch"));
    }

    #[test]
    fn rejects_duplicate_observe_markers() {
        let error = expand(
            quote!(),
            quote!(
                struct Counter {
                    #[observe]
                    #[observe]
                    value: u32,
                }
            ),
        )
        .unwrap_err();
        assert!(error.to_string().contains("duplicate"));
    }

    #[test]
    fn rejects_observe_arguments() {
        let error = expand(
            quote!(),
            quote!(
                struct Counter {
                    #[observe(change)]
                    value: u32,
                }
            ),
        )
        .unwrap_err();
        assert!(error.to_string().contains("does not take arguments"));
    }
}
