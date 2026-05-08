use crate::types::{Fixed, Rect};

/// Compare two framebuffers and return the minimal bounding rect of changed pixels.
/// Returns None if no change.
pub fn dirty_rect(prev: &[u8], curr: &[u8], width: u16, height: u16) -> Option<Rect> {
    let stride = width as usize * 4;
    let mut min_x = width as i32;
    let mut min_y = height as i32;
    let mut max_x: i32 = -1;
    let mut max_y: i32 = -1;

    for y in 0..height as usize {
        let row_offset = y * stride;
        let prev_row = &prev[row_offset..row_offset + stride];
        let curr_row = &curr[row_offset..row_offset + stride];
        if prev_row != curr_row {
            if (y as i32) < min_y {
                min_y = y as i32;
            }
            if (y as i32) > max_y {
                max_y = y as i32;
            }
            // Find x bounds in this row
            for x in 0..width as usize {
                let px = x * 4;
                if prev_row[px..px + 4] != curr_row[px..px + 4] {
                    if (x as i32) < min_x {
                        min_x = x as i32;
                    }
                    if (x as i32) > max_x {
                        max_x = x as i32;
                    }
                }
            }
        }
    }

    if max_x < 0 {
        None
    } else {
        Some(Rect {
            x: Fixed::from_int(min_x),
            y: Fixed::from_int(min_y),
            w: Fixed::from_int(max_x - min_x + 1),
            h: Fixed::from_int(max_y - min_y + 1),
        })
    }
}

/// Extract a sub-region from an RGBA framebuffer into a contiguous buffer.
pub fn extract_region(buf: &[u8], width: u16, rect: &Rect, out: &mut [u8]) {
    let stride = width as usize * 4;
    let rw = rect.w.to_int() as usize * 4;
    for row in 0..rect.h.to_int() as usize {
        let src_y = (rect.y.to_int() as usize + row) * stride + rect.x.to_int() as usize * 4;
        let dst_y = row * rw;
        out[dst_y..dst_y + rw].copy_from_slice(&buf[src_y..src_y + rw]);
    }
}
