use mirui::app::App;
use mirui::core::model::SharedValue;
use mirui::core::reactive::flush_signal_dirty;
use mirui::input::event::hit_test::hit_test;
use mirui::model;
use mirui::surface::FramebufferAccess;
use mirui::text::{TextLayoutCapacity, WorkspaceCapacity};
use mirui::types::Color;
use mirui::ui;
use mirui::ui::widgets::{Button, Text};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;
use tracking_allocator::tracked_allocations;

#[model]
struct Mode {
    #[observe]
    deep: bool,
}

#[model]
impl Mode {
    fn select(&mut self, deep: bool) {
        self.deep = deep;
    }
}

#[test]
fn retained_text_branch_switch_reuses_prepared_handle_storage() {
    let mut app = App::headless(128, 128);
    app.with_default_widgets();
    app.with_text_layout_capacity(TextLayoutCapacity {
        layout_slots: 6,
        measurements: 6,
        lines: 24,
        runs: 40,
        glyphs: 64,
        carets: 96,
        workspace: WorkspaceCapacity {
            runs: 16,
            glyphs: 32,
            scratch_glyphs: 32,
            lines: 16,
        },
    })
    .unwrap();
    let root = app.spawn_root().id();
    let mode = app.add_model(Mode { deep: false });
    let branch = mode.share();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)
        Column (width: 128, height: 80) {
            match ${ branch.deep() } {
                false => {
                    Text (
                        "SHALLOW TEXT", id: "shallow_hit", width: 128, height: 16,
                        font_size: 8, text_color: Color::rgb(255, 240, 180),
                        bg_color: Color::rgb(30, 70, 100)
                    ) on Tap {}
                }
                _ => {
                    Column (width: 128, height: 32) {
                        Column (width: 128, height: 32) {
                            Text (
                                "DEEP TEXT", id: "deep_hit", width: 128, height: 16,
                                font_size: 8, text_color: Color::rgb(255, 240, 180),
                                bg_color: Color::rgb(85, 30, 80)
                            ) on Tap {}
                            Text (
                                "SECOND LINE", width: 128, height: 16,
                                font_size: 8, text_color: Color::rgb(255, 240, 180)
                            )
                        }
                    }
                }
            }
            Text ("TAIL MARKER", width: 128, height: 16, font_size: 8)
            Text ("007", width: 128, height: 16, font_size: 8)
            Button (text: "007", width: 128, height: 16, font_size: 8) on Tap {}
        }
    };

    let shallow = app.world.find_by_id("shallow_hit").unwrap();
    let deep = app.world.find_by_id("deep_hit").unwrap();
    app.render().unwrap();
    let shallow_frame = app.backend.framebuffer().buf.as_slice().to_vec();
    assert_eq!(
        hit_test(&app.world, root, 8.into(), 8.into(), 128, 128),
        Some(shallow)
    );

    assert_eq!(
        tracked_allocations(|| {
            mode.select(true);
            flush_signal_dirty(&mut app.world);
            app.render_dirty().unwrap();
        }),
        0,
        "first retained text branch switch allocated"
    );
    let deep_frame = app.backend.framebuffer().buf.as_slice().to_vec();
    assert_ne!(deep_frame, shallow_frame);
    assert_eq!(
        hit_test(&app.world, root, 8.into(), 8.into(), 128, 128),
        Some(deep)
    );

    assert_eq!(
        tracked_allocations(|| {
            mode.select(false);
            flush_signal_dirty(&mut app.world);
            app.render_dirty().unwrap();
        }),
        0,
        "retained text branch return allocated"
    );
    assert_eq!(app.backend.framebuffer().buf.as_slice(), shallow_frame);
    assert_eq!(
        hit_test(&app.world, root, 8.into(), 8.into(), 128, 128),
        Some(shallow)
    );
}
