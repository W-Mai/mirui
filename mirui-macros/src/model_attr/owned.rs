use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Generics, ItemStruct, Type, parse_quote};

use super::accessor_attrs;
use crate::function_attrs::cfg_attrs;

pub(super) struct OwnedField {
    index: usize,
    pub name: syn::Ident,
    pub ty: Type,
    pub visibility: syn::Visibility,
    pub attrs: Vec<Attribute>,
    pub cfg_attrs: Vec<Attribute>,
}

mod keyword {
    syn::custom_keyword!(owned);
}

struct OwnedRole(Type);

impl syn::parse::Parse for OwnedRole {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<keyword::owned>()?;
        Ok(Self(input.parse()?))
    }
}

pub(super) fn lower_fields(item: &mut ItemStruct) -> syn::Result<Vec<OwnedField>> {
    let mut fields = Vec::new();
    for field in &mut item.fields {
        let Type::Macro(role) = &field.ty else {
            continue;
        };
        if !role
            .mac
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "model")
        {
            continue;
        }
        let OwnedRole(ty) = syn::parse2(role.mac.tokens.clone())?;
        let Some(name) = field.ident.clone() else {
            return Err(syn::Error::new_spanned(
                field,
                "owned model fields require a name",
            ));
        };
        if field
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("observe"))
        {
            return Err(syn::Error::new_spanned(
                field,
                "observe the owned model's values or derived methods",
            ));
        }
        let conditional = cfg_attrs(&field.attrs);
        field.ty = parse_quote!(<#ty as ::mirui::core::model::Model>::Owned);
        fields.push(OwnedField {
            index: fields.len(),
            name,
            ty,
            visibility: field.vis.clone(),
            attrs: accessor_attrs(&field.attrs),
            cfg_attrs: conditional,
        });
    }
    Ok(fields)
}

pub(super) fn children_type(item: &ItemStruct, fields: &[OwnedField]) -> TokenStream {
    if fields.is_empty() {
        return quote!(());
    }
    let name = format_ident!("{}Children", item.ident);
    let (_, args, _) = item.generics.split_for_impl();
    quote!(#name #args)
}

fn field_type(parent: &syn::Ident, index: usize) -> syn::Ident {
    format_ident!("{}ModelField{}", parent, index)
}

pub(super) fn expand(
    item: &ItemStruct,
    fields: &[OwnedField],
    model_generics: &Generics,
) -> TokenStream {
    let name = &item.ident;
    let owned = format_ident!("{}Owned", name);
    let handle = format_ident!("{}Handle", name);
    let visibility = &item.vis;
    let (params, args, bounds) = model_generics.split_for_impl();
    let raw = quote!(#name #args);
    let shared_getters = fields.iter().map(|field| {
        let name = &field.name;
        let ty = &field.ty;
        let visibility = &field.visibility;
        let attrs = &field.attrs;
        let path = field_type(&item.ident, field.index);
        quote! {
            #(#attrs)*
            #visibility fn #name(&self) -> <#ty as ::mirui::core::model::Model>::Handle {
                ::mirui::core::model::ModelHandle::child::<#path #args>(self)
            }
        }
    });
    let wrappers = quote! {
        #[doc(hidden)]
        #visibility struct #owned #model_generics #bounds {
            __mirui_storage: ::mirui::core::model::OwnedStorage<#raw>,
        }

        impl #params ::core::convert::From<#raw> for #owned #args #bounds {
            fn from(value: #raw) -> Self {
                Self { __mirui_storage: ::mirui::core::model::OwnedStorage::new(value) }
            }
        }

        impl #params ::mirui::core::model::OwnedModel for #owned #args #bounds {
            type Data = #raw;
            fn storage(&self) -> &::mirui::core::model::OwnedStorage<#raw> { &self.__mirui_storage }
            fn storage_mut(&mut self) -> &mut ::mirui::core::model::OwnedStorage<#raw> { &mut self.__mirui_storage }
        }

        impl #params #handle #args #bounds { #(#shared_getters)* }
    };
    if fields.is_empty() {
        return wrappers;
    }
    let children = format_ident!("{}Children", name);
    let snapshot = format_ident!("{}ChildrenSnapshot", name);
    let events = format_ident!("{}ChildrenEvents", name);
    let pending = format_ident!("{}ChildrenPending", name);
    let fields_for = |kind: &str| {
        fields
            .iter()
            .map(|field| {
                let field_name = &field.name;
                let ty = &field.ty;
                let attrs = &field.cfg_attrs;
                let kind = format_ident!("{kind}");
                quote!(#(#attrs)* #field_name: ::mirui::core::model::#kind<#ty>,)
            })
            .collect::<Vec<_>>()
    };
    let metadata_fields = fields_for("ChildNode");
    let snapshot_fields = fields_for("ModelSnapshot");
    let event_fields = fields_for("ModelEvents");
    let pending_fields = fields_for("ModelPending");
    let registrations = fields.iter().map(|field| {
        let field_name = &field.name;
        let path = field_type(name, field.index);
        let attrs = &field.cfg_attrs;
        quote! {
            #(#attrs)* #field_name: ::mirui::core::model::ChildNode::new::<__MiruiRoot, ::mirui::core::model::ModelPathThen<__MiruiPath, #path #args>>(__root, __owner, ::mirui::core::model::ModelPathThen::new(__path)),
        }
    });
    let snapshots = fields.iter().map(|field| {
        let name = &field.name;
        let ty = &field.ty;
        let attrs = &field.cfg_attrs;
        quote! {
            #(#attrs)* #name: ::mirui::core::model::OwnedModel::read(&__value.#name, ::mirui::core::model::ModelMetadata::<#ty>::snapshot),
        }
    });
    let extracts = fields.iter().map(|field| {
        let name = &field.name;
        let attrs = &field.cfg_attrs;
        quote! {
            #(#attrs)* #name: ::mirui::core::model::OwnedModel::storage_mut(&mut __value.#name).take_events(),
        }
    });
    let finishes = fields.iter().map(|field| {
        let name = &field.name;
        let attrs = &field.cfg_attrs;
        quote!(#(#attrs)* #name: self.#name.finish(::mirui::core::model::OwnedModel::storage_mut(&mut __value.#name), __reset),)
    });
    let publications = fields.iter().map(|field| {
        let name = &field.name;
        let attrs = &field.cfg_attrs;
        quote!(#(#attrs)* self.#name.publish(__before.#name, __after.#name, __pending.#name);)
    });
    let deliveries = fields.iter().map(|field| {
        let name = &field.name;
        let attrs = &field.cfg_attrs;
        quote!(#(#attrs)* self.#name.deliver(__events.#name);)
    });
    let invalidations = fields.iter().map(|field| {
        let name = &field.name;
        let attrs = &field.cfg_attrs;
        quote!(#(#attrs)* self.#name.invalidate();)
    });
    let paths = fields.iter().map(|field| {
        let field_name = &field.name;
        let ty = &field.ty;
        let path = field_type(name, field.index);
        let attrs = &field.cfg_attrs;
        quote! {
            #(#attrs)*
            struct #path #model_generics #bounds { __mirui_marker: ::core::marker::PhantomData<fn() -> #raw> }
            #(#attrs)*
            impl #params ::mirui::core::model::ModelChildField<#raw> for #path #args #bounds {
                type Child = #ty;
                fn storage(value: &#raw) -> &::mirui::core::model::OwnedStorage<#ty> {
                    ::mirui::core::model::OwnedModel::storage(&value.#field_name)
                }
                fn storage_mut(value: &mut #raw) -> &mut ::mirui::core::model::OwnedStorage<#ty> {
                    ::mirui::core::model::OwnedModel::storage_mut(&mut value.#field_name)
                }
                fn node(metadata: &::mirui::core::model::ModelMetadata<#raw>) -> &::mirui::core::model::ChildNode<#ty> {
                    &metadata.children().#field_name
                }
            }
        }
    });
    quote! {
        #wrappers
        #[doc(hidden)]
        #visibility struct #children #model_generics #bounds { #(#metadata_fields)* __mirui_marker: ::core::marker::PhantomData<fn() -> #raw> }
        #[doc(hidden)]
        #visibility struct #snapshot #model_generics #bounds { #(#snapshot_fields)* __mirui_marker: ::core::marker::PhantomData<fn() -> #raw> }
        impl #params ::core::marker::Copy for #snapshot #args #bounds {}
        impl #params ::core::clone::Clone for #snapshot #args #bounds { fn clone(&self) -> Self { *self } }
        #[doc(hidden)]
        #visibility struct #events #model_generics #bounds { #(#event_fields)* __mirui_marker: ::core::marker::PhantomData<fn() -> #raw> }
        #[doc(hidden)]
        #visibility struct #pending #model_generics #bounds { #(#pending_fields)* __mirui_marker: ::core::marker::PhantomData<fn() -> #raw> }
        #(#paths)*
        impl #params ::mirui::core::model::ModelChildren<#raw> for #children #args #bounds {
            type Snapshot = #snapshot #args;
            type Events = #events #args;
            type Pending = #pending #args;
            fn register<__MiruiRoot, __MiruiPath>(__root: &::mirui::__macro_support::__Weak<::mirui::core::model::ModelRoot<__MiruiRoot>>, __owner: ::mirui::core::model::BindingOwner, __path: __MiruiPath) -> Self
            where __MiruiRoot: ::mirui::core::model::Model, __MiruiPath: ::mirui::core::model::ModelPath<__MiruiRoot, Target = #raw> {
                Self { #(#registrations)* __mirui_marker: ::core::marker::PhantomData }
            }
            fn snapshot(__value: &#raw) -> Self::Snapshot {
                #snapshot { #(#snapshots)* __mirui_marker: ::core::marker::PhantomData }
            }
            fn take_events(__value: &mut #raw) -> Self::Events {
                #events { #(#extracts)* __mirui_marker: ::core::marker::PhantomData }
            }
            fn finish(&self, __value: &mut #raw, __reset: bool) -> Self::Pending {
                #pending { #(#finishes)* __mirui_marker: ::core::marker::PhantomData }
            }
            fn publish(&self, __before: Self::Snapshot, __after: Self::Snapshot, __pending: Self::Pending) { #(#publications)* }
            fn deliver(&self, __events: Self::Events) { #(#deliveries)* }
            fn invalidate(&self) { #(#invalidations)* }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::lower_fields;
    use quote::quote;
    use syn::{ItemStruct, parse_quote};

    #[test]
    fn owned_fields_require_named_slots_and_observe_their_own_values() {
        for mut item in [
            parse_quote!(
                struct Tuple(model!(owned Child));
            ),
            parse_quote!(
                struct Parent {
                    #[observe]
                    child: model!(owned Child),
                }
            ),
        ] {
            assert!(lower_fields(&mut item).is_err());
        }
    }

    #[test]
    fn owned_fields_preserve_authored_bounds_without_exposing_child_types() {
        let mut item: ItemStruct = parse_quote! {
            struct Parent<T> where T: Model {
                child: model!(owned T),
                #[cfg(any())]
                missing: model!(owned NotDefined),
            }
        };
        let fields = lower_fields(&mut item).unwrap();
        assert_eq!(fields.len(), 2);
        let predicates = &item.generics.where_clause.unwrap().predicates;
        assert_eq!(predicates.len(), 1);
        assert!(!quote!(#predicates).to_string().contains("NotDefined"));
        assert!(fields[0].cfg_attrs.is_empty());
        assert_eq!(fields[1].cfg_attrs.len(), 1);
    }
}
