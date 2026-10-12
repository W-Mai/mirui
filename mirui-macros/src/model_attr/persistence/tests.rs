use super::*;

#[test]
fn persistence_declarations_reject_ambiguous_metadata() {
    for options in [
        quote!(persistence, persistence),
        quote!(persistence(key = "")),
        quote!(persistence(key = "a", key = "b")),
        quote!(persistence(key = 3)),
        quote!(persistence(other = 3)),
    ] {
        assert!(
            super::super::expand(
                options,
                quote!(
                    struct Settings {
                        value: u8,
                    }
                )
            )
            .is_err()
        );
    }
    for declaration in [
        quote!(
            struct Settings {
                #[persist]
                #[persist]
                value: u8,
            }
        ),
        quote!(
            struct Settings {
                #[persist(1)]
                value: u8,
            }
        ),
        quote!(
            struct Settings(#[persist] u8);
        ),
    ] {
        assert!(super::super::expand(quote!(persistence), declaration).is_err());
    }
    assert!(
        super::super::expand(
            quote!(),
            quote!(
                struct Settings {
                    #[persist]
                    value: u8,
                }
            )
        )
        .is_err()
    );
}

#[test]
fn feature_selection_is_delegated_to_the_dependency() {
    let output = super::super::expand(
        quote!(persistence),
        quote!(
            struct Settings {
                #[persist]
                value: String,
            }
        ),
    )
    .unwrap()
    .to_string();
    assert!(output.contains("__mirui_persistence"));
    assert!(!output.contains("feature = \"persistence\""));
    assert!(!output.contains("# [persist]"));
}
