//! Shared typed parameter and access-plan lowering.
use crate::binding::{
    BindingAccess, BindingCapability, BindingCardinality, BindingSource, BindingSpec, ModelSource,
    QueryDriver, QueryFilterKind, QueryItemSpec, QuerySpec,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Type;
use syn::spanned::Spanned;

pub(crate) fn lower_fetched_parameter(
    binding: &BindingSpec,
) -> syn::Result<(TokenStream, Vec<TokenStream>)> {
    match &binding.source {
        BindingSource::Resource(resource) => {
            let ty = &resource.ty;
            let (marker, access) = match (binding.access, binding.cardinality) {
                (BindingAccess::Read, BindingCardinality::Required) => (
                    quote!(::mirui::__macro_support::Res<#ty>),
                    quote!(::mirui::__macro_support::SystemAccess::read_resource::<#ty>()),
                ),
                (BindingAccess::Write, BindingCardinality::Required) => (
                    quote!(::mirui::__macro_support::ResMut<#ty>),
                    quote!(::mirui::__macro_support::SystemAccess::write_resource::<#ty>()),
                ),
                (BindingAccess::Read, BindingCardinality::Optional) => (
                    quote!(::mirui::__macro_support::OptionalRes<#ty>),
                    quote!(::mirui::__macro_support::SystemAccess::read_resource::<#ty>()),
                ),
                _ => {
                    return Err(syn::Error::new(
                        binding.syntax.span(),
                        "unsupported resource binding",
                    ));
                }
            };
            Ok((marker, vec![access]))
        }
        BindingSource::Query(query) => lower_query(query),
        _ => Err(syn::Error::new(
            binding.syntax.span(),
            "expected a resource or query binding",
        )),
    }
}

pub(crate) fn lower_helper_parameter(
    binding: &BindingSpec,
    context: &str,
) -> syn::Result<TokenStream> {
    let name = &binding.name;
    match &binding.source {
        BindingSource::Model(ModelSource::Registered { ty }) => {
            Ok(quote!(#name: &<#ty as ::mirui::core::model::BindType>::Shared))
        }
        BindingSource::Resource(resource) => {
            let ty = &resource.ty;
            match (binding.access, binding.cardinality) {
                (BindingAccess::Read, BindingCardinality::Required) => {
                    Ok(quote!(#name: ::mirui::ecs::resource::ResourceRef<'_, #ty>))
                }
                (BindingAccess::Write, BindingCardinality::Required) => Ok(
                    quote!(#[allow(unused_mut)] mut #name: ::mirui::ecs::resource::ResourceMut<'_, #ty>),
                ),
                (BindingAccess::Read, BindingCardinality::Optional) => Ok(
                    quote!(#name: ::core::option::Option<::mirui::ecs::resource::ResourceRef<'_, #ty>>),
                ),
                _ => Err(syn::Error::new(
                    binding.syntax.span(),
                    "unsupported resource binding",
                )),
            }
        }
        BindingSource::Query(query) => {
            let marker = lower_query(query)?.0;
            let mutable = query.items.iter().any(|item| {
                matches!(
                    item,
                    QueryItemSpec::Component {
                        access: BindingAccess::Write,
                        ..
                    }
                )
            });
            let pattern = if mutable {
                quote!(#[allow(unused_mut)] mut #name)
            } else {
                quote!(#name)
            };
            Ok(quote!(
                #pattern: <#marker as ::mirui::__macro_support::SystemParam>::Item<'_>
            ))
        }
        BindingSource::Capability(BindingCapability::Commands) => Ok(quote!(
            mut #name: ::mirui::__macro_support::DeferredCommands<'_>
        )),
        _ => Err(syn::Error::new(
            binding.syntax.span(),
            format!("binding role is not available in `{context}`"),
        )),
    }
}

fn lower_query(query: &QuerySpec) -> syn::Result<(TokenStream, Vec<TokenStream>)> {
    let mut items = Vec::with_capacity(query.items.len());
    let mut metadata = Vec::new();
    for item in &query.items {
        match item {
            QueryItemSpec::Entity(_) => items.push(quote!(::mirui::__macro_support::EntityItem)),
            QueryItemSpec::Component {
                ty,
                access,
                cardinality,
            } => {
                let marker = match (*access, *cardinality) {
                    (BindingAccess::Read, BindingCardinality::Required) => {
                        metadata.push(
                            quote!(::mirui::__macro_support::SystemAccess::read_component::<#ty>()),
                        );
                        quote!(::mirui::__macro_support::Read<#ty>)
                    }
                    (BindingAccess::Write, BindingCardinality::Required) => {
                        metadata.push(
                            quote!(::mirui::__macro_support::SystemAccess::write_component::<#ty>()),
                        );
                        quote!(::mirui::__macro_support::Write<#ty>)
                    }
                    (BindingAccess::Read, BindingCardinality::Optional) => {
                        metadata.push(
                            quote!(::mirui::__macro_support::SystemAccess::read_component::<#ty>()),
                        );
                        quote!(::mirui::__macro_support::OptionalRead<#ty>)
                    }
                    _ => {
                        return Err(syn::Error::new_spanned(
                            ty,
                            "unsupported query item binding",
                        ));
                    }
                };
                items.push(marker);
            }
        }
    }

    let driver_ty = match query.driver {
        QueryDriver::Item(index) => match query.items.get(index) {
            Some(QueryItemSpec::Component { ty, .. }) => ty,
            _ => return Err(syn::Error::new(query_span(query), "invalid query driver")),
        },
        QueryDriver::WithFilter(index) => match query.filters.get(index) {
            Some(filter) if filter.kind == QueryFilterKind::With => &filter.ty,
            _ => return Err(syn::Error::new(query_span(query), "invalid query driver")),
        },
    };
    let driver = quote!(::mirui::__macro_support::ComponentDriver<#driver_ty>);
    let data = if items.len() == 1 {
        items[0].clone()
    } else {
        quote!((#(#items,)*))
    };
    let filters = query.filters.iter().map(|filter| {
        let ty = &filter.ty;
        match filter.kind {
            QueryFilterKind::With => quote!(::mirui::__macro_support::With<#ty>),
            QueryFilterKind::Without => quote!(::mirui::__macro_support::Without<#ty>),
        }
    });
    let filters = if query.filters.is_empty() {
        quote!(())
    } else {
        quote!((#(#filters,)*))
    };
    Ok((
        quote!(::mirui::__macro_support::QueryParam<#driver, #data, #filters>),
        metadata,
    ))
}

pub(crate) fn query_dependency_types(query: &QuerySpec) -> impl Iterator<Item = &Type> {
    query
        .items
        .iter()
        .filter_map(|item| match item {
            QueryItemSpec::Entity(_) => None,
            QueryItemSpec::Component { ty, .. } => Some(ty.as_ref()),
        })
        .chain(query.filters.iter().map(|filter| &filter.ty))
}

fn query_span(query: &QuerySpec) -> proc_macro2::Span {
    query
        .items
        .first()
        .map(|item| match item {
            QueryItemSpec::Entity(entity) => entity.span(),
            QueryItemSpec::Component { ty, .. } => ty.span(),
        })
        .unwrap_or_else(proc_macro2::Span::call_site)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    #[test]
    fn query_marker_and_dependency_types_come_from_the_same_shared_spec() {
        let parameter: syn::FnArg = syn::parse_quote!(
            rows: query!(Entity, Position, optional Velocity; with Active; without Hidden;)
        );
        let binding = BindingSpec::parse_system_parameter(&parameter).unwrap();
        let BindingSource::Query(query) = &binding.source else {
            panic!("expected a query binding");
        };
        let types = query_dependency_types(query)
            .map(|ty| ty.to_token_stream().to_string())
            .collect::<Vec<_>>();
        assert_eq!(types, ["Position", "Velocity", "Active", "Hidden"]);
        let marker: syn::TypePath =
            syn::parse2(lower_fetched_parameter(&binding).unwrap().0).unwrap();
        let syn::PathArguments::AngleBracketed(arguments) =
            &marker.path.segments.last().unwrap().arguments
        else {
            panic!("expected typed query arguments");
        };
        assert_eq!(
            arguments
                .args
                .iter()
                .map(|argument| argument.to_token_stream().to_string())
                .collect::<Vec<_>>(),
            [
                quote!(::mirui::__macro_support::ComponentDriver<Position>),
                quote!((
                    ::mirui::__macro_support::EntityItem,
                    ::mirui::__macro_support::Read<Position>,
                    ::mirui::__macro_support::OptionalRead<Velocity>,
                )),
                quote!((
                    ::mirui::__macro_support::With<Active>,
                    ::mirui::__macro_support::Without<Hidden>,
                )),
            ]
            .map(|argument| argument.to_string()),
        );
    }
}
