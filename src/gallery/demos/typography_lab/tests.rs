use super::caret::CaretOverlay;
use super::runtime::{ARABIC, CJK, DEVANAGARI, ELLIPSIS, FALLBACKS, THAI, UI, mixed_stack};
use super::state::TypographyModel;
use super::style::BACKGROUND;
use super::*;
use crate::core::reactive::flush_signal_dirty;
use crate::input::event::GestureHandler;
use crate::input::event::gesture::GestureEvent;
use crate::input::event::scroll::scroll_bounds;
use crate::prelude::*;
use crate::render::font::FontManager;
use crate::types::Viewport;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::slider::{SliderEvent, SliderHandler};
use crate::ui::widgets::{LanguageTag, Text, TextAlign, TextDirection, TextOverflow, TextWrap};
use crate::ui::{ComputedRect, Parent};

fn fixture_at(width: u16, height: u16) -> World {
    let mut app = App::headless(width, height);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    ViewRegistry::reconcile_observations(&mut app.world);
    flush_signal_dirty(&mut app.world);
    app.world
}

fn fixture() -> World {
    fixture_at(VIEWPORT.0, VIEWPORT.1)
}

fn tap(world: &mut World, id: &'static str) {
    let entity = world.find_by_id(id).expect("control id");
    GestureHandler::trigger(
        world,
        entity,
        &GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: entity,
        },
    );
    flush_signal_dirty(world);
}

fn with_model<R>(world: &World, inspect: impl FnOnce(&TypographyModel) -> R) -> R {
    let overlay = world.find_by_id("typography_path_sample").unwrap();
    let overlay = world.get::<CaretOverlay>(overlay).unwrap();
    crate::core::model::ModelHandle::read(&overlay.model, inspect)
}

fn bounded_text_pointer(world: &World, id: &'static str) -> *const u8 {
    let text = world
        .get::<Text>(world.find_by_id(id).unwrap())
        .expect("bounded text");
    match text.content() {
        crate::ui::widgets::TextContent::Plain(alloc::borrow::Cow::Owned(value)) => value.as_ptr(),
        _ => panic!("{id} does not own bounded text storage"),
    }
}

#[test]
fn builds_the_complete_typography_matrix() {
    let world = fixture();

    for id in [
        "typography_latin",
        "typography_cjk",
        "typography_arabic",
        "typography_thai",
        "typography_devanagari",
        "typography_bidi",
        "typography_rasters",
        "typography_path",
        "typography_contour",
        "typography_projective_sample",
    ] {
        assert!(world.find_by_id(id).is_some(), "missing {id}");
    }
}

#[test]
fn phone_layout_stacks_controls_below_the_grid_and_scrolls_to_them() {
    use crate::input::event::scroll::scroll_bounds;
    use crate::ui::ComputedRect;

    for (width, height) in [(320, 568), (422, 600), (480, 320)] {
        let mut world = fixture_at(width, height);
        let shell = world.find_by_id("typography_lab_shell").unwrap();
        let mut root = shell;
        while let Some(parent) = world.get::<Parent>(root).map(|parent| parent.0) {
            root = parent;
        }
        crate::ui::render_system::update_layout(
            &mut world,
            root,
            &Viewport::new(width, height, Fixed::ONE),
        );
        let grid = world.find_by_id("typography_lab_grid").unwrap();
        let controls = world.find_by_id("typography_controls").unwrap();
        let shell_rect = world.get::<ComputedRect>(shell).unwrap().0;
        let grid_rect = world.get::<ComputedRect>(grid).unwrap().0;
        let controls_rect = world.get::<ComputedRect>(controls).unwrap().0;
        let bounds = scroll_bounds(&world, shell).unwrap();
        let extent = bounds.content_height;
        let max_offset = bounds.max_y;

        assert!(
            controls_rect.y >= grid_rect.y + grid_rect.h,
            "{width}x{height}",
        );
        assert!(max_offset > Fixed::ZERO, "{width}x{height}");
        assert!(
            controls_rect.y + controls_rect.h - max_offset <= shell_rect.y + shell_rect.h,
            "{width}x{height}: {controls_rect:?} {shell_rect:?} {extent:?}",
        );
        assert!(
            controls_rect.y + controls_rect.h - max_offset > shell_rect.y,
            "{width}x{height}: {controls_rect:?} {shell_rect:?} {extent:?}",
        );
    }
}

#[test]
fn large_phone_scroll_repaints_the_destination_content() {
    use crate::input::event::scroll::{ScrollDelta, ScrollOffset};
    use crate::surface::FramebufferAccess;
    use crate::ui::dirty::Dirty;

    let mut app = App::headless(422, 600);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.render().unwrap();
    let shell = app.world.find_by_id("typography_lab_shell").unwrap();
    let bounds = scroll_bounds(&app.world, shell).unwrap();
    let max_offset = bounds.max_y;
    app.world.get_mut::<ScrollOffset>(shell).unwrap().y = max_offset;
    app.world.insert(
        shell,
        ScrollDelta {
            dx: Fixed::ZERO,
            dy: max_offset,
        },
    );
    app.world.insert(shell, Dirty);
    app.render().unwrap();

    let background = Theme::default().resolve(BACKGROUND);
    let painted = app
        .backend
        .framebuffer()
        .buf
        .as_slice()
        .chunks_exact(4)
        .filter(|pixel| pixel[..3] != [background.r, background.g, background.b])
        .count();
    let controls = app.world.find_by_id("typography_controls").unwrap();
    let controls_rect = app.world.get::<ComputedRect>(controls).unwrap().0;
    assert!(
        painted > 4_000,
        "large scroll left the viewport blank: painted={painted}, bounds={bounds:?}, max_offset={max_offset:?}, controls={controls_rect:?}",
    );
}

#[test]
fn labels_use_text_components() {
    let world = fixture();

    for id in [
        "typography_panel_count",
        "typography_wrap",
        "typography_align",
        "typography_overflow",
    ] {
        let entity = world.find_by_id(id).expect("label id");
        assert!(world.get::<Text>(entity).is_some(), "{id} is not Text");
    }
}

#[test]
fn fallback_stack_stays_borrowed_and_ordered() {
    let stack = mixed_stack();
    assert_eq!(stack.primary(), &UI);
    assert_eq!(stack.fallbacks(), &FALLBACKS);
}

#[test]
fn mixed_bidi_sample_ellipsizes_within_its_card() {
    let mut world = fixture();
    let mut root = world.find_by_id("typography_bidi").unwrap();
    while let Some(parent) = world.get::<Parent>(root).map(|parent| parent.0) {
        root = parent;
    }
    crate::ui::render_system::update_layout(
        &mut world,
        root,
        &Viewport::new(VIEWPORT.0, VIEWPORT.1, Fixed::ONE),
    );

    let card = world.find_by_id("typography_bidi").unwrap();
    let sample = world.find_by_id("typography_bidi_sample").unwrap();
    let card_rect = world.get::<crate::ui::ComputedRect>(card).unwrap().0;
    let sample_rect = world.get::<crate::ui::ComputedRect>(sample).unwrap().0;
    assert!(sample_rect.x + sample_rect.w <= card_rect.x + card_rect.w);

    let handle = world.get::<crate::text::TextLayoutHandle>(sample).unwrap();
    let layouts = world
        .resource::<crate::text::layout::TextLayoutResource>()
        .unwrap()
        .borrow();
    let layout = layouts.get(*handle).unwrap();
    let text = world.get::<Text>(sample).unwrap().resolve(&world);
    assert_eq!(layout.lines().len(), 1);
    assert!(layout.lines()[0].text().end < text.len() as u32);
    assert!(crate::types::fixed::from_textflow(layout.measure().width) <= sample_rect.w);
    let manager = world.resource::<FontManager>().unwrap();
    let ellipsis = [UI, CJK, ARABIC, DEVANAGARI, THAI, ELLIPSIS]
        .iter()
        .find_map(|token| manager.resolve(token.cache_key()).map_char('…'))
        .expect("ellipsis glyph in the configured font stack");
    assert!(
        layout
            .glyphs()
            .iter()
            .any(|glyph| glyph.glyph_id() == ellipsis)
    );
}

#[test]
fn path_tap_updates_the_caret_probe() {
    let mut world = fixture();
    let sample = world.find_by_id("typography_path_sample").unwrap();
    let overlay = sample;
    let live_overlay = world.find_by_id("typography_live_sample").unwrap();
    assert!(world.has::<CaretOverlay>(overlay));
    assert!(world.has::<CaretOverlay>(live_overlay));
    assert_eq!(with_model(&world, |model| model.path_probe), None);
    world.remove::<crate::ui::dirty::VisualDirty>(overlay);
    world.remove::<crate::ui::dirty::VisualDirty>(live_overlay);

    GestureHandler::trigger(
        &mut world,
        sample,
        &GestureEvent::Tap {
            x: Fixed::from_int(42),
            y: Fixed::from_int(55),
            target: sample,
        },
    );
    flush_signal_dirty(&mut world);

    assert_eq!(
        with_model(&world, |model| model.path_probe),
        Some(Point::new(42, 55))
    );
    assert!(world.has::<crate::ui::dirty::VisualDirty>(overlay));
    assert!(!world.has::<crate::ui::dirty::VisualDirty>(live_overlay));
    world.remove::<crate::ui::dirty::VisualDirty>(overlay);

    GestureHandler::trigger(
        &mut world,
        sample,
        &GestureEvent::DragMove {
            x: Fixed::from_int(70),
            y: Fixed::from_int(60),
            dx: Fixed::from_int(28),
            dy: Fixed::from_int(5),
            target: sample,
        },
    );
    flush_signal_dirty(&mut world);
    assert_eq!(
        with_model(&world, |model| model.path_probe),
        Some(Point::new(70, 60))
    );
    assert!(world.has::<crate::ui::dirty::VisualDirty>(overlay));
}

#[test]
fn path_sample_shares_curved_bidi_interaction_geometry() {
    let mut world = fixture();
    let root = world.find_by_id("typography_path").unwrap();
    let mut parent = root;
    while let Some(next) = world.get::<Parent>(parent).map(|parent| parent.0) {
        parent = next;
    }
    crate::ui::render_system::update_layout(
        &mut world,
        parent,
        &Viewport::new(VIEWPORT.0, VIEWPORT.1, Fixed::ONE),
    );
    let sample = world.find_by_id("typography_path_sample").unwrap();
    let geometry = crate::ui::widgets::text::PathTextGeometry::for_widget(&world, sample).unwrap();
    let text = world.get::<Text>(sample).unwrap().resolve(&world);
    let mut storage = [crate::ui::widgets::text::PathSelectionRibbon::default(); 32];
    let ribbons = geometry
        .selection_into(0..text.len() as u32, &mut storage)
        .unwrap();

    assert!(ribbons.iter().any(|ribbon| ribbon.bidi_level() == 0));
    assert!(ribbons.iter().any(|ribbon| ribbon.bidi_level() & 1 == 1));
    assert!(ribbons.windows(2).any(|pair| {
        let first = pair[0].quad();
        let second = pair[1].quad();
        first[1].x - first[0].x != second[1].x - second[0].x
            || first[1].y - first[0].y != second[1].y - second[0].y
    }));

    let ribbon = ribbons[0];
    let quad = ribbon.quad();
    let probe = Point {
        x: (quad[0].x + quad[3].x) / Fixed::from_int(2),
        y: (quad[0].y + quad[3].y) / Fixed::from_int(2),
    };
    let hit = geometry.hit_test(probe, Fixed::ONE).unwrap().unwrap();
    let range = ribbon.text_range();
    assert!(hit.text_offset() == range.start || hit.text_offset() == range.end);
}

#[test]
fn samples_keep_their_shaping_contracts() {
    let world = fixture();

    let arabic = world
        .get::<Text>(world.find_by_id("typography_arabic_sample").unwrap())
        .unwrap();
    assert_eq!(arabic.paragraph().direction, TextDirection::RightToLeft);
    assert_eq!(
        arabic
            .paragraph()
            .language
            .as_ref()
            .map(LanguageTag::as_str),
        Some("ar")
    );

    let thai = world
        .get::<Text>(world.find_by_id("typography_thai_sample").unwrap())
        .unwrap();
    assert_eq!(thai.paragraph().direction, TextDirection::LeftToRight);
    assert_eq!(
        thai.paragraph().language.as_ref().map(LanguageTag::as_str),
        Some("th")
    );

    let devanagari = world
        .get::<Text>(world.find_by_id("typography_devanagari_sample").unwrap())
        .unwrap();
    assert_eq!(devanagari.paragraph().direction, TextDirection::LeftToRight);
    assert_eq!(
        devanagari
            .paragraph()
            .language
            .as_ref()
            .map(LanguageTag::as_str),
        Some("hi")
    );

    let bidi = world
        .get::<crate::ui::Style>(world.find_by_id("typography_bidi_sample").unwrap())
        .unwrap();
    assert_eq!(bidi.font_stack.primary(), &UI);
    assert_eq!(bidi.font_stack.fallbacks(), &FALLBACKS);
}

#[test]
fn controls_publish_into_the_live_paragraph() {
    let mut world = fixture();
    let dynamic_ids = [
        "typography_ppem_value",
        "typography_width_value",
        "typography_wrap",
        "typography_align",
        "typography_overflow",
    ];
    let text_buffers = dynamic_ids.map(|id| bounded_text_pointer(&world, id));
    let sample = world
        .find_by_id("typography_live_sample")
        .expect("sample id");
    let ppem = world.find_by_id("typography_ppem").expect("ppem id");
    let width = world.find_by_id("typography_width").expect("width id");

    for (slider, new) in [(ppem, 42), (width, 320)] {
        let callback = world
            .get::<SliderHandler>(slider)
            .expect("slider handler")
            .on_event
            .clone_out();
        callback.call(
            &mut world,
            slider,
            &SliderEvent::ValueChanged {
                new: Fixed::from_int(new),
                old: Fixed::ZERO,
            },
        );
    }
    flush_signal_dirty(&mut world);
    tap(&mut world, "typography_wrap");
    tap(&mut world, "typography_align");
    tap(&mut world, "typography_overflow");

    let style = world.get::<crate::ui::Style>(sample).unwrap();
    assert_eq!(style.font_size, Some(42));
    assert_eq!(style.layout.width, Dimension::px(320));
    let paragraph = world.get::<Text>(sample).unwrap().paragraph();
    assert_eq!(paragraph.wrap, TextWrap::Grapheme);
    assert_eq!(paragraph.align, TextAlign::Center);
    assert_eq!(paragraph.overflow, TextOverflow::Ellipsis);
    assert_eq!(paragraph.max_lines, Some(2));

    for (id, value, capacity) in [
        ("typography_ppem_value", "42", 2),
        ("typography_width_value", "320", 3),
        ("typography_wrap", "GRAPHEME", 8),
        ("typography_align", "CENTER", 7),
        ("typography_overflow", "ELLIPSIS", 8),
    ] {
        let text = world
            .get::<Text>(world.find_by_id(id).unwrap())
            .expect("bounded value text");
        assert_eq!(text.resolve(&world).as_ref(), value, "{id}");
        assert_eq!(text.text_capacity(), Some(capacity), "{id}");
        assert!(text.has_valid_content(), "{id}");
    }
    for (id, pointer) in dynamic_ids.into_iter().zip(text_buffers) {
        assert_eq!(bounded_text_pointer(&world, id), pointer, "{id}");
    }
}
