use crate::binding::{
    BindingCardinality, BindingSource, BindingSpec, COMPONENT_FIELD_ROLES, ModelSource, RoleKind,
};
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::visit::Visit;
use syn::{
    Field, Fields, Generics, Item, Meta, Path, Type, punctuated::Punctuated, spanned::Spanned,
};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    reject_attribute_arguments(attr)?;
    let mut item = syn::parse2::<Item>(item)?;
    let (name, generics, clone_impl) = match &mut item {
        Item::Struct(item) => {
            let clone_impl = match &mut item.fields {
                Fields::Named(fields) => {
                    let mut model_fields = Vec::new();
                    for field in &mut fields.named {
                        if lower_model_field(field, &mut item.generics)? {
                            model_fields.push(field.ident.clone().expect("named field"));
                        }
                    }

                    if model_fields.is_empty() {
                        None
                    } else {
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
                                let name = field.ident.as_ref().expect("named field");
                                if model_fields.contains(name) {
                                    continue;
                                }
                                if field
                                    .attrs
                                    .iter()
                                    .any(|attr| may_remove_field(&attr.meta))
                                {
                                    for parameter in
                                        referenced_type_parameters(&field.ty, &clone_generics)
                                    {
                                        push_where_predicate_once(
                                            &mut clone_generics,
                                            syn::parse_quote!(
                                                #parameter: ::core::clone::Clone
                                            ),
                                        );
                                    }
                                } else {
                                    let field_type = &field.ty;
                                    push_where_predicate_once(
                                        &mut clone_generics,
                                        syn::parse_quote!(#field_type: ::core::clone::Clone),
                                    );
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
                    }
                }
                fields => {
                    reject_model_roles(
                        fields,
                        "`model!(...)` fields require a component struct with named fields",
                    )?;
                    None
                }
            };
            (item.ident.clone(), item.generics.clone(), clone_impl)
        }
        Item::Enum(item) => {
            for variant in &item.variants {
                reject_model_roles(
                    &variant.fields,
                    "`model!(...)` fields are not supported in component enums; use a named-field struct",
                )?;
            }
            (item.ident.clone(), item.generics.clone(), None)
        }
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "#[component] requires a struct or enum",
            ));
        }
    };
    Ok(component_output(item, &name, &generics, clone_impl))
}

fn component_output(
    item: impl quote::ToTokens,
    name: &syn::Ident,
    generics: &syn::Generics,
    clone_impl: Option<TokenStream>,
) -> TokenStream {
    let marker = marker_impl(name, generics);
    quote! {
        #item
        #marker
        #clone_impl
    }
}

fn reject_attribute_arguments(attr: TokenStream) -> syn::Result<()> {
    if attr.is_empty() {
        return Ok(());
    }
    Err(syn::Error::new_spanned(
        attr,
        "`#[component]` does not accept arguments; declare Model fields with `model!(T)`",
    ))
}

fn lower_model_field(field: &mut Field, generics: &mut syn::Generics) -> syn::Result<bool> {
    let Some((role, _)) = COMPONENT_FIELD_ROLES.classify(&field.ty, "#[component]")? else {
        return Ok(false);
    };
    debug_assert_eq!(role, RoleKind::Model);
    let name = field.ident.clone().expect("named component field");
    let Some(binding) = BindingSpec::parse_model_type_role(name, &field.ty)? else {
        return Ok(false);
    };
    let BindingSource::Model(ModelSource::Registered { ty: declared }) = binding.source else {
        return Err(syn::Error::new(
            binding.syntax.span(),
            "component Model fields must use `model!(T)` or `model!(optional T)`",
        ));
    };

    if field.attrs.iter().any(|attr| may_remove_field(&attr.meta)) {
        // Stable Rust cannot attach `cfg` to one where predicate. Existing
        // generic parameters are always in scope, so constraining only those
        // closes the enabled projection without leaking a cfg-only concrete
        // type into the disabled configuration.
        for parameter in referenced_type_parameters(&declared, generics) {
            push_where_predicate_once(
                generics,
                syn::parse_quote!(#parameter: ::mirui::core::model::BindType),
            );
        }
    } else {
        push_where_predicate_once(
            generics,
            syn::parse_quote!(#declared: ::mirui::core::model::BindType),
        );
    }

    field.ty = match binding.cardinality {
        BindingCardinality::Required => syn::parse_quote!(
            <#declared as ::mirui::core::model::BindType>::Shared
        ),
        BindingCardinality::Optional => syn::parse_quote!(
            ::core::option::Option<<#declared as ::mirui::core::model::BindType>::Shared>
        ),
        BindingCardinality::Many => unreachable!("Model fields are never plural"),
    };
    Ok(true)
}

fn reject_model_roles(fields: &Fields, message: &str) -> syn::Result<()> {
    for field in fields {
        let Some((role, _)) = COMPONENT_FIELD_ROLES.classify(&field.ty, "#[component]")? else {
            continue;
        };
        debug_assert_eq!(role, RoleKind::Model);
        let name = field
            .ident
            .clone()
            .unwrap_or_else(|| syn::Ident::new("__model", field.ty.span()));
        if let Some(binding) = BindingSpec::parse_model_type_role(name, &field.ty)? {
            return Err(syn::Error::new(binding.syntax.span(), message));
        }
    }
    Ok(())
}

fn referenced_type_parameters(ty: &Type, generics: &Generics) -> Vec<syn::Ident> {
    struct TypeParameters<'a> {
        candidates: &'a [syn::Ident],
        found: Vec<syn::Ident>,
    }

    impl<'ast> Visit<'ast> for TypeParameters<'_> {
        fn visit_type_path(&mut self, path: &'ast syn::TypePath) {
            if path.qself.is_none()
                && path.path.segments.len() == 1
                && let Some(segment) = path.path.segments.first()
                && let Some(parameter) = self
                    .candidates
                    .iter()
                    .find(|candidate| **candidate == segment.ident)
                && !self.found.contains(parameter)
            {
                self.found.push(parameter.clone());
            }
            syn::visit::visit_type_path(self, path);
        }
    }

    let candidates: Vec<_> = generics
        .type_params()
        .map(|parameter| parameter.ident.clone())
        .collect();
    let mut visitor = TypeParameters {
        candidates: &candidates,
        found: Vec::new(),
    };
    visitor.visit_type(ty);
    visitor.found
}

fn push_where_predicate_once(generics: &mut Generics, predicate: syn::WherePredicate) {
    let rendered = predicate.to_token_stream().to_string();
    if generics.where_clause.as_ref().is_some_and(|clause| {
        clause
            .predicates
            .iter()
            .any(|existing| existing.to_token_stream().to_string() == rendered)
    }) {
        return;
    }
    generics.make_where_clause().predicates.push(predicate);
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
    fn rejects_attribute_arguments() {
        let error = expand(
            quote!(unexpected),
            quote!(
                struct Board {
                    model: model!(Model),
                }
            ),
        )
        .unwrap_err();
        assert!(error.to_string().contains("does not accept arguments"));
    }

    #[test]
    fn rejects_unsupported_component_shapes() {
        assert!(expand(quote!(), quote!(union Bits { value: u8 })).is_err());
        assert!(
            expand(
                quote!(),
                quote!(
                    struct PlainTuple(u8);
                )
            )
            .is_ok()
        );
        assert!(
            expand(
                quote!(),
                quote!(
                    enum PlainEnum {
                        Value(u8),
                    }
                )
            )
            .is_ok()
        );

        let tuple = expand(
            quote!(),
            quote!(
                struct Board(model!(Model));
            ),
        )
        .unwrap_err();
        assert!(tuple.to_string().contains("named fields"));
        let enumeration = expand(
            quote!(),
            quote!(
                enum Board {
                    Value { model: model!(Model) },
                }
            ),
        )
        .unwrap_err();
        assert!(enumeration.to_string().contains("component enums"));
    }

    #[test]
    fn lowers_required_and_optional_model_fields() {
        let output = expand(
            quote!(),
            quote! {
                struct Board<T> {
                    game: model!(T),
                    selection: model!(optional Selection),
                    label: u8,
                }
            },
        )
        .unwrap()
        .to_string();

        assert!(output.contains("game : < T as :: mirui :: core :: model :: BindType > :: Shared"));
        assert!(output.contains(
            "selection : :: core :: option :: Option < < Selection as :: mirui :: core :: model :: BindType > :: Shared >"
        ));
        assert!(output.contains("label : u8"));
        assert!(output.contains("T : :: mirui :: core :: model :: BindType"));
        assert!(output.contains("Selection : :: mirui :: core :: model :: BindType"));
    }

    #[test]
    fn rejects_projection_and_malformed_model_roles() {
        let projection = expand(
            quote!(),
            quote!(
                struct Board {
                    game: model!(component.game),
                }
            ),
        )
        .unwrap_err();
        assert!(projection.to_string().contains("must use `model!(T)`"));

        let empty = expand(
            quote!(),
            quote!(
                struct Board {
                    game: model!(),
                }
            ),
        )
        .unwrap_err();
        assert!(empty.to_string().contains("expects a model type"));
    }

    #[test]
    fn custom_clone_accepts_qualified_derive_and_preserves_conditional_fields() {
        let output = expand(
            quote!(),
            quote! {
                #[derive(::core::clone::Clone)]
                struct Board {
                    #[cfg(any())]
                    model: model!(Model),
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
            quote!(),
            quote! {
                #[derive(Clone)]
                struct Board {
                    #[cfg_attr(feature = "extra", cfg(feature = "other"))]
                    model: model!(ConditionalModel),
                    #[cfg_attr(feature = "extra", doc = "note")]
                    stable: model!(StableModel),
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
        assert!(output.contains("StableModel : :: mirui :: core :: model :: BindType"));
        assert!(output.contains("StableModel as :: mirui :: core :: model :: BindType"));
    }

    #[test]
    fn conditional_generic_fields_receive_safe_unconditional_bounds() {
        let output = expand(
            quote!(),
            quote! {
                #[derive(Clone)]
                struct Board<T, U> {
                    stable: model!(StableModel),
                    #[cfg(any())]
                    model: model!(T),
                    #[cfg_attr(feature = "extra", cfg(feature = "model"))]
                    value: U,
                    #[cfg(any())]
                    unavailable: UnavailableType,
                }
            },
        )
        .unwrap()
        .to_string();

        assert!(output.contains("T : :: mirui :: core :: model :: BindType"));
        assert!(output.contains("U : :: core :: clone :: Clone"));
        assert!(!output.contains("UnavailableType : :: core :: clone :: Clone"));
    }

    #[test]
    fn rejects_binding_roles_outside_the_component_model_field_grammar() {
        for role in [
            quote!(res!(Theme)),
            quote!(query!(Style)),
            quote!(commands!()),
            quote!(component!(Board)),
            quote!(entity!()),
            quote!(paint!()),
            quote!(motion!()),
        ] {
            let error = expand(
                quote!(),
                quote! {
                    struct Board {
                        value: #role,
                    }
                },
            )
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("is not available in `#[component]`")
            );
        }
    }
}
