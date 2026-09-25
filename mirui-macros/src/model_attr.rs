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

fn expand_struct(item: syn::ItemStruct) -> syn::Result<TokenStream> {
    let name = &item.ident;
    let handle = format_ident!("{}Handle", name);
    let visibility = &item.vis;
    let generics = &item.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let mut model_generics = generics.clone();
    model_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#name #type_generics: 'static));
    let (_, _, model_where) = model_generics.split_for_impl();

    Ok(quote! {
        #item

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

            fn handle(
                cell: ::mirui::__Weak<::mirui::core::model::ModelCell<Self>>,
            ) -> Self::Handle {
                #handle { cell }
            }
        }

        impl #impl_generics ::mirui::core::model::ModelHandle for #handle #type_generics #model_where {
            type Data = #name #type_generics;

            fn cell(&self) -> &::mirui::__Weak<::mirui::core::model::ModelCell<Self::Data>> {
                &self.cell
            }
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
