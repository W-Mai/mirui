use crate::binding::{
    BindingCardinality, BindingSource, BindingSpec, COMPONENT_FIELD_ROLES, ModelSource, RoleKind,
};
use crate::function_attrs::cfg_attrs;
use proc_macro2::TokenStream;
use quote::{ToTokens, quote, quote_spanned};
use syn::visit::Visit;
use syn::{
    Attribute, Field, Fields, Generics, Item, Meta, Path, Type, punctuated::Punctuated,
    spanned::Spanned,
};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    reject_attribute_arguments(attr)?;
    let mut item = syn::parse2::<Item>(item)?;
    let dependencies = lower_output_dependencies(&mut item)?;
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
    let conditional = match &item {
        Item::Struct(item) => cfg_attrs(&item.attrs),
        Item::Enum(item) => cfg_attrs(&item.attrs),
        _ => unreachable!("component shape was checked"),
    };
    Ok(component_output(
        item,
        &name,
        &generics,
        clone_impl,
        &conditional,
        &dependencies,
    ))
}

fn component_output(
    item: impl quote::ToTokens,
    name: &syn::Ident,
    generics: &syn::Generics,
    clone_impl: Option<TokenStream>,
    conditional: &[Attribute],
    dependencies: &[TokenStream],
) -> TokenStream {
    let marker = marker_impl(name, generics, dependencies);
    let clone_impl = clone_impl.map(|clone_impl| quote!(#(#conditional)* #clone_impl));
    quote! {
        #item
        #(#conditional)*
        #marker
        #clone_impl
    }
}

struct OutputDeclaration {
    ty: Type,
    previous: bool,
    conditions: Vec<Meta>,
}

impl OutputDeclaration {
    fn tokens(&self, conditional: &[Attribute], target: &TokenStream) -> TokenStream {
        let ty = &self.ty;
        let conditions = &self.conditions;
        let condition = (!conditions.is_empty()).then(|| quote!(#[cfg(all(#(#conditions),*))]));
        let dependency = if self.previous {
            quote!(::mirui::ui::outputs::OutputDependency::previous::<#ty>())
        } else {
            quote!(::mirui::ui::outputs::OutputDependency::current::<#ty>(#target))
        };
        quote!(#(#conditional)* #condition #dependency)
    }
}

fn lower_output_dependencies(item: &mut Item) -> syn::Result<Vec<TokenStream>> {
    let (attrs, fields) = match item {
        Item::Struct(item) => (&mut item.attrs, Some(&mut item.fields)),
        Item::Enum(item) => (&mut item.attrs, None),
        _ => return Ok(Vec::new()),
    };
    let owner = quote!(|_, owner| ::core::option::Option::Some(owner));
    let mut dependencies: Vec<_> = take_output_declarations(attrs, true)?
        .iter()
        .map(|declaration| declaration.tokens(&[], &owner))
        .collect();
    if let Some(fields) = fields {
        lower_output_fields(fields, None, &[], &mut dependencies)?;
    } else if let Item::Enum(item) = item {
        for variant in &mut item.variants {
            lower_output_fields(
                &mut variant.fields,
                Some(&variant.ident),
                &cfg_attrs(&variant.attrs),
                &mut dependencies,
            )?;
        }
    }
    Ok(dependencies)
}

fn lower_output_fields(
    fields: &mut Fields,
    variant: Option<&syn::Ident>,
    variant_conditions: &[Attribute],
    dependencies: &mut Vec<TokenStream>,
) -> syn::Result<()> {
    let unnamed = matches!(fields, Fields::Unnamed(_));
    let mut conditional_predecessor = false;
    for (index, field) in fields.iter_mut().enumerate() {
        let declarations = take_output_declarations(&mut field.attrs, false)?;
        let mut conditional = variant_conditions.to_vec();
        conditional.extend(cfg_attrs(&field.attrs));
        if unnamed && conditional_predecessor && !declarations.is_empty() {
            return Err(syn::Error::new_spanned(
                field,
                "`depends_on` on a tuple field cannot follow a field with `cfg` or conditional `cfg`; use a named field",
            ));
        }
        conditional_predecessor |= field.attrs.iter().any(|attr| may_remove_field(&attr.meta));
        if declarations.is_empty() {
            continue;
        }
        let member = field.ident.clone().map_or_else(
            || syn::Member::Unnamed(syn::Index::from(index)),
            syn::Member::Named,
        );
        let source = if let Some(variant) = variant {
            let binding = syn::Ident::new("__mirui_output_source", proc_macro2::Span::mixed_site());
            quote_spanned! {field.ty.span()=>
                match component {
                    Self::#variant { #member: #binding, .. } =>
                        ::mirui::ui::outputs::OutputSource::output_source(#binding),
                    #[allow(unreachable_patterns)]
                    _ => ::core::option::Option::None,
                }
            }
        } else {
            quote_spanned!(field.ty.span()=>
                ::mirui::ui::outputs::OutputSource::output_source(&component.#member)
            )
        };
        let target = quote! {
            |world, owner| {
                let component = world.get::<Self>(owner)?;
                #source
            }
        };
        dependencies.extend(
            declarations
                .iter()
                .map(|declaration| declaration.tokens(&conditional, &target)),
        );
    }
    Ok(())
}

fn take_output_declarations(
    attrs: &mut Vec<Attribute>,
    allow_previous: bool,
) -> syn::Result<Vec<OutputDeclaration>> {
    fn lower_meta(
        meta: Meta,
        conditions: &mut Vec<Meta>,
        allow_previous: bool,
        declarations: &mut Vec<OutputDeclaration>,
    ) -> syn::Result<Option<Meta>> {
        if meta.path().is_ident("depends_on") || meta.path().is_ident("retain_previous") {
            let previous = meta.path().is_ident("retain_previous");
            if previous && !allow_previous {
                return Err(syn::Error::new_spanned(
                    meta,
                    "`retain_previous` describes the component owner and is only allowed on a component type",
                ));
            }
            let Meta::List(list) = &meta else {
                return Err(syn::Error::new_spanned(
                    meta,
                    "output declarations require a non-empty list of output types",
                ));
            };
            let types =
                list.parse_args_with(Punctuated::<Type, syn::Token![,]>::parse_terminated)?;
            if types.is_empty() {
                return Err(syn::Error::new_spanned(
                    meta,
                    "output declarations require a non-empty list of output types",
                ));
            }
            declarations.extend(types.into_iter().map(|ty| OutputDeclaration {
                ty,
                previous,
                conditions: conditions.clone(),
            }));
            return Ok(None);
        }
        if !meta.path().is_ident("cfg_attr") {
            return Ok(Some(meta));
        }
        let Meta::List(list) = &meta else {
            return Ok(Some(meta));
        };
        let nested = list.parse_args_with(Punctuated::<Meta, syn::Token![,]>::parse_terminated)?;
        if nested.len() < 2 {
            return Err(syn::Error::new_spanned(
                meta,
                "`cfg_attr` requires a condition and at least one attribute",
            ));
        }
        let mut nested = nested.into_iter();
        let condition = nested.next().expect("cfg_attr condition");
        conditions.push(condition.clone());
        let mut remaining = Vec::new();
        for meta in nested {
            if let Some(meta) = lower_meta(meta, conditions, allow_previous, declarations)? {
                remaining.push(meta);
            }
        }
        conditions.pop();
        Ok((!remaining.is_empty())
            .then(|| syn::parse_quote!(cfg_attr(#condition, #(#remaining),*))))
    }

    let mut declarations = Vec::new();
    let mut retained = Vec::with_capacity(attrs.len());
    for mut attr in attrs.drain(..) {
        if let Some(meta) = lower_meta(
            attr.meta,
            &mut Vec::new(),
            allow_previous,
            &mut declarations,
        )? {
            attr.meta = meta;
            retained.push(attr);
        }
    }
    *attrs = retained;
    Ok(declarations)
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

fn marker_impl(
    name: &syn::Ident,
    generics: &syn::Generics,
    dependencies: &[TokenStream],
) -> TokenStream {
    let (_, type_generics, _) = generics.split_for_impl();
    let mut component_generics = generics.clone();
    component_generics
        .make_where_clause()
        .predicates
        .push(syn::parse_quote!(#name #type_generics: 'static));
    let (impl_generics, _, where_clause) = component_generics.split_for_impl();
    quote! {
        impl #impl_generics ::mirui::ecs::Component for #name #type_generics #where_clause {
            const OUTPUT_DEPENDENCIES: &'static [::mirui::ui::outputs::OutputDependency] = &[
                #(#dependencies),*
            ];
        }
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

    #[test]
    fn output_declarations_are_static_typed_projections() {
        let output = expand(
            quote!(),
            quote! {
                #[depends_on(Geometry, Pixels)]
                #[retain_previous(Pixels)]
                struct Consumer<T: Output, S: OutputSource> {
                    #[depends_on(T, Pixels)]
                    source: S,
                    unrelated: Entity,
                }
            },
        )
        .unwrap();
        syn::parse2::<syn::File>(output.clone()).unwrap();
        let output = output.to_string();
        assert_eq!(output.matches("OutputDependency :: current").count(), 4);
        assert_eq!(output.matches("OutputDependency :: previous").count(), 1);
        assert_eq!(output.matches("world . get :: < Self >").count(), 2);
        assert!(output.contains("OutputSource :: output_source (& component . source)"));
        assert!(!output.contains("component . unrelated"));
        assert!(!output.contains("depends_on"));
        assert!(!output.contains("retain_previous"));
        assert!(!output.contains("S : :: mirui"));
    }

    #[test]
    fn output_declarations_preserve_presence_and_nested_helper_conditions() {
        let output = expand(
            quote!(),
            quote! {
                #[cfg(all())]
                #[cfg_attr(all(), retain_previous(Pixels), doc = "consumer")]
                struct Consumer {
                    #[cfg(any())]
                    #[depends_on(Unavailable)]
                    removed: UnavailableEntity,
                    #[cfg_attr(all(), cfg_attr(all(), depends_on(Pixels)), doc = "source")]
                    source: Entity,
                    #[cfg_attr(all(), cfg(any()), depends_on(Unavailable))]
                    alternate: UnavailableEntity,
                }
            },
        )
        .unwrap();
        let parsed = syn::parse2::<syn::File>(output.clone()).unwrap();
        let syn::Item::Struct(component) = &parsed.items[0] else {
            panic!("component declaration");
        };
        let syn::Item::Impl(component_impl) = &parsed.items[1] else {
            panic!("component implementation");
        };
        assert_eq!(component_impl.attrs.len(), 1);
        assert!(
            component
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("cfg_attr"))
        );
        let output = output.to_string();
        assert!(!output.contains("depends_on"));
        assert!(!output.contains("retain_previous"));
        assert!(output.contains("cfg (all (all () , all ()))"));
        assert_eq!(output.matches("doc = \"source\"").count(), 1);
        assert_eq!(
            output.matches("cfg_attr (all () , cfg (any ()))").count(),
            2
        );
        assert!(!output.contains("Unavailable :"));
    }

    #[test]
    fn output_fields_cover_tuple_and_enum_components_without_type_spelling_checks() {
        for item in [
            quote!(
                struct Consumer(#[depends_on(Pixels)] EntityAlias);
            ),
            quote! {
                enum Consumer {
                    Named { #[depends_on(Pixels)] source: MaybeEntity },
                    Tuple(#[depends_on(Pixels)] EntityAlias, u8),
                    Empty,
                }
            },
        ] {
            let output = expand(quote!(), item).unwrap();
            syn::parse2::<syn::File>(output.clone()).unwrap();
            assert!(output.to_string().contains("OutputSource :: output_source"));
        }
    }

    #[test]
    fn rejects_output_attributes_with_invalid_placement_or_empty_types() {
        for item in [
            quote!(
                #[depends_on()]
                struct Consumer;
            ),
            quote!(
                #[retain_previous()]
                struct Consumer;
            ),
            quote!(
                struct Consumer {
                    #[retain_previous(Pixels)]
                    source: Entity,
                }
            ),
            quote! {
                struct Consumer {
                    #[cfg_attr(all(), retain_previous(Pixels))]
                    source: Entity,
                }
            },
        ] {
            assert!(expand(quote!(), item).is_err());
        }
    }

    #[test]
    fn rejects_only_tuple_projections_whose_index_depends_on_conditional_fields() {
        let error = expand(
            quote!(),
            quote! {
                struct Consumer(
                    #[cfg_attr(all(), cfg(any()))] u8,
                    #[depends_on(Pixels)] Entity,
                );
            },
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cannot follow a field with `cfg`")
        );

        let output = expand(
            quote!(),
            quote! {
                struct Consumer(
                    #[cfg(all())] #[depends_on(Pixels)] Entity,
                    #[cfg(any())] Unavailable,
                );
            },
        )
        .unwrap();
        syn::parse2::<syn::File>(output).unwrap();
    }
}
