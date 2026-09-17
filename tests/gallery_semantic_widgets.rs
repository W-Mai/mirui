use std::fs;
use std::path::Path;

fn container_text_attributes(source: &str) -> Vec<&'static str> {
    let mut violations = Vec::new();
    for widget in ["View", "Row", "Column"] {
        let mut offset = 0usize;
        while let Some(relative) = source[offset..].find(widget) {
            let index = offset + relative;
            let after = index + widget.len();
            offset = after;
            let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
            if index > 0 && is_ident(source.as_bytes()[index - 1]) {
                continue;
            }
            if source
                .as_bytes()
                .get(after)
                .is_some_and(|byte| is_ident(*byte))
            {
                continue;
            }
            let rest = &source[after..];
            let Some(header) = rest.trim_start().strip_prefix('(') else {
                continue;
            };
            let mut depth = 1usize;
            let mut end = None;
            for (index, byte) in header.bytes().enumerate() {
                match byte {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(index);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if end.is_some_and(|end| header[..end].contains("text:")) {
                violations.push(widget);
            }
        }
    }
    violations
}

#[test]
fn gallery_containers_do_not_carry_text_attributes() {
    let demos = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gallery/demos");
    let mut violations = Vec::new();

    for entry in fs::read_dir(demos).expect("gallery demos directory") {
        let path = entry.expect("gallery demo entry").path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("gallery demo source");
        for widget in container_text_attributes(&source) {
            violations.push(format!("{}: {widget}", path.display()));
        }
    }

    assert!(
        violations.is_empty(),
        "container text must use a Text child:\n{}",
        violations.join("\n")
    );
}

#[test]
fn semantic_guard_detects_container_text() {
    assert_eq!(
        container_text_attributes("ui! { View (height: 20, text: \"bad\") }"),
        ["View"]
    );
    assert!(container_text_attributes("ui! { View (height: 20) { Text (\"ok\") } }").is_empty());
}

#[test]
fn themed_gallery_surfaces_do_not_embed_palette_literals() {
    for source in [
        include_str!("../src/gallery/demos/nested_scroll.rs"),
        include_str!("../src/gallery/demos/scroll.rs"),
        include_str!("../src/gallery/demos/tabbar.rs"),
        include_str!("../src/gallery/demos/lazy_list.rs"),
        include_str!("../src/gallery/demos/pinch_rotate.rs"),
        include_str!("../src/gallery/demos/layout_lab.rs"),
        include_str!("../src/gallery/demos/interaction_lab.rs"),
        include_str!("../src/gallery/demos/curve_text.rs"),
        include_str!("../src/gallery/demos/curve_text_compact.rs"),
        include_str!("../src/gallery/demos/typography_lab.rs"),
        include_str!("../src/gallery/demos/book_flip.rs"),
        include_str!("../src/gallery/demos/flip_card.rs"),
        include_str!("../src/gallery/demos/image_flip.rs"),
        include_str!("../src/gallery/demos/kinetic_console.rs"),
        include_str!("../src/gallery/demos/orbit_console.rs"),
    ] {
        assert!(!source.contains("bg_color: Color::"));
        assert!(!source.contains("text_color: Color::"));
    }
}

#[test]
fn gallery_theme_ownership_stays_with_theme_studies() {
    let demos = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gallery/demos");
    let theme_studies = ["theme_swap.rs", "widgets.rs", "widgets_compact.rs"];
    let mut violations = Vec::new();

    for entry in fs::read_dir(demos).expect("gallery demos directory") {
        let path = entry.expect("gallery demo entry").path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.extension().is_none_or(|extension| extension != "rs")
            || theme_studies.contains(&file_name)
        {
            continue;
        }

        let source = fs::read_to_string(&path).expect("gallery demo source");
        let runtime = source.split("#[cfg(test)]").next().unwrap_or(&source);
        if runtime.contains("Theme::light()")
            || runtime.contains("Theme::dark()")
            || runtime.contains("theme::set_theme(")
            || runtime.contains(".with_theme(")
        {
            violations.push(path.display().to_string());
        }
    }

    assert!(
        violations.is_empty(),
        "only theme studies may replace the active gallery theme:\n{}",
        violations.join("\n")
    );
}

#[test]
fn native_compact_demos_are_not_registered_as_stretchable() {
    let registry = include_str!("../gallery/web/src/lib.rs");
    for slug in [
        "curve_text_compact",
        "kinetic_console",
        "effect_glass",
        "widgets_compact",
    ] {
        let entry = registry
            .lines()
            .find(|line| line.contains(&format!("(\"{slug}\"")))
            .unwrap_or_else(|| panic!("compact demo `{slug}` must be registered"));
        let compact: String = entry.split_whitespace().collect();
        assert!(
            compact.contains(",128,128,false),"),
            "compact demo `{slug}` must retain its native 128 x 128 canvas"
        );
    }
}
