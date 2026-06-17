use crate::core::cache::HasSize;

/// Dual of [`HasSize`]: [`HasSize`] quantifies a value's byte cost so the
/// cache can evict by bytes; [`HasProbe`] extracts a cheap metadata snapshot
/// so a caller can answer "what is this resource" without a full decode.
///
/// `Meta: HasSize` is required because the manager stores Meta values in a
/// secondary cache that follows the cache framework's API contract.
pub trait HasProbe {
    type Meta: Clone + HasSize + 'static;

    fn extract_meta(&self) -> Self::Meta;
}
