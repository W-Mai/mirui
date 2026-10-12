use alloc::vec::Vec;
use core::marker::PhantomData;

use super::{PersistenceError, archive, value::PersistModel};
use crate::core::model::Model;

/// A complete archive format for one Model type.
///
/// Encoding receives the raw Model and a reusable output buffer. Preparation validates all bytes and completes fallible work without modifying the Model. Application consumes that prepared result without allocation, business effects, or additional validation; apply it only to the instance used for preparation, without intervening edits.
pub trait ModelCodec: 'static {
    type Model: Model;
    type Prepared;

    /// Encode the complete storage record. The caller clears `output` before invoking this method.
    fn encode(&self, model: &Self::Model, output: &mut Vec<u8>) -> Result<(), PersistenceError>;

    /// Prepare restoration from the complete storage record, with no framework prefix removed.
    fn prepare(
        &self,
        model: &Self::Model,
        bytes: &[u8],
    ) -> Result<Self::Prepared, PersistenceError>;

    /// Commit a successfully prepared restoration to the unchanged source instance.
    fn apply(&self, model: &mut Self::Model, prepared: Self::Prepared);
}

/// The generated field archive format for a persistent Model.
#[doc(hidden)]
pub struct FieldCodec<M>(PhantomData<fn() -> M>);

impl<M> Default for FieldCodec<M> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<M: PersistModel> ModelCodec for FieldCodec<M> {
    type Model = M;
    type Prepared = M::Prepared;

    fn encode(&self, model: &M, output: &mut Vec<u8>) -> Result<(), PersistenceError> {
        archive::encode(&M::SCHEMA, output, |writer| model.encode(writer))
    }

    fn prepare(&self, model: &M, bytes: &[u8]) -> Result<Self::Prepared, PersistenceError> {
        model.prepare(archive::Archive::parse(bytes)?.root())
    }

    fn apply(&self, model: &mut M, prepared: Self::Prepared) {
        model.apply(prepared);
    }
}
