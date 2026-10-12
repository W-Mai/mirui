use super::{state::PixelSurface, style::PALETTE};
use crate::gallery::play::pixel::{GRID_HEIGHT, GRID_WIDTH, PixelFrame};
use crate::ui::clipboard::{
    ClipboardData, ClipboardError, ClipboardFormat, ClipboardItem, CopySelection, CutSelection,
    InspectSelection, PasteSelection, SelectionState,
};
use crate::ui::selection::{RequestError, SelectAll};
use alloc::vec::Vec;

const MIME: &str = "application/x-mirui-pixel-frame";
const HEADER: [u8; 6] = [b'P', b'X', b'L', 1, GRID_WIDTH, GRID_HEIGHT];
const PALETTE_END: usize = HEADER.len() + PALETTE.len() * 4;
const SNAPSHOT_BYTES: usize = PALETTE_END + GRID_WIDTH as usize * GRID_HEIGHT as usize / 2;

fn palette_entry(index: usize) -> [u8; 4] {
    let color = PALETTE[index];
    [color.r, color.g, color.b, if index == 0 { 0 } else { 255 }]
}

fn export(frame: PixelFrame) -> Result<ClipboardData, ClipboardError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(SNAPSHOT_BYTES)
        .map_err(|_| ClipboardError::Capacity)?;
    bytes.extend_from_slice(&HEADER);
    for index in 0..PALETTE.len() {
        bytes.extend_from_slice(&palette_entry(index));
    }
    bytes.extend_from_slice(frame.bytes());
    let mut item = ClipboardItem::with_capacity(1)?;
    item.push(ClipboardFormat::new(MIME, bytes))?;
    let mut data = ClipboardData::with_capacity(1, SNAPSHOT_BYTES)?;
    data.push(item)?;
    Ok(data)
}

fn import(data: &ClipboardData) -> Result<PixelFrame, ClipboardError> {
    let [item] = data.items() else {
        return Err(ClipboardError::Unsupported);
    };
    let bytes = item
        .formats()
        .iter()
        .find(|format| format.mime().eq_ignore_ascii_case(MIME))
        .ok_or(ClipboardError::Unsupported)?
        .bytes();
    if bytes.len() != SNAPSHOT_BYTES || bytes[..HEADER.len()] != HEADER {
        return Err(ClipboardError::InvalidData);
    }
    for (index, color) in bytes[HEADER.len()..PALETTE_END].chunks_exact(4).enumerate() {
        if color != palette_entry(index) {
            return Err(ClipboardError::InvalidData);
        }
    }
    PixelFrame::from_bytes(&bytes[PALETTE_END..]).ok_or(ClipboardError::InvalidData)
}

#[crate::event]
pub(super) fn select_all(
    node: entity!(),
    _event: event!(SelectAll),
    surfaces: query!(PixelSurface),
) -> Result<(), RequestError> {
    let surface = surfaces.get(node).ok_or(RequestError::Unavailable)?;
    surface
        .model
        .selection_revision()
        .ok_or(RequestError::Unavailable)?;
    Ok(())
}

#[crate::event]
pub(super) fn inspect(
    node: entity!(),
    _event: event!(InspectSelection),
    surfaces: query!(PixelSurface),
) -> Result<SelectionState, ClipboardError> {
    let surface = surfaces.get(node).ok_or(ClipboardError::Unavailable)?;
    let revision = surface
        .model
        .selection_revision()
        .ok_or(ClipboardError::Unavailable)?;
    Ok(SelectionState::new(true, revision))
}

#[crate::event]
pub(super) fn copy(
    node: entity!(),
    _event: event!(CopySelection),
    surfaces: query!(PixelSurface),
) -> Result<ClipboardData, ClipboardError> {
    let surface = surfaces.get(node).ok_or(ClipboardError::Unavailable)?;
    surface
        .model
        .selection_revision()
        .ok_or(ClipboardError::Unavailable)?;
    export(surface.model.selected_frame())
}

#[crate::event]
pub(super) fn cut(
    node: entity!(),
    _event: event!(CutSelection),
    surfaces: query!(PixelSurface),
) -> Result<(), ClipboardError> {
    let surface = surfaces.get(node).ok_or(ClipboardError::Unavailable)?;
    surface
        .model
        .replace_frame(PixelFrame::EMPTY)
        .map(|_| ())
        .map_err(|_| ClipboardError::Unavailable)
}

#[crate::event]
pub(super) fn paste(
    node: entity!(),
    event: event!(PasteSelection),
    surfaces: query!(PixelSurface),
) -> Result<(), ClipboardError> {
    let frame = import(event.data())?;
    let surface = surfaces.get(node).ok_or(ClipboardError::Unavailable)?;
    surface
        .model
        .replace_frame(frame)
        .map(|_| ())
        .map_err(|_| ClipboardError::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_preserves_pixels_and_rejects_incomplete_or_foreign_frames() {
        let frame = PixelFrame::from_bytes(&[0x61; 72]).unwrap();
        let data = export(frame).unwrap();
        assert_eq!(import(&data), Ok(frame));
        let original = data.items()[0].formats()[0].bytes();
        for (index, value) in [(0, 0), (4, 13), (6, 0), (PALETTE_END, 0xf0)] {
            let mut bytes = original.to_vec();
            bytes[index] = value;
            let mut item = ClipboardItem::with_capacity(1).unwrap();
            item.push(ClipboardFormat::new(MIME, bytes)).unwrap();
            let mut invalid = ClipboardData::with_capacity(1, SNAPSHOT_BYTES).unwrap();
            invalid.push(item).unwrap();
            assert_eq!(import(&invalid), Err(ClipboardError::InvalidData));
        }
        assert_eq!(
            import(&ClipboardData::text("pixels").unwrap()),
            Err(ClipboardError::Unsupported)
        );
        assert!(PixelFrame::from_bytes(&original[PALETTE_END..original.len() - 1]).is_none());
    }
}
