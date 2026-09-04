use super::SurfaceDescriptor;
use crate::payload::ColorTableView;

impl SurfaceDescriptor {
    pub(super) fn read_color_table(
        self,
        bytes: Option<&[u8]>,
    ) -> Result<Option<ColorTableView<'_>>, ColorTableError> {
        match (self.sample_layout().color_table_entries(), bytes) {
            (Some(entries), Some(bytes)) => {
                let expected = usize::try_from(entries)
                    .ok()
                    .and_then(|entries| entries.checked_mul(4))
                    .ok_or(ColorTableError::SizeOverflow)?;
                if bytes.len() != expected {
                    return Err(ColorTableError::SizeMismatch {
                        expected,
                        actual: bytes.len(),
                    });
                }
                Ok(Some(
                    ColorTableView::from_rgba_bytes(bytes).expect("checked palette length"),
                ))
            }
            (Some(_), None) => Err(ColorTableError::Missing),
            (None, Some(_)) => Err(ColorTableError::Unexpected),
            (None, None) => Ok(None),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ColorTableError {
    Missing,
    Unexpected,
    SizeMismatch { expected: usize, actual: usize },
    SizeOverflow,
}
