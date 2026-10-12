use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Attribute, Generics, ItemStruct, LitStr, Meta, Type, ext::IdentExt, parse_quote,
    punctuated::Punctuated,
};

use crate::function_attrs::cfg_attrs;

pub(super) struct Options {
    key: Option<LitStr>,
}

#[cfg(test)]
mod tests;

impl Options {
    pub fn parse(meta: Meta) -> syn::Result<Self> {
        let mut options = Self { key: None };
        match meta {
            Meta::Path(_) => {}
            Meta::List(list) => {
                for entry in
                    list.parse_args_with(Punctuated::<Meta, syn::Token![,]>::parse_terminated)?
                {
                    match entry {
                        Meta::NameValue(value) if value.path.is_ident("key") => {
                            if options.key.is_some() {
                                return Err(syn::Error::new_spanned(
                                    value,
                                    "duplicate persistence key",
                                ));
                            }
                            let syn::Expr::Lit(syn::ExprLit {
                                lit: syn::Lit::Str(key),
                                ..
                            }) = value.value
                            else {
                                return Err(syn::Error::new_spanned(
                                    value,
                                    "persistence key must be a string literal",
                                ));
                            };
                            if key.value().is_empty() {
                                return Err(syn::Error::new_spanned(
                                    key,
                                    "persistence key cannot be empty",
                                ));
                            }
                            options.key = Some(key);
                        }
                        other => {
                            return Err(syn::Error::new_spanned(other, "expected key = \"...\""));
                        }
                    }
                }
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "expected persistence or persistence(key = \"...\")",
                ));
            }
        }
        Ok(options)
    }
}

pub(super) struct Field {
    name: syn::Ident,
    stored_name: String,
    ty: Type,
    attrs: Vec<Attribute>,
}

pub(super) fn take_fields(item: &mut ItemStruct, enabled: bool) -> syn::Result<Vec<Field>> {
    let mut fields = Vec::new();
    for field in &mut item.fields {
        let mut found = false;
        for attr in field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("persist"))
        {
            if !enabled {
                return Err(syn::Error::new_spanned(
                    attr,
                    "#[persist] requires #[model(persistence)]",
                ));
            }
            if found {
                return Err(syn::Error::new_spanned(attr, "duplicate #[persist] marker"));
            }
            if !matches!(attr.meta, Meta::Path(_)) {
                return Err(syn::Error::new_spanned(
                    attr,
                    "#[persist] takes no arguments",
                ));
            }
            found = true;
        }
        if !found {
            continue;
        }
        let Some(name) = field.ident.clone() else {
            return Err(syn::Error::new_spanned(
                field,
                "#[persist] requires a named field",
            ));
        };
        field.attrs.retain(|attr| !attr.path().is_ident("persist"));
        fields.push(Field {
            stored_name: name.unraw().to_string(),
            name,
            ty: field.ty.clone(),
            attrs: cfg_attrs(&field.attrs),
        });
    }
    fields.sort_by(|a, b| a.stored_name.cmp(&b.stored_name));
    Ok(fields)
}

pub(super) fn expand(
    item: &ItemStruct,
    generics: &Generics,
    fields: &[Field],
    options: &Options,
) -> TokenStream {
    let name = &item.ident;
    let visibility = &item.vis;
    let owned = format_ident!("{}Owned", name);
    let prepared = format_ident!("{}PersistencePrepared", name);
    let mut marker = format_ident!("__mirui_marker");
    while fields.iter().any(|field| marker == field.stored_name) {
        marker = format_ident!("{}_", marker);
    }
    let mut generics = generics.clone();
    for field in fields
        .iter()
        .filter(|field| !super::may_be_cfg_removed(&field.attrs))
    {
        let ty = &field.ty;
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: ::mirui::core::persistence::value::PersistValue));
    }
    let (params, args, bounds) = generics.split_for_impl();
    let fields_schema = fields.iter().map(
        |Field {
             stored_name,
             ty,
             attrs,
             ..
         }| {
            quote! {
                #(#attrs)* ::mirui::core::persistence::archive::Field {
                    name: #stored_name,
                    schema: &<#ty as ::mirui::core::persistence::value::PersistValue>::SCHEMA,
                },
            }
        },
    );
    let patch_fields = fields.iter().map(|Field { name, ty, attrs, .. }| quote! {
        #(#attrs)* #name: ::core::option::Option<<#ty as ::mirui::core::persistence::value::PersistValue>::Prepared>,
    });
    let encode = fields.iter().map(|Field { name, attrs, .. }| quote! {
        #(#attrs)* __writer.sized(|__writer| ::mirui::core::persistence::value::PersistValue::encode(&self.#name, __writer))?;
    });
    let prepare = fields.iter().map(|Field { name, stored_name, attrs, .. }| quote! {
        #(#attrs)* #name: ::mirui::core::persistence::value::prepare_field(&self.#name, __value, #stored_name)?,
    });
    let apply = fields.iter().map(|Field { name, attrs, .. }| {
        quote! {
            #(#attrs)* if let ::core::option::Option::Some(__field) = __prepared.#name {
                ::mirui::core::persistence::value::PersistValue::apply(&mut self.#name, __field);
            }
        }
    });
    let key = options.key.as_ref().map_or_else(
        || quote!(::core::option::Option::None),
        |key| quote!(::core::option::Option::Some(#key)),
    );
    quote! {
        ::mirui::__mirui_persistence! {
            #[doc(hidden)]
            #visibility struct #prepared #generics #bounds {
                #(#patch_fields)*
                #marker: ::core::marker::PhantomData<fn() -> #name #args>,
            }

            impl #params ::mirui::core::persistence::value::PersistValue for #name #args #bounds {
                const SCHEMA: ::mirui::core::persistence::archive::Schema = ::mirui::core::persistence::archive::Schema::Record(&[#(#fields_schema)*]);
                type Prepared = #prepared #args;
                fn encode(&self, __writer: &mut ::mirui::core::persistence::archive::Writer<'_>) -> ::core::result::Result<(), ::mirui::core::persistence::PersistenceError> {
                    #(#encode)*
                    Ok(())
                }
                fn prepare(&self, __value: ::mirui::core::persistence::archive::Value<'_>) -> ::core::result::Result<Self::Prepared, ::mirui::core::persistence::PersistenceError> {
                    if !__value.matches(&<Self as ::mirui::core::persistence::value::PersistValue>::SCHEMA) { return Err(::mirui::core::persistence::PersistenceError::InvalidArchive); }
                    Ok(#prepared { #(#prepare)* #marker: ::core::marker::PhantomData })
                }
                fn apply(&mut self, __prepared: Self::Prepared) { #(#apply)* }
            }

            impl #params ::mirui::core::persistence::value::PersistModel for #name #args #bounds {
                const KEY: ::core::option::Option<&'static str> = #key;
            }

            impl #params ::mirui::core::persistence::value::PersistValue for #owned #args #bounds {
                const SCHEMA: ::mirui::core::persistence::archive::Schema = <#name #args as ::mirui::core::persistence::value::PersistValue>::SCHEMA;
                type Prepared = <#name #args as ::mirui::core::persistence::value::PersistValue>::Prepared;
                fn encode(&self, __writer: &mut ::mirui::core::persistence::archive::Writer<'_>) -> ::core::result::Result<(), ::mirui::core::persistence::PersistenceError> {
                    ::mirui::core::model::OwnedModel::read(self, |__value| ::mirui::core::persistence::value::PersistValue::encode(__value, __writer))
                }
                fn prepare(&self, __value: ::mirui::core::persistence::archive::Value<'_>) -> ::core::result::Result<Self::Prepared, ::mirui::core::persistence::PersistenceError> {
                    ::mirui::core::model::OwnedModel::read(self, |__current| ::mirui::core::persistence::value::PersistValue::prepare(__current, __value))
                }
                fn apply(&mut self, __prepared: Self::Prepared) {
                    ::mirui::core::model::OwnedModel::storage_mut(self).apply_persisted(__prepared);
                }
            }
        }
    }
}
