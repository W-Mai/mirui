extern crate std;
use super::*;
use crate::render::renderer::Renderer;
use crate::render::sw::SwRenderer;
use crate::render::texture::{ColorFormat, Texture};

/// End-to-end check: scroll-blit + strip repaint matches a full
/// repaint of the post-scroll state. Covers the `Rect` arithmetic
/// the walker hands to the renderer; unit-level shifts are tested
/// directly in `sw/mod.rs`.
#[test]
fn scroll_blit_then_strip_paint_matches_full_repaint() {
    let mut blit_buf = std::vec![0u8; 32 * 32 * 4];
    let mut full_buf = std::vec![0u8; 32 * 32 * 4];
    for y in 0..32 {
        let red = (y * 8) as u8;
        for x in 0..32 {
            let off = (y * 32 + x) * 4;
            for buf in [&mut blit_buf, &mut full_buf] {
                buf[off] = red;
                buf[off + 1] = 0;
                buf[off + 2] = 0;
                buf[off + 3] = 255;
            }
        }
    }

    {
        let tex = Texture::new(&mut blit_buf, 32, 32, ColorFormat::RGBA8888)
            .expect("valid pixel storage");
        let mut backend = SwRenderer::new(tex);
        let area = Rect::new(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(32),
            Fixed::from_int(32),
        );
        backend
            .scroll_target_region(&area, Fixed::ZERO, Fixed::from_int(-4))
            .unwrap();
        for y in 28..32 {
            let post_shift_red = ((y + 4) * 8) as u8;
            for x in 0..32 {
                let off = (y * 32 + x) * 4;
                backend.target.pixels_mut().unwrap().bytes_mut()[off] = post_shift_red;
            }
        }
    }

    {
        for y in 0..32 {
            let post_shift_red = ((y + 4) * 8) as u8;
            for x in 0..32 {
                let off = (y * 32 + x) * 4;
                full_buf[off] = post_shift_red;
            }
        }
    }

    for y in 0..32 {
        for x in 0..32 {
            let off = (y * 32 + x) * 4;
            assert_eq!(
                blit_buf[off], full_buf[off],
                "mismatch at ({x},{y}): blit={} full={}",
                blit_buf[off], full_buf[off]
            );
        }
    }
}

/// Nested scroll-blit: outer container shifts dy=-4, inner
/// (sub-rect at rows 8..24, height 16) shifts dy=-2 in its own
/// frame. End state must equal a full repaint where each row's
/// red channel is shifted by the cumulative offset that applies
/// at that y. Outer rows outside the inner sub-rect see only the
/// outer shift; rows inside see outer-then-inner.
#[test]
fn nested_scroll_blit_matches_full_repaint() {
    let mut blit_buf = std::vec![0u8; 32 * 32 * 4];
    let mut full_buf = std::vec![0u8; 32 * 32 * 4];
    for y in 0..32 {
        let red = (y * 4) as u8;
        for x in 0..32 {
            let off = (y * 32 + x) * 4;
            for buf in [&mut blit_buf, &mut full_buf] {
                buf[off] = red;
                buf[off + 1] = 0;
                buf[off + 2] = 0;
                buf[off + 3] = 255;
            }
        }
    }

    // Apply nested shifts: inner shift first (DFS post-order
    // — inner content moves in its own frame), then outer shift
    // pulls the already-shifted inner along with everything.
    {
        let tex = Texture::new(&mut blit_buf, 32, 32, ColorFormat::RGBA8888)
            .expect("valid pixel storage");
        let mut backend = SwRenderer::new(tex);
        let inner_area = Rect::new(
            Fixed::ZERO,
            Fixed::from_int(8),
            Fixed::from_int(32),
            Fixed::from_int(16),
        );
        backend
            .scroll_target_region(&inner_area, Fixed::ZERO, Fixed::from_int(-2))
            .unwrap();
        // Inner strip exposed at rows 22..24 (bottom of inner).
        for y in 22..24 {
            let post_inner_red = ((y + 2) * 4) as u8;
            for x in 0..32 {
                let off = (y * 32 + x) * 4;
                backend.target.pixels_mut().unwrap().bytes_mut()[off] = post_inner_red;
            }
        }

        let outer_area = Rect::new(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(32),
            Fixed::from_int(32),
        );
        backend
            .scroll_target_region(&outer_area, Fixed::ZERO, Fixed::from_int(-4))
            .unwrap();
        // Outer strip exposed at rows 28..32.
        for y in 28..32 {
            // After both shifts: row y on screen reads from
            // (y + 4) in original buffer, but if (y + 4) was in
            // the inner area (8..24) it had already been shifted
            // by 2 — so its source content was at (y + 4 + 2).
            let src_y = y + 4;
            let red = if (8..24).contains(&src_y) {
                ((src_y + 2) * 4) as u8
            } else {
                (src_y * 4) as u8
            };
            for x in 0..32 {
                let off = (y * 32 + x) * 4;
                backend.target.pixels_mut().unwrap().bytes_mut()[off] = red;
            }
        }
    }

    // Reference: a fresh full-repaint of the post-shift state.
    // Each row y reads from its post-shift source row.
    for y in 0..32 {
        // Outer shift -4: every row y on screen comes from y+4
        // in the pre-outer-shift buffer.
        let post_outer_y = y + 4;
        // Within the inner sub-rect (8..24), inner shift -2
        // composes: pre-inner buffer was at post_outer_y + 2.
        let src_y = if (8..24).contains(&post_outer_y) {
            post_outer_y + 2
        } else {
            post_outer_y
        };
        let red = (src_y * 4) as u8;
        for x in 0..32 {
            let off = (y * 32 + x) * 4;
            full_buf[off] = red;
        }
    }

    for y in 0..32 {
        for x in 0..32 {
            let off = (y * 32 + x) * 4;
            assert_eq!(
                blit_buf[off], full_buf[off],
                "row {y} mismatch: blit={} full={}",
                blit_buf[off], full_buf[off]
            );
        }
    }
}
