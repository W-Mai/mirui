use super::SurfaceDescriptor;
use crate::palette::ColorTableView;

impl SurfaceDescriptor {
    pub(crate) fn read_color_table(
        self,
        bytes: Option<&[u8]>,
    ) -> Result<Option<ColorTableView<'_>>, ColorTableError> {
        self.validate_color_table_size(bytes.map(<[u8]>::len))?;
        Ok(bytes
            .map(|bytes| ColorTableView::from_rgba_bytes(bytes).expect("checked palette length")))
    }

    pub(crate) fn validate_color_table_size(
        self,
        size: Option<usize>,
    ) -> Result<(), ColorTableError> {
        match (self.sample_layout().color_table_entries(), size) {
            (Some(entries), Some(actual)) => {
                let expected = usize::try_from(entries)
                    .ok()
                    .and_then(|entries| entries.checked_mul(4))
                    .ok_or(ColorTableError::SizeOverflow)?;
                if actual != expected {
                    return Err(ColorTableError::SizeMismatch { expected, actual });
                }
                Ok(())
            }
            (Some(_), None) => Err(ColorTableError::Missing),
            (None, Some(_)) => Err(ColorTableError::Unexpected),
            (None, None) => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ColorTableError {
    Missing,
    Unexpected,
    SizeMismatch { expected: usize, actual: usize },
    SizeOverflow,
}
