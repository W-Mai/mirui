use mirui::app::App;
use mirui::core::model::SharedValue;
use mirui::core::reactive::flush_signal_dirty;
use mirui::model;
use mirui::render::RenderError;
use mirui::surface::FramebufferAccess;
use mirui::text::{
    TextLayoutBuffer, TextLayoutCapacity, TextLayoutError, TextLayoutLimits, WorkspaceCapacity,
};
use mirui::types::Dimension;
use mirui::ui;
use mirui::ui::render_system::TextLayoutFailureKind;
use mirui::ui::widgets::Text;

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;
use tracking_allocator::tracked_allocations;

const OVERSIZED_TEXT: &str = "THIS PARAGRAPH IS MUCH LONGER THAN THE DECLARED OUTPUT STORAGE AND MUST REPORT A CAPACITY ERROR";

#[model]
struct Mode {
    #[observe]
    selected: u8,
}

#[model]
impl Mode {
    fn select(&mut self, selected: u8) {
        self.selected = selected;
    }
}

fn capacity() -> TextLayoutCapacity {
    TextLayoutCapacity {
        layout_slots: 4,
        measurements: 4,
        lines: 16,
        runs: 32,
        glyphs: 64,
        carets: 64,
        workspace: WorkspaceCapacity {
            runs: 16,
            glyphs: 32,
            scratch_glyphs: 32,
            lines: 16,
        },
    }
}

#[test]
fn bounded_text_match_switches_without_layout_allocations_and_reports_overflow() {
    let mut app = App::headless(64, 64);
    app.with_default_widgets();
    app.with_text_layout_limits(TextLayoutLimits::HOST.with_cache_bytes(64 * 1024));
    app.with_text_layout_capacity(capacity()).unwrap();
    let root = app.spawn_root().id();
    let mode = app.add_model(Mode { selected: 0 });
    let selection = mode.share();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Column (width: Dimension::px(64), height: Dimension::px(64)) {
            Text ("HEADER", id: "header")
            match ${ selection.selected() } {
                0 => {
                    Text ("ONE", id: "first")
                }
                _ => {
                    Text ("TWO", id: "second")
                    Text ("THREE", id: "second_extra")
                }
            }
        }
    };

    app.render().unwrap();
    let initial = app.backend.framebuffer().buf.as_slice().to_vec();
    assert_eq!(
        tracked_allocations(|| {
            mode.select(1);
            flush_signal_dirty(&mut app.world);
            app.render_dirty().unwrap();
        }),
        0
    );
    let switched = app.backend.framebuffer().buf.as_slice().to_vec();
    assert_ne!(switched, initial);

    assert_eq!(
        tracked_allocations(|| {
            mode.select(0);
            flush_signal_dirty(&mut app.world);
            app.render_dirty().unwrap();
        }),
        0
    );
    assert_eq!(app.backend.framebuffer().buf.as_slice(), initial);

    let second = app.world.find_by_id("second").unwrap();
    mode.select(1);
    flush_signal_dirty(&mut app.world);
    app.world
        .get_mut::<Text>(second)
        .unwrap()
        .set_content(OVERSIZED_TEXT);
    assert_eq!(app.render_dirty(), Err(RenderError::TextLayout));
    let failure = app.last_text_layout_failure().unwrap();
    assert_eq!(
        failure.kind,
        TextLayoutFailureKind::Text(TextLayoutError::Capacity {
            buffer: TextLayoutBuffer::WorkspaceGlyphs,
            required: OVERSIZED_TEXT.len(),
            capacity: 32,
        })
    );
    assert_eq!(app.backend.framebuffer().buf.as_slice(), initial);

    app.world
        .get_mut::<Text>(second)
        .unwrap()
        .set_content("TWO");
    app.render_dirty().unwrap();
    assert_eq!(app.last_text_layout_failure(), None);
    assert_eq!(app.backend.framebuffer().buf.as_slice(), switched);

    assert_eq!(
        app.with_text_layout_capacity(capacity()).err(),
        Some(TextLayoutError::InUse)
    );
    app.handle_memory_warning();
    assert_eq!(
        app.with_text_layout_capacity(capacity()).err(),
        Some(TextLayoutError::InUse)
    );
    mode.select(0);
    flush_signal_dirty(&mut app.world);
    app.render_dirty().unwrap();
    assert_eq!(app.backend.framebuffer().buf.as_slice(), initial);
}

#[test]
fn failed_bounded_reservation_preserves_existing_app_cache() {
    let mut app = App::headless(64, 64);
    app.with_default_widgets();
    app.with_text_layout_capacity(capacity()).unwrap();
    app.with_text_layout_limits(TextLayoutLimits::HOST.with_cache_bytes(9 * 1024));
    assert!(matches!(
        app.try_with_text_layout_limits(TextLayoutLimits::HOST.with_cache_bytes(4 * 1024)),
        Err(TextLayoutError::CacheBudget { .. })
    ));
    let oversized = TextLayoutCapacity {
        glyphs: 4_096,
        ..capacity()
    };
    assert!(matches!(
        app.with_text_layout_capacity(oversized),
        Err(TextLayoutError::CacheBudget { .. })
    ));
    let root = app.spawn_root().id();
    ui! {
        :(
            parent: root
            world: &mut app.world
        :)
        Text ("PRESERVED")
    };
    app.render().unwrap();
    assert_eq!(
        app.try_with_text_layout_limits(TextLayoutLimits::HOST)
            .err(),
        Some(TextLayoutError::InUse)
    );
}
