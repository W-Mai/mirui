use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Attribute, FnArg, ImplItem, Item, Pat, Type, parse_quote, spanned::Spanned};

struct ObservedValue {
    name: syn::Ident,
    ty: Type,
    visibility: syn::Visibility,
    attrs: Vec<Attribute>,
    cfg_attrs: Vec<Attribute>,
}

struct EffectExtractor {
    name: syn::Ident,
    event: Type,
    array: Type,
    cfg_attrs: Vec<Attribute>,
}

fn cfg_attrs(attrs: &[Attribute]) -> Vec<Attribute> {
    fn presence_meta(meta: &syn::Meta) -> Option<syn::Meta> {
        if meta.path().is_ident("cfg") {
            return Some(meta.clone());
        }
        if !meta.path().is_ident("cfg_attr") {
            return None;
        }
        let syn::Meta::List(list) = meta else {
            return Some(meta.clone());
        };
        let Ok(nested) =
            list.parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        else {
            return Some(meta.clone());
        };
        let mut nested = nested.into_iter();
        let Some(condition) = nested.next() else {
            return Some(meta.clone());
        };
        let presence: Vec<_> = nested.filter_map(|meta| presence_meta(&meta)).collect();
        if presence.is_empty() {
            None
        } else {
            Some(parse_quote!(cfg_attr(#condition, #(#presence),*)))
        }
    }

    attrs
        .iter()
        .filter_map(|attr| presence_meta(&attr.meta))
        .map(|meta| parse_quote!(#[#meta]))
        .collect()
}

fn may_be_cfg_removed(attrs: &[Attribute]) -> bool {
    fn meta_removes_item(meta: &syn::Meta) -> bool {
        if meta.path().is_ident("cfg") {
            return true;
        }
        if !meta.path().is_ident("cfg_attr") {
            return false;
        }
        let syn::Meta::List(list) = meta else {
            return true;
        };
        list.parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
            .map(|nested| nested.iter().skip(1).any(meta_removes_item))
            .unwrap_or(true)
    }

    attrs.iter().any(|attr| meta_removes_item(&attr.meta))
}

fn accessor_attrs(attrs: &[Attribute]) -> Vec<Attribute> {
    attrs
        .iter()
        .filter(|attr| {
            attr.path().is_ident("cfg")
                || attr.path().is_ident("cfg_attr")
                || attr.path().is_ident("doc")
                || attr.path().is_ident("deprecated")
        })
        .cloned()
        .collect()
}

fn returns_reference(ty: &Type) -> bool {
    match ty {
        Type::Reference(_) => true,
        Type::Array(ty) => returns_reference(&ty.elem),
        Type::Group(ty) => returns_reference(&ty.elem),
        Type::Paren(ty) => returns_reference(&ty.elem),
        Type::Slice(ty) => returns_reference(&ty.elem),
        Type::Tuple(ty) => ty.elems.iter().any(returns_reference),
        Type::Path(ty) => ty
            .path
            .segments
            .iter()
            .any(|segment| match &segment.arguments {
                syn::PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| match arg {
                    syn::GenericArgument::Type(ty) => returns_reference(ty),
                    _ => false,
                }),
                _ => false,
            }),
        _ => false,
    }
}

struct ModelOptions {
    change: Option<syn::Type>,
    watches: Vec<(syn::Ident, syn::Expr)>,
}

fn parse_options(attr: TokenStream) -> syn::Result<ModelOptions> {
    let mut options = ModelOptions {
        change: None,
        watches: Vec::new(),
    };
    let metas = Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated.parse2(attr)?;
    for meta in metas {
        match meta {
            syn::Meta::NameValue(value) if value.path.is_ident("change") => {
                if options.change.is_some() {
                    return Err(syn::Error::new_spanned(value, "duplicate change type"));
                }
                let syn::Expr::Path(path) = value.value else {
                    return Err(syn::Error::new_spanned(
                        value.value,
                        "change expects a type path",
                    ));
                };
                options.change = Some(parse_quote!(#path));
            }
            syn::Meta::List(list) if list.path.is_ident("watch") => {
                let entries = list
                    .parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)?;
                for entry in entries {
                    let syn::Meta::NameValue(value) = entry else {
                        return Err(syn::Error::new_spanned(
                            entry,
                            "watch expects name = mask entries",
                        ));
                    };
                    let Some(name) = value.path.get_ident() else {
                        return Err(syn::Error::new_spanned(
                            value.path,
                            "watch name must be an identifier",
                        ));
                    };
                    if options.watches.iter().any(|(existing, _)| existing == name) {
                        return Err(syn::Error::new_spanned(name, "duplicate watch name"));
                    }
                    options.watches.push((name.clone(), value.value));
                }
            }
            other => return Err(syn::Error::new_spanned(other, "expected change or watch")),
        }
    }
    if options.change.is_none() && !options.watches.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "watch requires change = Type",
        ));
    }
    Ok(options)
}

fn effect_item_type(output: &syn::ReturnType) -> syn::Result<syn::Type> {
    let syn::ReturnType::Type(_, result) = output else {
        return Err(syn::Error::new_spanned(
            output,
            "#[effects] requires [Option<Event>; N]",
        ));
    };
    let syn::Type::Array(array) = &**result else {
        return Err(syn::Error::new_spanned(
            result,
            "#[effects] requires [Option<Event>; N]",
        ));
    };
    let syn::Type::Path(option) = &*array.elem else {
        return Err(syn::Error::new_spanned(
            &array.elem,
            "#[effects] requires Option<Event> elements",
        ));
    };
    let Some(segment) = option.path.segments.last() else {
        return Err(syn::Error::new_spanned(option, "expected Option<Event>"));
    };
    if segment.ident != "Option" {
        return Err(syn::Error::new_spanned(option, "expected Option<Event>"));
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return Err(syn::Error::new_spanned(option, "expected Option<Event>"));
    };
    let Some(syn::GenericArgument::Type(event)) = args.args.first() else {
        return Err(syn::Error::new_spanned(option, "expected Option<Event>"));
    };
    if args.args.len() != 1 {
        return Err(syn::Error::new_spanned(option, "expected Option<Event>"));
    }
    Ok(event.clone())
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    match syn::parse2::<Item>(item)? {
        Item::Struct(item) => expand_struct(item, parse_options(attr)?),
        Item::Impl(item) if attr.is_empty() => expand_impl(item),
        Item::Impl(_) => Err(syn::Error::new_spanned(attr, "model impl takes no options")),
        other => Err(syn::Error::new_spanned(
            other,
            "#[model] requires a struct or inherent impl",
        )),
    }
}

fn expand_struct(mut item: syn::ItemStruct, options: ModelOptions) -> syn::Result<TokenStream> {
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
        observed.push(ObservedValue {
            name: field_name.clone(),
            ty: field.ty.clone(),
            visibility: field.vis.clone(),
            attrs: accessor_attrs(&field.attrs),
            cfg_attrs: cfg_attrs(&field.attrs),
        });
    }
    let observed_names: Vec<_> = observed.iter().map(|value| &value.name).collect();
    for (name, _) in &options.watches {
        let getter = format_ident!("{}_revision", name);
        if observed_names.contains(&&getter) {
            return Err(syn::Error::new_spanned(
                name,
                "watch revision getter conflicts with an observed field",
            ));
        }
    }
    let source_count = observed.len();
    let mut model_generics = generics.clone();
    model_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#name #type_generics: 'static));
    model_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#name #type_generics: ::mirui::core::model::ModelMethods));
    for ty in observed
        .iter()
        .filter(|value| !may_be_cfg_removed(&value.cfg_attrs))
        .map(|value| &value.ty)
    {
        model_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: Copy + Eq + 'static));
    }
    let (_, _, model_where) = model_generics.split_for_impl();

    let mut snapshot_generics = generics.clone();
    for ty in observed
        .iter()
        .filter(|value| !may_be_cfg_removed(&value.cfg_attrs))
        .map(|value| &value.ty)
    {
        snapshot_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: Copy + Eq + 'static));
    }
    let (snapshot_impl_generics, _, snapshot_where) = snapshot_generics.split_for_impl();
    let snapshot_definition = if observed.is_empty() {
        quote!()
    } else {
        let fields = observed.iter().map(|value| {
            let name = &value.name;
            let ty = &value.ty;
            let attrs = &value.cfg_attrs;
            quote!(#(#attrs)* #name: #ty,)
        });
        quote! {
            #visibility struct #snapshot #snapshot_generics #snapshot_where {
                #(#fields)*
                __mirui_marker: ::core::marker::PhantomData<#name #type_generics>,
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
        let fields = observed.iter().map(|value| {
            let name = &value.name;
            let attrs = &value.cfg_attrs;
            quote!(#(#attrs)* #name: self.#name,)
        });
        quote!(#snapshot {
            #(#fields)*
            __mirui_marker: ::core::marker::PhantomData,
        })
    };
    let publish = observed.iter().enumerate().map(|(index, value)| {
        let index = syn::Index::from(index);
        let name = &value.name;
        let attrs = &value.cfg_attrs;
        quote! {
            #(#attrs)*
            if before.#name != after.#name {
                sources[#index].notify();
            }
        }
    });
    let publish_body = if observed.is_empty() {
        quote!(let _ = (sources, before, after);)
    } else {
        quote!(#(#publish)*)
    };
    let accessors = observed.iter().enumerate().map(|(index, value)| {
        let field = &value.name;
        let ty = &value.ty;
        let vis = &value.visibility;
        let attrs = &value.attrs;
        quote! {
            #(#attrs)*
            #vis fn #field(&self) -> #ty {
                ::mirui::core::model::ModelHandle::read_observed(self, #index, |data| data.#field)
            }
        }
    });
    let subscriptions = observed.iter().enumerate().map(|(index, value)| {
        let subscribe = format_ident!("__mirui_subscribe_{}", value.name);
        let attrs = &value.cfg_attrs;
        quote! {
            #(#attrs)*
            #[doc(hidden)]
            pub fn #subscribe(
                &self,
                world: &::mirui::ecs::World,
                entity: ::mirui::ecs::Entity,
            ) -> ::mirui::core::model::ModelSubscription {
                ::mirui::core::model::ModelHandle::subscribe_observed(self, #index, world, entity)
            }
        }
    });
    let change_type = options
        .change
        .map_or_else(|| quote!(()), |change| quote!(#change));
    let watch_count = options.watches.len();
    let watch_getters = options
        .watches
        .iter()
        .enumerate()
        .map(|(index, (name, _))| {
            let getter = format_ident!("{}_revision", name);
            quote! {
                #visibility fn #getter(&self) -> u64 {
                    ::mirui::core::model::ModelHandle::watch_revision(self, #index)
                }
            }
        });
    let watch_subscriptions = options
        .watches
        .iter()
        .enumerate()
        .map(|(index, (name, _))| {
            let subscribe = format_ident!("__mirui_subscribe_{}_revision", name);
            quote! {
                #[doc(hidden)]
                pub fn #subscribe(
                    &self,
                    world: &::mirui::ecs::World,
                    entity: ::mirui::ecs::Entity,
                ) -> ::mirui::core::model::ModelSubscription {
                    ::mirui::core::model::ModelHandle::subscribe_watch(self, #index, world, entity)
                }
            }
        });
    let watch_publish = options
        .watches
        .iter()
        .enumerate()
        .map(|(index, (_, mask))| {
            quote! {
                if change.contains(#mask) {
                    watches[#index].publish();
                }
            }
        });
    let watch_publish_body = if options.watches.is_empty() {
        quote!(let _ = (watches, change);)
    } else {
        quote!(#(#watch_publish)*)
    };

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
            type Change = #change_type;
            type Watches = [::mirui::core::model::ModelWatch; #watch_count];

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

            fn watches() -> Self::Watches {
                ::core::array::from_fn(|_| ::mirui::core::model::ModelWatch::new())
            }

            fn publish_change(watches: &Self::Watches, change: Self::Change) {
                #watch_publish_body
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
            #(#watch_getters)*
            #(#subscriptions)*
            #(#watch_subscriptions)*
        }
    })
}

fn expand_impl(mut item: syn::ItemImpl) -> syn::Result<TokenStream> {
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
    if self_type.qself.is_some() {
        return Err(syn::Error::new_spanned(
            &item.self_ty,
            "model impl target must use a named type path",
        ));
    }
    let Some(target) = self_type.path.segments.last() else {
        return Err(syn::Error::new_spanned(
            &item.self_ty,
            "model impl target must use a named type path",
        ));
    };
    let name = target.ident.clone();
    let handle = format_ident!("{}Handle", name);
    let mut handle_path = self_type.path.clone();
    handle_path.segments.last_mut().unwrap().ident = handle.clone();
    let snapshot = format_ident!("{}DerivedSnapshot", name);
    let generics = item.generics.clone();
    let (_, _, impl_where) = generics.split_for_impl();
    let type_args = target.arguments.clone();
    let mut forwards = Vec::new();
    let mut observed = Vec::new();
    let mut effects = Vec::new();
    for member in &mut item.items {
        let ImplItem::Fn(method) = member else {
            continue;
        };
        let markers: Vec<_> = method
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("observe"))
            .collect();
        let is_observed = !markers.is_empty();
        let effect_markers: Vec<_> = method
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("effects"))
            .collect();
        let is_effect = !effect_markers.is_empty();
        if is_observed && is_effect {
            return Err(syn::Error::new_spanned(
                method,
                "a model method cannot be both observed and an effect extractor",
            ));
        }
        if effect_markers.len() > 1 {
            return Err(syn::Error::new_spanned(
                method,
                "duplicate #[effects] marker",
            ));
        }
        if is_effect && !matches!(effect_markers[0].meta, syn::Meta::Path(_)) {
            return Err(syn::Error::new_spanned(
                effect_markers[0],
                "#[effects] does not take arguments",
            ));
        }
        if markers.len() > 1 {
            return Err(syn::Error::new_spanned(
                method,
                "duplicate #[observe] marker",
            ));
        }
        if is_observed && !matches!(markers[0].meta, syn::Meta::Path(_)) {
            return Err(syn::Error::new_spanned(
                markers[0],
                "#[observe] does not take arguments",
            ));
        }
        method
            .attrs
            .retain(|attr| !attr.path().is_ident("observe") && !attr.path().is_ident("effects"));
        let method_cfg_attrs = cfg_attrs(&method.attrs);
        let Some(FnArg::Receiver(receiver)) = method.sig.inputs.first() else {
            if is_observed || is_effect {
                return Err(syn::Error::new_spanned(
                    &method.sig,
                    "observed methods require &self",
                ));
            }
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
        if let syn::ReturnType::Type(_, ty) = &method.sig.output
            && returns_reference(ty)
        {
            return Err(syn::Error::new_spanned(
                ty,
                "model methods cannot return references; return an owned value",
            ));
        }
        if is_observed {
            if receiver.mutability.is_some()
                || method.sig.inputs.len() != 1
                || !method.sig.generics.params.is_empty()
            {
                return Err(syn::Error::new_spanned(
                    &method.sig,
                    "observed methods require &self, no arguments, and no generics",
                ));
            }
            let syn::ReturnType::Type(_, ty) = &method.sig.output else {
                return Err(syn::Error::new_spanned(
                    &method.sig,
                    "observed methods require a Copy + Eq return value",
                ));
            };
            observed.push(ObservedValue {
                name: method.sig.ident.clone(),
                ty: (**ty).clone(),
                visibility: method.vis.clone(),
                attrs: accessor_attrs(&method.attrs),
                cfg_attrs: method_cfg_attrs,
            });
            continue;
        }
        if is_effect {
            if receiver.mutability.is_none()
                || method.sig.inputs.len() != 1
                || !method.sig.generics.params.is_empty()
            {
                return Err(syn::Error::new_spanned(
                    &method.sig,
                    "effect extractors require &mut self, no arguments, and no generics",
                ));
            }
            let event = effect_item_type(&method.sig.output)?;
            if effects.iter().any(|existing: &EffectExtractor| {
                let existing_event = &existing.event;
                existing.cfg_attrs.len() == method_cfg_attrs.len()
                    && existing
                        .cfg_attrs
                        .iter()
                        .zip(&method_cfg_attrs)
                        .all(|(left, right)| {
                            quote!(#left).to_string() == quote!(#right).to_string()
                        })
                    && quote!(#event).to_string() == quote!(#existing_event).to_string()
            }) {
                return Err(syn::Error::new_spanned(event, "duplicate effect type"));
            }
            let syn::ReturnType::Type(_, array) = &method.sig.output else {
                unreachable!();
            };
            effects.push(EffectExtractor {
                name: method.sig.ident.clone(),
                event,
                array: (**array).clone(),
                cfg_attrs: method_cfg_attrs,
            });
            continue;
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
    let source_count = observed.len();
    let mut derived_generics = generics.clone();
    for ty in observed
        .iter()
        .filter(|value| !may_be_cfg_removed(&value.cfg_attrs))
        .map(|value| &value.ty)
    {
        derived_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: Copy + Eq + 'static));
    }
    let (derived_impl_generics, _, derived_where) = derived_generics.split_for_impl();
    let snapshot_definition = if observed.is_empty() {
        quote!()
    } else {
        let fields = observed.iter().map(|value| {
            let name = &value.name;
            let ty = &value.ty;
            let attrs = &value.cfg_attrs;
            quote!(#(#attrs)* #name: #ty,)
        });
        quote! {
            pub struct #snapshot #derived_generics #derived_where {
                #(#fields)*
                __mirui_marker: ::core::marker::PhantomData<#self_type>,
            }

            impl #derived_impl_generics ::core::marker::Copy for #snapshot #type_args #derived_where {}

            impl #derived_impl_generics ::core::clone::Clone for #snapshot #type_args #derived_where {
                fn clone(&self) -> Self { *self }
            }
        }
    };
    let snapshot_type = if observed.is_empty() {
        quote!(())
    } else {
        quote!(#snapshot #type_args)
    };
    let snapshot_value = if observed.is_empty() {
        quote!()
    } else {
        let fields = observed.iter().map(|value| {
            let name = &value.name;
            let attrs = &value.cfg_attrs;
            quote!(#(#attrs)* #name: self.#name(),)
        });
        quote!(#snapshot {
            #(#fields)*
            __mirui_marker: ::core::marker::PhantomData,
        })
    };
    let publish = observed.iter().enumerate().map(|(index, value)| {
        let index = syn::Index::from(index);
        let name = &value.name;
        let attrs = &value.cfg_attrs;
        quote! {
            #(#attrs)*
            if before.#name != after.#name {
                sources[#index].notify();
            }
        }
    });
    let publish_body = if observed.is_empty() {
        quote!(let _ = (sources, before, after);)
    } else {
        quote!(#(#publish)*)
    };
    let accessors = observed.iter().enumerate().map(|(index, value)| {
        let method = &value.name;
        let ty = &value.ty;
        let vis = &value.visibility;
        let attrs = &value.attrs;
        quote! {
            #(#attrs)*
            #vis fn #method(&self) -> #ty {
                ::mirui::core::model::ModelHandle::read_derived(self, #index, |data| data.#method())
            }
        }
    });
    let subscriptions = observed.iter().enumerate().map(|(index, value)| {
        let subscribe = format_ident!("__mirui_subscribe_{}", value.name);
        let attrs = &value.cfg_attrs;
        quote! {
            #(#attrs)*
            #[doc(hidden)]
            pub fn #subscribe(
                &self,
                world: &::mirui::ecs::World,
                entity: ::mirui::ecs::Entity,
            ) -> ::mirui::core::model::ModelSubscription {
                ::mirui::core::model::ModelHandle::subscribe_derived(self, #index, world, entity)
            }
        }
    });
    let event_batch = format_ident!("{}EffectBatch", name);
    let route_store = format_ident!("{}EffectRoutes", name);
    let effect_storage = if effects.is_empty() {
        quote!()
    } else {
        let event_fields = effects.iter().map(|effect| {
            let name = &effect.name;
            let array = &effect.array;
            let attrs = &effect.cfg_attrs;
            quote!(#(#attrs)* #name: #array,)
        });
        let route_fields = effects.iter().map(|effect| {
            let name = &effect.name;
            let event = &effect.event;
            let attrs = &effect.cfg_attrs;
            quote!(#(#attrs)* #name: ::mirui::core::model::EffectRoute<#event>,)
        });
        quote! {
            pub struct #event_batch #generics #impl_where {
                #(#event_fields)*
                __mirui_marker: ::core::marker::PhantomData<#self_type>,
            }

            pub struct #route_store #generics #impl_where {
                #(#route_fields)*
                __mirui_marker: ::core::marker::PhantomData<#self_type>,
            }
        }
    };
    let (events_type, routes_type, take_body, routes_body, deliver_body) = if effects.is_empty() {
        (
            quote!(()),
            quote!(()),
            quote!(),
            quote!(),
            quote!(let _ = (routes, events);),
        )
    } else {
        let dispatches = effects.iter().map(|effect| {
            let name = &effect.name;
            let attrs = &effect.cfg_attrs;
            quote! {
                #(#attrs)*
                for event in events.#name.into_iter().flatten() {
                    routes.#name.deliver(event);
                }
            }
        });
        let take_fields = effects.iter().map(|effect| {
            let name = &effect.name;
            let attrs = &effect.cfg_attrs;
            quote!(#(#attrs)* #name: self.#name(),)
        });
        let route_values = effects.iter().map(|effect| {
            let name = &effect.name;
            let event = &effect.event;
            let attrs = &effect.cfg_attrs;
            quote!(#(#attrs)* #name: ::mirui::core::model::EffectRoute::<#event>::new(),)
        });
        (
            quote!(#event_batch #type_args),
            quote!(#route_store #type_args),
            quote!(#event_batch {
                #(#take_fields)*
                __mirui_marker: ::core::marker::PhantomData,
            }),
            quote!(#route_store {
                #(#route_values)*
                __mirui_marker: ::core::marker::PhantomData,
            }),
            quote!(#(#dispatches)*),
        )
    };
    let effect_traits = effects.iter().map(|effect| {
        let event = &effect.event;
        let name = &effect.name;
        let attrs = &effect.cfg_attrs;
        quote! {
            #(#attrs)*
            impl #derived_impl_generics ::mirui::core::model::Produces<#event>
                for #self_type #derived_where
            {
                fn route(routes: &Self::Routes) -> &::mirui::core::model::EffectRoute<#event> {
                    &routes.#name
                }
            }
        }
    });
    Ok(quote! {
        #item

        #snapshot_definition
        #effect_storage

        impl #derived_impl_generics ::mirui::core::model::ModelMethods for #self_type #derived_where {
            type DerivedSnapshot = #snapshot_type;
            type DerivedSources = [::mirui::core::reactive::ModelSource; #source_count];
            type Events = #events_type;
            type Routes = #routes_type;

            fn derived_snapshot(&self) -> Self::DerivedSnapshot {
                #snapshot_value
            }

            fn derived_sources() -> Self::DerivedSources {
                ::core::array::from_fn::<_, #source_count, _>(|_| ::mirui::core::reactive::ModelSource::new())
            }

            fn publish_derived(
                sources: &Self::DerivedSources,
                before: Self::DerivedSnapshot,
                after: Self::DerivedSnapshot,
            ) {
                #publish_body
            }

            fn take_events(&mut self) -> Self::Events {
                #take_body
            }

            fn routes() -> Self::Routes {
                #routes_body
            }

            fn deliver_events(routes: &Self::Routes, events: Self::Events) {
                #deliver_body
            }
        }

        #(#effect_traits)*

        impl #derived_impl_generics #handle_path #derived_where {
            #(#forwards)*
        }

        impl #derived_impl_generics #handle_path #derived_where {
            #(#accessors)*
            #(#subscriptions)*
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{cfg_attrs, expand, may_be_cfg_removed};
    use quote::quote;
    use syn::parse_quote;

    #[test]
    fn cfg_attr_without_cfg_keeps_observed_bounds() {
        assert!(!may_be_cfg_removed(&[parse_quote!(
            #[cfg_attr(feature = "extra", doc = "note")]
        )]));
        assert!(may_be_cfg_removed(&[parse_quote!(
            #[cfg_attr(feature = "extra", cfg(feature = "other"))]
        )]));
    }

    #[test]
    fn generated_presence_attrs_drop_expression_docs() {
        let attrs = cfg_attrs(&[parse_quote!(
            #[cfg_attr(feature = "x", cfg(feature = "y"), doc = "note")]
        )]);
        let generated = quote!(#(#attrs)*).to_string();
        assert!(generated.contains("cfg_attr"));
        assert!(generated.contains("feature = \"y\""));
        assert!(!generated.contains("doc"));
    }

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

    #[test]
    fn derived_observation_generates_a_separate_source() {
        let expanded = expand(
            quote!(),
            quote! {
                impl Counter {
                    #[observe]
                    fn doubled(&self) -> u32 { self.count * 2 }
                    fn increment(&mut self) { self.count += 1; }
                }
            },
        )
        .unwrap()
        .to_string();
        assert!(expanded.contains("DerivedSources"));
        assert!(expanded.contains("read_derived"));
        assert!(expanded.contains("fn increment"));
    }

    #[test]
    fn rejects_derived_observation_with_arguments() {
        let error = expand(
            quote!(),
            quote! {
                impl Counter {
                    #[observe]
                    fn plus(&self, extra: u32) -> u32 { self.count + extra }
                }
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("no arguments"));
    }

    #[test]
    fn change_masks_generate_named_revision_getters() {
        let expanded = expand(
            quote!(change = ChangeSet, watch(visual = ChangeSet::VISUAL)),
            quote!(
                struct Board {
                    pixels: u32,
                }
            ),
        )
        .unwrap()
        .to_string();
        assert!(expanded.contains("type Change = ChangeSet"));
        assert!(expanded.contains("fn visual_revision"));
        assert!(expanded.contains("change . contains"));
    }

    #[test]
    fn rejects_watch_without_change_type() {
        let error = expand(
            quote!(watch(visual = ChangeSet::VISUAL)),
            quote!(
                struct Board {
                    pixels: u32,
                }
            ),
        )
        .unwrap_err();
        assert!(error.to_string().contains("requires change"));
    }

    #[test]
    fn rejects_revision_getter_collision_with_observed_field() {
        let error = expand(
            quote!(change = ChangeSet, watch(visual = ChangeSet::VISUAL)),
            quote!(
                struct Board {
                    #[observe]
                    visual_revision: u64,
                }
            ),
        )
        .unwrap_err();
        assert!(error.to_string().contains("conflicts"));
    }

    #[test]
    fn effects_generate_typed_routes_without_forwarding_extractors() {
        let expanded = expand(
            quote!(),
            quote! {
                impl Player {
                    fn play(&mut self) {}
                    #[effects]
                    fn take_notes(&mut self) -> [Option<Note>; 4] { [None; 4] }
                }
            },
        )
        .unwrap()
        .to_string();
        assert!(expanded.contains("Produces < Note >"));
        assert!(expanded.contains("fn play"));
        assert!(!expanded.contains("fn take_notes (& self"));
    }

    #[test]
    fn rejects_duplicate_effect_types() {
        let error = expand(
            quote!(),
            quote! {
                impl Player {
                    #[effects]
                    fn first(&mut self) -> [Option<Note>; 2] { [None; 2] }
                    #[effects]
                    fn second(&mut self) -> [Option<Note>; 4] { [None; 4] }
                }
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("duplicate effect type"));
    }

    #[test]
    fn rejects_unbounded_effect_result() {
        let error = expand(
            quote!(),
            quote! {
                impl Player {
                    #[effects]
                    fn take_notes(&mut self) -> Vec<Note> { Vec::new() }
                }
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("[Option<Event>; N]"));
    }

    #[test]
    fn rejects_duplicate_effect_types_under_the_same_cfg() {
        let error = expand(
            quote!(),
            quote! {
                impl Player {
                    #[cfg(feature = "sound")]
                    #[effects]
                    fn first(&mut self) -> [Option<Note>; 1] { [None] }
                    #[cfg(feature = "sound")]
                    #[effects]
                    fn second(&mut self) -> [Option<Note>; 1] { [None] }
                }
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("duplicate effect type"));
    }

    #[test]
    fn allows_effect_types_under_distinct_cfgs() {
        expand(
            quote!(),
            quote! {
                impl Player {
                    #[cfg(feature = "sound")]
                    #[effects]
                    fn first(&mut self) -> [Option<Note>; 1] { [None] }
                    #[cfg(not(feature = "sound"))]
                    #[effects]
                    fn second(&mut self) -> [Option<Note>; 1] { [None] }
                }
            },
        )
        .unwrap();
    }

    #[test]
    fn rejects_nested_reference_returns() {
        let error = expand(
            quote!(),
            quote! {
                impl Player {
                    fn borrowed(&self) -> Option<&u8> { None }
                }
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("cannot return references"));
    }
}
