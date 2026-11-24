use super::*;

#[test]
fn walk_root_ownership_preserves_the_direct_path_and_checks_replacing_roots() {
    let direct = expand_tree(quote! { Column() { walk ${ items() } with item { View() } } });
    assert!(!quote!(#direct).to_string().contains("with_owned_effects"));
    let compound = expand_tree(quote! {
        Column() { walk ${ items() } with item { if ${ visible() } { View() } else { View() View() } } }
    });
    assert!(quote!(#compound).to_string().contains("with_owned_effects"));
    let replacing = expand_tree(quote! {
        Column() { walk ${ items() } with item { walk ${ nested() } with child { View() } } }
    });
    assert!(
        quote!(#replacing)
            .to_string()
            .contains("need a widget parent")
    );
    let payload = expand_tree(quote! {
        Column() { walk ${ items() } with item { match ${ selected() } { Some(value) => { View() }, None => { View() } } } }
    });
    assert!(
        quote!(#payload)
            .to_string()
            .contains("need a widget parent")
    );
    let contained = expand_tree(quote! {
        Column() { walk ${ items() } with item { Row() { walk ${ nested() } with child { View() } } } }
    });
    assert!(
        !quote!(#contained)
            .to_string()
            .contains("need a widget parent")
    );
}

#[test]
fn repeated_component_properties_are_rejected_during_lowering() {
    let output = expand_tree(quote! { PrivateState(value: 1_u16, value: 2_u16) });
    assert!(
        quote!(#output)
            .to_string()
            .contains("duplicate component property `value`")
    );
}

fn expand_tree(input: proc_macro2::TokenStream) -> syn::Expr {
    let (captures, props, input) = strip_bound_captures(input).unwrap();
    let tree = syn::parse2::<DsTree>(input).unwrap().into_ref();
    let mut rune = MiruiRune::new();
    rune.props = props;
    rune.world_expr = scope_world_expr();
    rune.parent_expr = quote!(__parent);
    decipher(&tree, &mut rune);
    let body = rune.seal();
    syn::parse2(capture_output(quote!({ #body }), captures)).unwrap()
}

#[test]
fn selectors_options_and_callback_forms_use_one_typed_installation_path() {
    let output = expand_tree(quote! {
        Column() {
            Button("Go") on Tap(next_count(), callback::callback())
            View() [Pad] on Pad::Hit(|input| input.value > 0)
            Alias() on Tap { event.x > Fixed::ZERO }
            Row() on Tap(|input: event!(Tap), state: res!(mut State)| {
                state.0 += input.x;
                false
            })
        }
    });
    let output = quote!(#output).to_string();
    assert_eq!(output.matches("install_entity_event").count(), 4);
    assert_eq!(output.matches("next_count").count(), 1);
    assert!(output.contains("Button :: Tap"));
    assert!(output.contains("Pad :: Hit"));
    assert!(output.contains("Alias :: Tap"));
    assert!(output.contains("mirui :: ui :: Widget :: Tap"));
    assert!(output.contains("__bind_plain"));
    assert!(output.contains("__bind_params"));
    assert!(output.contains("__bind_callback"));
    assert!(output.contains("NodeContext"));
    assert!(output.contains("callback :: callback ()"));
    assert!(!output.contains("GestureHandler"));
    assert!(!output.contains("BusinessCallback"));
    assert!(!output.contains("EventCommandQueue"));
}

#[test]
fn declared_lexical_model_captures_are_shared_and_registered_before_invocation() {
    let output = expand_tree(quote! {
        __mirui_bind(game, other);
        Column() {
            Button("Go") on Tap { game.increment(); }
            Button("Role") on Tap(|input: event!(Tap), state: res!(mut State)| {
                state.0 += input.x;
                game.increment();
            })
            Button("Shadow") on Tap(|game| { let _ = game; })
            Button("Named") on Tap(handler::callback(other.clone()))
        }
    });
    let output = quote!(#output).to_string();
    assert_eq!(output.matches("__mirui_capture_owner_game =").count(), 2);
    assert_eq!(
        output
            .matches("require_binding (& __mirui_capture_owner_game)")
            .count(),
        2
    );
    assert_eq!(output.matches("__mirui_capture_owner_other =").count(), 1);
    assert!(output.contains("handler :: callback (other . clone ())"));
    assert!(!output.contains("__mirui_event_world"));
}

#[test]
fn child_blocks_precede_callback_forms_on_the_same_widget() {
    let output = expand_tree(quote! {
        View() [Pad] {
            child_panel()
        } on Tap(|input: event!(Tap), rows: query!(Pad), node: entity!()| {
            rows.get(node).is_some() && input.x > Fixed::ZERO
        }) on DragStart(|input| input.x > Fixed::ZERO)
    });
    let output = quote!(#output).to_string();
    assert_eq!(output.matches("install_entity_event").count(), 2);
    assert!(output.contains("child_panel"));
    assert!(output.contains("__bind_params"));
    assert!(output.contains("__bind_plain"));
    assert!(!output.contains(". configure"));
}

#[test]
fn compose_children_share_declared_models_before_factory_execution() {
    let output = expand_tree(quote! {
        __mirui_bind(game);
        Column() {
            child_panel(game, label)
            child_panel(game, label)
        }
    });
    let output = quote!(#output).to_string();
    assert_eq!(
            output
                .matches("__compose_factory = child_panel (:: mirui :: core :: model :: SharedValue :: share (& game) , label)")
                .count(),
            2,
        );
    assert_eq!(
        output
            .matches("__compose_factory (& mut __compose_cx)")
            .count(),
        2,
    );
    assert!(!output.contains("child_panel (& mut __compose_cx"));
}

#[test]
fn compose_call_form_executes_factory_and_preserves_result_expression() {
    let call = syn::parse2(quote!(helpers::value::<u8>(source, 7))).unwrap();
    let output = expand_ui_fn_call(call);
    let expression: syn::Expr = syn::parse2(output).unwrap();
    assert_eq!(
        quote!(#expression).to_string(),
        quote!({
            let __compose_factory = helpers::value::<u8>(source, 7);
            __compose_factory(cx)
        })
        .to_string(),
    );
}
