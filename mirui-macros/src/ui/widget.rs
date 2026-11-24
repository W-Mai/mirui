use super::*;
use crate::event_attr;

impl MiruiRune {
    fn emit_on_dispatch(
        widget_name: &str,
        handlers: &[&OnCmd],
        widget_var: &syn::Ident,
        world: &proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let mut tokens = proc_macro2::TokenStream::new();
        for handler in handlers {
            let event = &handler.name;
            let host = match &handler.qualifier {
                Some(qualifier) => quote!(#qualifier),
                None if RESERVED_LAYOUT_NAMES.contains(&widget_name) => quote!(::mirui::ui::Widget),
                None => {
                    let component = syn::Ident::new(widget_name, proc_macro2::Span::call_site());
                    quote!(#component)
                }
            };
            let key = syn::Ident::new("__mirui_event_key", proc_macro2::Span::mixed_site());
            let callback_name = syn::LitStr::new(&format!("on {event}"), handler.name.span());
            let callback =
                match event_attr::lower_inline_callback(&handler.callback, &key, &callback_name) {
                    Ok(callback) => callback,
                    Err(error) => {
                        tokens.extend(error.to_compile_error());
                        continue;
                    }
                };
            let install = quote! {
                let __mirui_callback: ::mirui::__macro_support::EventCallback<
                    _, ::mirui::__macro_support::NodeContext,
                > = #callback;
                ::mirui::__macro_support::install_entity_event(
                    #world, #widget_var, #key, __mirui_callback,
                );
            };
            let selector = if handler.qualifier.is_none()
                && !RESERVED_LAYOUT_NAMES.contains(&widget_name)
                && !matches!(widget_name, "Text" | "TextInput")
            {
                let kind = quote::format_ident!("{widget_var}_kind");
                quote!(::mirui::__macro_support::component_event(#kind, #host::#event))
            } else {
                quote!(#host::#event)
            };
            let options = &handler.options;
            let binding = if options.is_empty() {
                quote! {
                    let #key = #selector;
                    #install
                }
            } else {
                quote! {
                    match (#selector).configure((#(#options,)*)) {
                        ::core::result::Result::Ok(#key) => { #install }
                        ::core::result::Result::Err(error) => {
                            ::mirui::warn!(
                                target: "mirui::event::install",
                                "skipping event binding: {:?}",
                                error,
                            );
                        }
                    }
                }
            };
            tokens.extend(quote! {
                {
                    #[allow(unused_imports)]
                    use ::mirui::input::event::gesture::GestureSelectors as _;
                    #[allow(unused_imports)]
                    use ::mirui::input::event::input::InputSelectors as _;
                    #[allow(unused_imports)]
                    use ::mirui::input::event::layout::LayoutSelectors as _;
                    #[allow(unused_imports)]
                    use ::mirui::ui::selection::SelectionSelectors as _;
                    use ::mirui::ui::clipboard::ClipboardSelectors as _;
                    #binding
                }
            });
        }
        tokens
    }
    pub(super) fn emit_widget(
        cmd: &WidgetCmd,
        world: &proc_macro2::TokenStream,
        parent_var: &proc_macro2::TokenStream,
        reserved: Option<&syn::Ident>,
    ) -> proc_macro2::TokenStream {
        let var = &cmd.var;
        let attrs = &cmd.attrs;
        let layout_fields = &cmd.layout_fields;
        let errors = &cmd.errors;

        let mut tokens = proc_macro2::TokenStream::new();

        for e in errors {
            tokens.extend(e.clone());
        }

        for (ident, key) in &cmd.id_lookups {
            tokens.extend(quote! {
                let #ident = mirui::ecs::World::find_by_id(&*(#world), #key)
                    .expect(concat!("ui!: id '", #key, "' not found in IdMap"));
            });
        }

        let on_handlers: Vec<&OnCmd> = cmd.on_handlers.iter().collect();
        let layout_call = if layout_fields.is_empty() {
            quote! {}
        } else {
            quote! { .layout(mirui::ui::layout::LayoutStyle { #(#layout_fields,)* ..Default::default() }) }
        };

        let builder = match reserved {
            Some(reserved) => quote! { #reserved.build(#world) },
            None => quote! { mirui::__macro_support::WidgetBuilder::new(#world) },
        };
        tokens.extend(quote! {
            let #var = #builder
                #(#attrs)*
                #layout_call
                .id();
        });

        if cmd.name == "Scroll" {
            if !cmd.style_fields.contains("clip_children") {
                tokens.extend(quote! {
                    if let Some(__style) = (#world).get_mut::<mirui::ui::Style>(#var) {
                        __style.clip_children = true;
                    }
                });
            }
            tokens.extend(quote! {
                (#world).insert(#var, mirui::input::event::scroll::ScrollOffset::default());
                (#world).insert(
                    #var,
                    mirui::input::event::scroll::ScrollConfig {
                        elastic: false,
                        ..Default::default()
                    },
                );
            });
        }

        // Must precede the reactive-bind injection: an effect's first run writes
        // into the component, so seeding it after would clobber that value.
        if cmd.kind == WidgetKind::Component {
            let comp_name = &cmd.name;
            if cmd.name == "Text" {
                let init = if let Some(capacity) = &cmd.capacity {
                    let paragraph = cmd
                        .text_paragraph_value
                        .as_ref()
                        .map(|value| quote! { .with_paragraph(#value) });
                    quote! {
                        #comp_name::new_bounded(#capacity)
                            .expect("Text capacity could not be reserved")
                            #paragraph
                    }
                } else {
                    match (&cmd.text_tuple_value, &cmd.text_paragraph_value) {
                        (Some(text_value), Some(paragraph_value)) => quote! {
                            ::core::convert::Into::<#comp_name>::into(#text_value)
                                .with_paragraph(#paragraph_value)
                        },
                        (Some(text_value), None) => {
                            quote! { ::core::convert::Into::<#comp_name>::into(#text_value) }
                        }
                        (None, Some(paragraph_value)) => {
                            quote! { #comp_name::from("").with_paragraph(#paragraph_value) }
                        }
                        (None, None) => quote! { #comp_name::from("") },
                    }
                };
                tokens.extend(quote! {
                    (#world).insert(#var, #init);
                });
            } else if cmd.name == "TextInput" {
                let setters = &cmd.component_fields;
                let capacity = cmd.capacity.as_ref().map(|capacity| quote! {
                    .with_capacity(#capacity).expect("TextEditing capacity could not be reserved")
                });
                let preedit_capacity = cmd.preedit_capacity.as_ref().map(|capacity| quote! {
                    .with_preedit_capacity(#capacity).expect("TextEditing preedit capacity could not be reserved")
                });
                let editing =
                    (cmd.capacity.is_some() || cmd.preedit_capacity.is_some()).then(|| {
                        quote! {
                            (#world).insert(#var,
                                ::mirui::ui::widgets::TextEditing::default()
                                    #capacity #preedit_capacity
                            );
                        }
                    });
                tokens.extend(quote! {
                    (#world).insert(#var, #comp_name::new() #(#setters)*);
                    #editing
                });
            } else {
                let comp_fields = &cmd.component_fields;
                let field_bindings = &cmd.field_bindings;
                let kind = quote::format_ident!("{var}_kind");
                tokens.extend(quote! {
                    let (#kind, #(#field_bindings,)*) = {
                        #[allow(unused_mut, clippy::field_reassign_with_default)]
                        let mut __c = #comp_name::default();
                        #(#comp_fields)*
                        let __kind = ::mirui::__macro_support::component_type(&__c);
                        (#world).insert(#var, __c);
                        (__kind, #(#field_bindings,)*)
                    };
                });
                if cmd.name == "Button" {
                    let paragraph = cmd.text_paragraph_value.clone().unwrap_or_else(
                        || quote! { ::mirui::ui::widgets::ParagraphStyle::label() },
                    );
                    if let Some(capacity) = &cmd.capacity {
                        tokens.extend(quote! {
                            (#world).insert(
                                #var,
                                ::mirui::ui::widgets::Text::new_bounded(#capacity)
                                    .expect("Button capacity could not be reserved")
                                    .with_paragraph(#paragraph),
                            );
                        });
                    } else if let Some(text_value) = &cmd.text_tuple_value {
                        let text = match &cmd.text_paragraph_value {
                            Some(paragraph) => quote! {
                                ::mirui::ui::widgets::Text::new(#text_value)
                                    .with_paragraph(#paragraph)
                            },
                            None => quote! { ::mirui::ui::widgets::Text::label(#text_value) },
                        };
                        tokens.extend(quote! {
                            (#world).insert(#var, #text);
                        });
                    } else if cmd.text_paragraph_value.is_some() {
                        // A reactive label fills this Text in on its first run.
                        tokens.extend(quote! {
                            (#world).insert(
                                #var,
                                ::mirui::ui::widgets::Text::new("").with_paragraph(#paragraph),
                            );
                        });
                    }
                }
            }
        }

        if let Some(capacity) = &cmd.capacity
            && (cmd.name == "Text" || cmd.name == "Button")
            && !cmd
                .reactive_binds
                .iter()
                .any(|binding| binding.property == "TextContent")
        {
            let value = cmd
                .text_tuple_value
                .as_ref()
                .map(|value| quote! { #value })
                .unwrap_or_else(|| quote! { "" });
            let apply = syn::parse2::<syn::Expr>(value.clone())
                .ok()
                .and_then(|value| direct_format_args_call(&value, &quote! { #var }, capacity))
                .unwrap_or_else(|| {
                    quote! {
                        mirui::ui::property::apply_bounded_text::<{ #capacity }>(
                            #var,
                            ::core::format_args!("{}", #value),
                        );
                    }
                });
            tokens.extend(quote! {
                mirui::__macro_support::with_world_scope(#world, || {
                    #apply
                });
            });
        }

        let layout_fields = cmd
            .style_fields
            .iter()
            .map(|key| quote::format_ident!("{key}"));
        let explicit = quote! { (0u64 #(| ::mirui::ui::style::StyleField::#layout_fields.bit())*) };
        let kind = &cmd.name;
        let widget = if cmd.kind == WidgetKind::Component {
            quote!(#kind)
        } else {
            quote!(())
        };
        let static_preset = cmd.preset.as_ref().filter(|preset| !preset.reactive);
        let applied = if let Some(preset) = static_preset {
            let preset = &preset.value;
            quote! {
                ::mirui::ui::style::apply_preset::<#widget, _>(#world, #var, #preset, #explicit)
            }
        } else {
            explicit.clone()
        };
        let fields = syn::Ident::new(
            "__mirui_authored_style_fields",
            proc_macro2::Span::mixed_site(),
        );
        if cmd.name == "Button" || cmd.classes.is_some() {
            tokens.extend(quote! {
                let #fields = #applied;
            });
        } else if static_preset.is_some() {
            tokens.extend(quote! { #applied; });
        }
        if cmd.name == "Button" {
            tokens.extend(quote! {
                ::mirui::ui::style::mark_button_layout(#world, #var, #fields);
            });
        }

        let column = matches!(cmd.name.to_string().as_str(), "Column" | "Scroll");
        let direction = if column { quote!(Column) } else { quote!(Row) };
        let clips = cmd.name == "Scroll";
        let defaults = quote! {
            ::mirui::ui::style::StyleDefaults {
                direction: ::mirui::ui::layout::FlexDirection::#direction,
                clip_children: #clips,
            }
        };
        let class_installation = cmd.classes.as_ref().map(|classes| {
            if !classes.reactive {
                let names = &classes.value;
                return quote! {
                    ::mirui::ui::style::install_classes(#world, #var, #names, #fields, #defaults);
                };
            }
            let run = syn::Ident::new("__mirui_class_binding", proc_macro2::Span::mixed_site());
            let once = syn::Ident::new("__mirui_class_once", proc_macro2::Span::mixed_site());
            let static_input = classes
                .static_input
                .as_ref()
                .map_or_else(|| quote!(false), |prop| quote!(#prop.__mirui_is_static()));
            let read = reactive_read(&classes.value);
            tokens.extend(quote! {
                let #once = #static_input;
                let #run = ::mirui::ui::style::prepare_class_binding(#var, move || #read);
            });
            quote! {
                ::mirui::ui::style::install_classes(
                    #world, #var, ::mirui::ui::ClassList::default(), #fields, #defaults,
                );
                #run(#world, #once);
            }
        });

        let preset_binding = cmd
            .preset
            .as_ref()
            .filter(|preset| preset.reactive)
            .map(|preset| {
                let name =
                    syn::Ident::new("__mirui_style_binding", proc_macro2::Span::mixed_site());
                let once = syn::Ident::new("__mirui_style_once", proc_macro2::Span::mixed_site());
                let static_input = preset
                    .static_input
                    .as_ref()
                    .map_or_else(|| quote!(false), |prop| quote!(#prop.__mirui_is_static()));
                let read = reactive_read(&preset.value);
                tokens.extend(quote! {
                    let #once = #static_input;
                    let #name = ::mirui::ui::style::prepare_style_binding::<#widget, _>(
                        #var, #explicit, #defaults, move || #read,
                    );
                });
                (name, once)
            });

        // Capture bindings before attachments may move their declared Models.
        let mut prepared_bindings = Vec::new();
        for (index, binding) in cmd.reactive_binds.iter().enumerate() {
            let prepared = syn::Ident::new(
                &format!("{var}_binding_{index}"),
                proc_macro2::Span::mixed_site(),
            );
            let property = &binding.property;
            let once = binding.static_input.as_ref().map(|input| {
                let flag = quote::format_ident!("{prepared}_once");
                tokens.extend(quote! { let #flag = #input.__mirui_is_static(); });
                flag
            });
            let role = binding_role(property);
            let expr = &binding.expr;
            let body = if property == "Page" || property == "PageTransition" {
                let kind = quote::format_ident!("{var}_kind");
                let field = quote::format_ident!(
                    "{}",
                    if property == "Page" {
                        "page"
                    } else {
                        "transition"
                    }
                );
                quote! {
                    let __value = #expr;
                    ::mirui::__macro_support::apply_component_field(#kind, #var, __value, |component, value| {
                        let value = ::core::convert::Into::into(value);
                        if component.#field == value { return false; }
                        component.#field = value;
                        true
                    });
                }
            } else if property == "TextContent"
                && (cmd.name == "Text" || cmd.name == "Button")
                && let Some(capacity) = &cmd.capacity
            {
                direct_format_args_call(&binding.format_expr, &quote! { #var }, capacity)
                    .unwrap_or_else(|| {
                        quote! {
                            mirui::ui::property::apply_bounded_text::<{ #capacity }>(
                                #var,
                                ::core::format_args!("{}", #expr),
                            );
                        }
                    })
            } else {
                let value = if property == "TextContent" {
                    quote! { mirui::ui::property::IntoText::into_text(__v) }
                } else {
                    quote! { ::core::convert::Into::into(__v) }
                };
                quote! {
                    let __v = #expr;
                    mirui::ui::property::apply::<mirui::ui::property::prop::#property>(#var, #value);
                }
            };
            tokens.extend(quote! {
                let #prepared = mirui::__macro_support::prepare_widget_effect(#var, #role, move || {
                    #body
                });
            });
            prepared_bindings.push((prepared, once));
        }

        let enchants = &cmd.enchants;
        for enchant in enchants {
            tokens.extend(quote! {
                (#world).insert(#var, #enchant);
            });
        }
        if let Some(id_lit) = &cmd.id {
            tokens.extend(quote! {
                (#world).insert(#var, mirui::ui::NamedId(#id_lit));
            });
        }
        tokens.extend(quote! {
            let __widget_parent = #parent_var;
            ::mirui::__macro_support::finish_widget_static(#world, __widget_parent, #var);
        });
        tokens.extend(class_installation);
        if let Some((name, once)) = preset_binding {
            tokens.extend(quote! { #name(#world, #once); });
        }

        for (property, value) in &cmd.property_values {
            tokens.extend(quote! {
                ::mirui::ui::property::apply_to_world::<::mirui::ui::property::prop::#property>(
                    #world,
                    #var,
                    ::core::convert::Into::into(#value),
                );
            });
        }

        for (prepared, once) in prepared_bindings {
            if let Some(once) = once {
                tokens.extend(quote! {
                    if #once { #prepared.install_once(#world); } else { #prepared.install(#world); }
                });
            } else {
                tokens.extend(quote! { #prepared.install(#world); });
            }
        }
        for install in &cmd.field_bindings {
            tokens.extend(quote! { #install(#world, #var); });
        }

        if !cmd.layout_binds.is_empty() {
            let mut dependencies = Vec::new();
            for binding in &cmd.layout_binds {
                for dependency in &binding.value.dependencies {
                    if !dependencies
                        .iter()
                        .any(|candidate: &crate::layout_reactive::Dependency| {
                            candidate.same_source(dependency)
                        })
                    {
                        dependencies.push(dependency.clone());
                    }
                }
            }
            if dependencies.len() > 4 {
                tokens.extend(
                    syn::Error::new(
                        cmd.layout_binds[0].property.span(),
                        "layout-responsive properties on one widget use more than four distinct dependencies",
                    )
                    .to_compile_error(),
                );
            } else {
                let emitted_dependencies = dependencies.iter().map(|dependency| dependency.emit());
                let mut applies = Vec::new();
                for binding in &cmd.layout_binds {
                    let mut value = binding.value.clone();
                    match value.emit_apply(&binding.property, &dependencies) {
                        Ok(apply) => applies.push(apply),
                        Err(error) => tokens.extend(error.to_compile_error()),
                    }
                }
                tokens.extend(quote! {
                    (#world).insert(
                        #var,
                        ::mirui::__macro_support::LayoutBinding::new(
                            const { &[#(#emitted_dependencies),*] },
                            |__layout_world, __layout_target, __layout_values| {
                                let mut __layout_changed = false;
                                #(#applies)*
                                __layout_changed
                            },
                        ),
                    );
                });
            }
        }

        if !on_handlers.is_empty() {
            tokens.extend(Self::emit_on_dispatch(
                &cmd.name.to_string(),
                &on_handlers,
                &cmd.var,
                world,
            ));
        }

        let var_ts = quote! { #var };
        tokens.extend(
            Self::emit_siblings(&cmd.children, world, &var_ts)
                .collect::<proc_macro2::TokenStream>(),
        );

        tokens
    }
}
