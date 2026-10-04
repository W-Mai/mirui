use crate::function_attrs::{FunctionAttributes, classify_function_attributes};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Ident, ItemFn, LitStr, ReturnType, Token};

use crate::binding::{BindingAccess, BindingCapability, BindingSource, BindingSpec, ModelSource};

struct AnimateArgs {
    component: Ident,
}

impl Parse for AnimateArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut component = None;
        while !input.is_empty() {
            let key = input.parse::<Ident>()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "component" => {
                    if component.replace(input.parse()?).is_some() {
                        return Err(syn::Error::new(key.span(), "duplicate `component`"));
                    }
                }
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "unknown `#[animate]` argument; expected `component`",
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        let component = component.ok_or_else(|| {
            input.error("`#[animate]` requires `component = MotionComponentName`")
        })?;
        Ok(Self { component })
    }
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let args = syn::parse2::<AnimateArgs>(attr)?;
    let function = syn::parse2::<ItemFn>(item)?;
    validate_function(&function)?;
    let bindings = BindingSpec::parse_animate_parameters(&function.sig.inputs)?;
    reject_motion_component_alias(&args.component, &bindings)?;
    lower(function, args, bindings)
}

fn validate_function(function: &ItemFn) -> syn::Result<()> {
    let signature = &function.sig;
    if let Some(token) = signature.constness {
        return Err(syn::Error::new(
            token.span(),
            "animations cannot be `const`",
        ));
    }
    if let Some(token) = signature.asyncness {
        return Err(syn::Error::new(token.span(), "animations cannot be async"));
    }
    if let Some(token) = signature.unsafety {
        return Err(syn::Error::new(token.span(), "animations cannot be unsafe"));
    }
    if let Some(abi) = &signature.abi {
        return Err(syn::Error::new_spanned(
            abi,
            "animations cannot declare an ABI",
        ));
    }
    if !signature.generics.params.is_empty() || signature.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &signature.generics,
            "animations cannot be generic",
        ));
    }
    if let Some(variadic) = &signature.variadic {
        return Err(syn::Error::new_spanned(
            variadic,
            "animations cannot be variadic",
        ));
    }
    if !matches!(signature.output, ReturnType::Default) {
        return Err(syn::Error::new_spanned(
            &signature.output,
            "animations return `()` implicitly",
        ));
    }
    Ok(())
}

fn reject_motion_component_alias(
    motion_component: &Ident,
    bindings: &[BindingSpec],
) -> syn::Result<()> {
    for binding in bindings {
        let BindingSource::Component(component) = &binding.source else {
            continue;
        };
        let ty = &component.ty;
        if quote!(#motion_component).to_string() == quote!(#ty).to_string() {
            return Err(syn::Error::new(
                binding.syntax.span(),
                "the generated motion component cannot also be requested through `component!(...)`",
            ));
        }
    }
    Ok(())
}

fn lower(
    function: ItemFn,
    args: AnimateArgs,
    bindings: Vec<BindingSpec>,
) -> syn::Result<TokenStream> {
    let function_name = function.sig.ident;
    let visibility = function.vis;
    let attributes = classify_function_attributes(function.attrs, "animations")?;
    let body = function.block;
    let motion_component = args.component;
    let system_name = LitStr::new(&motion_component.to_string(), motion_component.span());
    let helper_name = format_ident!(
        "__mirui_animate_body_{function_name}",
        span = Span::mixed_site()
    );
    let system_function = format_ident!(
        "__mirui_animate_system_{function_name}",
        span = Span::mixed_site()
    );
    let delta = internal_ident("delta");
    let delta_ms = internal_ident("delta_ms");
    let rows = internal_ident("rows");
    let entity = internal_ident("entity");
    let motion = internal_ident("motion");
    let frame = internal_ident("frame");
    let commands = internal_ident("commands");

    let helper_parameters = bindings
        .iter()
        .map(lower_helper_parameter)
        .collect::<syn::Result<Vec<_>>>()?;
    let helper_arguments = bindings
        .iter()
        .map(|binding| lower_helper_argument(binding, &entity, &frame, &commands))
        .collect::<syn::Result<Vec<_>>>()?;

    let models = bindings
        .iter()
        .filter_map(|binding| match &binding.source {
            BindingSource::Model(ModelSource::Registered { ty }) => {
                Some((binding.name.clone(), ty.clone()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let components = bindings
        .iter()
        .filter_map(|binding| match &binding.source {
            BindingSource::Component(component) => {
                Some((binding.name.clone(), component.ty.clone(), binding.access))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let model_parameters = models.iter().map(|(name, ty)| quote!(#name: model!(#ty)));
    let registration_inputs = models
        .iter()
        .map(|(name, ty)| quote!(#name: <#ty as ::mirui::core::model::BindType>::Shared));
    let registration_arguments = models.iter().map(|(name, _)| name);
    let component_query_items = components.iter().map(|(_, ty, access)| match access {
        BindingAccess::Read => quote!(#ty),
        BindingAccess::Write => quote!(mut #ty),
        BindingAccess::Command => unreachable!("components cannot use command access"),
    });
    let component_names = components.iter().map(|(name, _, _)| name);

    let system_constructor = if models.is_empty() {
        quote! {
            #visibility const fn system() -> ::mirui::ecs::System {
                #system_function::system()
            }
        }
    } else {
        quote! {
            #visibility fn system(#(#registration_inputs),*) -> ::mirui::ecs::System {
                #system_function::system(#(#registration_arguments),*)
            }
        }
    };

    let FunctionAttributes {
        helper,
        declaration,
        conditional,
    } = attributes;
    Ok(quote! {
        #(#conditional)*
        #(#declaration)*
        #visibility struct #motion_component(#visibility ::mirui::anim::Motion);

        #(#conditional)*
        impl ::mirui::ecs::Component for #motion_component {}

        #(#conditional)*
        impl ::mirui::__macro_support::MotionComponent for #motion_component {
            fn motion(&self) -> &::mirui::anim::Motion {
                &self.0
            }

            fn motion_mut(&mut self) -> &mut ::mirui::anim::Motion {
                &mut self.0
            }
        }

        #(#helper)*
        #[doc(hidden)]
        fn #helper_name(#(#helper_parameters),*) {
            #body
        }

        #(#conditional)*
        #[::mirui::system(name = #system_name, order = Animation, expect = #motion_component)]
        fn #system_function(
            #(#model_parameters,)*
            #delta: res!(optional ::mirui::ecs::DeltaTimeMs),
            #rows: query!(
                Entity,
                mut #motion_component,
                #(#component_query_items,)*
            ),
            #commands: commands!(),
        ) {
            let #delta_ms = #delta.map_or(16, |value| value.0);
            for (#entity, #motion, #(#component_names,)*) in #rows {
                let #frame = ::mirui::__macro_support::advance_motion(#motion, #delta_ms);
                #helper_name(#(#helper_arguments),*);
                if #frame.done {
                    #commands.remove::<#motion_component>(#entity);
                }
            }
        }

        #(#conditional)*
        impl #motion_component {
            #system_constructor
        }
    })
}

fn lower_helper_parameter(binding: &BindingSpec) -> syn::Result<TokenStream> {
    let name = &binding.name;
    match &binding.source {
        BindingSource::Model(ModelSource::Registered { ty }) => {
            Ok(quote!(#name: &<#ty as ::mirui::core::model::BindType>::Shared))
        }
        BindingSource::Component(component) => {
            let ty = &component.ty;
            match binding.access {
                BindingAccess::Read => Ok(quote!(#name: &#ty)),
                BindingAccess::Write => Ok(quote!(#name: &mut #ty)),
                BindingAccess::Command => Err(syn::Error::new(
                    binding.syntax.span(),
                    "component bindings cannot use command access",
                )),
            }
        }
        BindingSource::Capability(BindingCapability::Entity) => {
            Ok(quote!(#name: ::mirui::ecs::Entity))
        }
        BindingSource::Capability(BindingCapability::Motion) => {
            Ok(quote!(#name: ::mirui::types::Fixed))
        }
        BindingSource::Capability(BindingCapability::Commands) => Ok(quote!(
            #name: &mut ::mirui::__macro_support::DeferredCommands<'_>
        )),
        _ => Err(syn::Error::new(
            binding.syntax.span(),
            "binding role is not available in `#[animate]`",
        )),
    }
}

fn lower_helper_argument(
    binding: &BindingSpec,
    entity: &Ident,
    frame: &Ident,
    commands: &Ident,
) -> syn::Result<TokenStream> {
    match &binding.source {
        BindingSource::Model(ModelSource::Registered { .. }) | BindingSource::Component(_) => {
            let name = &binding.name;
            Ok(quote!(#name))
        }
        BindingSource::Capability(BindingCapability::Entity) => Ok(quote!(#entity)),
        BindingSource::Capability(BindingCapability::Motion) => Ok(quote!(#frame.value)),
        BindingSource::Capability(BindingCapability::Commands) => Ok(quote!(&mut #commands)),
        _ => Err(syn::Error::new(
            binding.syntax.span(),
            "binding role is not available in `#[animate]`",
        )),
    }
}

fn internal_ident(name: &str) -> Ident {
    format_ident!("__mirui_{name}", span = Span::mixed_site())
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn expand_ok(attr: TokenStream, item: TokenStream) -> String {
        expand(attr, item)
            .unwrap_or_else(|error| panic!("unexpected expansion error: {error}"))
            .to_string()
    }

    #[test]
    fn lowers_typed_capabilities_and_motion_system() {
        let output = expand_ok(
            quote!(component = AnimateThumbX),
            quote! {
                pub(crate) fn apply_thumb(
                    game: model!(Game),
                    entity: entity!(),
                    value: motion!(),
                    transform: component!(mut Transform),
                    marker: component!(Marker),
                    commands: commands!(),
                ) {
                    transform.x = value;
                    commands.remove::<Marker>(entity);
                    game.observe();
                }
            },
        );
        assert!(output.contains("struct AnimateThumbX"));
        assert!(output.contains("impl :: mirui :: ecs :: Component for AnimateThumbX"));
        assert!(
            output
                .contains("impl :: mirui :: __macro_support :: MotionComponent for AnimateThumbX")
        );
        assert!(output.contains("mut AnimateThumbX"));
        assert!(output.contains("mut Transform"));
        assert!(output.contains("advance_motion"));
        assert!(output.contains("remove :: < AnimateThumbX >"));
        assert!(output.contains("BindType > :: Shared"));
        assert!(output.contains("fn system"));
        assert!(!output.contains("& mut World"));
    }

    #[test]
    fn no_model_animation_exposes_const_system_constructor() {
        let output = expand_ok(
            quote!(component = Pulse),
            quote!(
                fn apply(value: motion!()) {
                    let _ = value;
                }
            ),
        );
        assert!(output.contains("const fn system"));
    }

    #[test]
    fn rejects_missing_component_name_and_raw_parameters() {
        let missing = expand(
            TokenStream::new(),
            quote!(
                fn apply(value: motion!()) {}
            ),
        )
        .unwrap_err()
        .to_string();
        assert!(missing.contains("requires `component = MotionComponentName`"));

        let raw = expand(
            quote!(component = Pulse),
            quote!(
                fn apply(world: &mut World) {}
            ),
        )
        .unwrap_err()
        .to_string();
        assert!(raw.contains("animate parameters must use"));
    }

    #[test]
    fn rejects_binding_the_generated_motion_component() {
        let error = expand(
            quote!(component = Pulse),
            quote!(
                fn apply(pulse: component!(mut Pulse)) {}
            ),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("generated motion component"));
    }
}
