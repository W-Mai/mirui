use super::payload::resolve_node_payload;
use super::{Document, DocumentChunkRef, DocumentState, EditError};
use crate::payload::image::ImagePayloadError;
use crate::scene::{Scene, VectorAccessError, VectorEncodeError, VectorReadError};
use crate::{ChunkFlags, ChunkId, ChunkType};

#[cfg(test)]
use crate::scene::{CodecError, SceneOp, VectorChunkHeader};

impl<'a> DocumentChunkRef<'a> {
    /// Decodes this chunk into an owned VECTOR scene.
    pub fn decode_vector(&self) -> Result<Scene, VectorAccessError> {
        if self.chunk_type() != ChunkType::VECTOR {
            return Err(VectorAccessError::UnexpectedChunkType {
                actual: self.chunk_type(),
            });
        }
        let payload = resolve_node_payload(self.document(), self.node())
            .map_err(vector_access_resolution_error)?;
        let bytes = payload
            .bytes()
            .ok_or(VectorAccessError::NonContiguousPayload)?;
        Scene::decode_with_limits(bytes, &self.document().payload_limits).map_err(Into::into)
    }
}

impl Document<'_> {
    /// Resolves and bounded-decodes one VECTOR node by its stable identity.
    ///
    /// The document's retained [`crate::PayloadLimits`] profile bounds every owned
    /// scene component. Preserved trailing bytes do not block typed reads.
    pub(super) fn decode_vector_at(&self, id: ChunkId) -> Result<Scene, VectorAccessError> {
        let DocumentState::Chunk(chunks) = &self.state else {
            return Err(VectorAccessError::ChunkLayoutRequired);
        };
        let node = chunks
            .chunks
            .iter()
            .find(|node| node.id == id)
            .ok_or(VectorAccessError::InvalidChunkId)?;
        if node.chunk_type != ChunkType::VECTOR {
            return Err(VectorAccessError::UnexpectedChunkType {
                actual: node.chunk_type,
            });
        }

        let payload = resolve_node_payload(self, node).map_err(vector_access_resolution_error)?;
        let bytes = payload
            .bytes()
            .ok_or(VectorAccessError::NonContiguousPayload)?;
        Scene::decode_with_limits(bytes, &self.payload_limits).map_err(Into::into)
    }

    /// Appends one checked VECTOR payload with no chunk flags.
    pub fn push_vector(&mut self, scene: &Scene) -> Result<ChunkId, EditError> {
        self.push_vector_with_flags(scene, ChunkFlags::NONE)
    }

    /// Appends one checked VECTOR payload with explicit chunk flags.
    ///
    /// Structural gates and the document's retained resource profile are
    /// checked before one canonical payload allocation is committed.
    pub fn push_vector_with_flags(
        &mut self,
        scene: &Scene,
        flags: ChunkFlags,
    ) -> Result<ChunkId, EditError> {
        let limits = self.payload_limits;
        self.push_typed_owned_with(ChunkType::VECTOR, flags, || {
            let plan = scene.payload_plan().map_err(EditError::InvalidVector)?;
            scene
                .validate_limits(&limits)
                .map_err(invalid_vector_read_error)?;
            plan.payload_to_vec().map_err(vector_encode_error_for_edit)
        })
    }

    /// Replaces one VECTOR payload without changing its identity or descriptor.
    ///
    /// Typed replacement is an explicit canonical rewrite. The existing payload
    /// is not decoded, so malformed or segmented VECTOR nodes can be repaired.
    pub(super) fn replace_vector(&mut self, id: ChunkId, scene: &Scene) -> Result<(), EditError> {
        let limits = self.payload_limits;
        self.replace_typed_owned_with(
            id,
            ChunkType::VECTOR,
            || {
                let plan = scene.payload_plan().map_err(EditError::InvalidVector)?;
                scene
                    .validate_limits(&limits)
                    .map_err(invalid_vector_read_error)?;
                Ok(plan)
            },
            |_, _| Ok(false),
            |plan| plan.payload_to_vec().map_err(vector_encode_error_for_edit),
            vector_edit_resolution_error,
        )
    }

    pub(super) fn begin_vector_edit(&mut self, id: ChunkId) -> Result<Scene, EditError> {
        self.ensure_mutable()?;
        self.decode_vector_at(id)
            .map_err(vector_access_error_for_edit)
    }
}

fn invalid_vector_read_error(error: VectorReadError) -> EditError {
    match error {
        VectorReadError::AllocationFailed => EditError::AllocationFailed,
        error => EditError::InvalidVector(VectorEncodeError::InvalidPayload(error)),
    }
}

fn vector_encode_error_for_edit(error: VectorEncodeError) -> EditError {
    match error {
        VectorEncodeError::AllocationFailed
        | VectorEncodeError::InvalidPayload(VectorReadError::AllocationFailed) => {
            EditError::AllocationFailed
        }
        error => EditError::InvalidVector(error),
    }
}

fn vector_access_resolution_error(_: ImagePayloadError) -> VectorAccessError {
    VectorAccessError::InvalidPayload(VectorReadError::SizeOverflow)
}

fn vector_edit_resolution_error(_: ImagePayloadError) -> EditError {
    invalid_vector_read_error(VectorReadError::SizeOverflow)
}

fn vector_access_error_for_edit(error: VectorAccessError) -> EditError {
    match error {
        VectorAccessError::ChunkLayoutRequired => EditError::ChunkLayoutRequired,
        VectorAccessError::InvalidChunkId => EditError::InvalidChunkId,
        VectorAccessError::UnexpectedChunkType { .. } => EditError::InvalidChunkType,
        VectorAccessError::NonContiguousPayload => EditError::NonContiguousPayload {
            chunk_type: ChunkType::VECTOR,
        },
        VectorAccessError::InvalidPayload(error) => invalid_vector_read_error(error),
        VectorAccessError::AllocationFailed => EditError::AllocationFailed,
    }
}

#[cfg(test)]
mod tests {
    use alloc::{borrow::Cow, string::String, vec, vec::Vec};
    use core::mem::size_of;

    use super::*;
    use crate::document::{
        CriticalAssumption, EncodeOptions, OpenOptions, PayloadInput, PayloadOrigin, RawChunkInput,
        RawChunkPolicy, RelocationAssumption, ReservedBitsPolicy,
    };
    use crate::extension::SourcePolicy;
    use crate::path::{Path, PathCmd};
    use crate::scene::{
        FillRule, GlyphPlacement, GradientStop, GradientUnits, LineCap, LineJoin, LinearGradient,
        Paint, ResourceRef, SpreadMode,
    };
    use crate::types::{Color, Fixed, Point, Transform};
    use crate::{
        ColorFormat, ImageAsset, Layout, PayloadLimits, TrailingBytesPolicy, crc32, encode_chunks,
    };

    fn id(counter: u32) -> ChunkId {
        ChunkId::new(counter)
    }

    fn point(x: i32, y: i32) -> Point {
        Point::new(Fixed::from_int(x), Fixed::from_int(y))
    }

    fn representative_scene() -> Scene {
        let path = Path::from_cmds(vec![PathCmd::MoveTo(point(1, 2)), PathCmd::Close]);
        Scene::from_ops(vec![
            SceneOp::GroupBegin {
                transform: None,
                projective: None,
                opacity: Some(200),
                clip: Some(ResourceRef::Token(String::from("clip"))),
                mask: Some(ResourceRef::Inline(Path::from_cmds(vec![PathCmd::Close]))),
                filter: None,
                disjoint_hint: false,
            },
            SceneOp::FillPath {
                path,
                transform: Transform::IDENTITY,
                paint: Paint::LinearGradient(LinearGradient {
                    start: Point::ZERO,
                    end: point(4, 4),
                    stops: Cow::Owned(vec![GradientStop {
                        offset: Fixed::ZERO,
                        color: Color::rgba(1, 2, 3, 255),
                    }]),
                    spread: SpreadMode::Pad,
                    units: GradientUnits::UserSpaceOnUse,
                    transform: Transform::IDENTITY,
                }),
                opa: 255,
                fill_rule: FillRule::NonZero,
            },
            SceneOp::StrokePath {
                path: Path::from_cmds(vec![PathCmd::Close]),
                transform: Transform::IDENTITY,
                paint: Paint::Color(Color::rgb(4, 5, 6)),
                width: Fixed::ONE,
                opa: 240,
                line_cap: LineCap::Round,
                line_join: LineJoin::Bevel,
                miter_limit: Fixed::from_int(4),
                dash: Cow::Owned(vec![Fixed::ONE, Fixed::from_int(2)]),
            },
            SceneOp::GlyphRun {
                font: ResourceRef::Token(String::from("font")),
                ppem: 16,
                pos: point(2, 3),
                transform: Transform::IDENTITY,
                color: Color::rgb(7, 8, 9),
                opa: 230,
                glyphs: vec![
                    GlyphPlacement::new(1, point(0, 12)),
                    GlyphPlacement::new(2, point(8, 12)),
                ],
            },
            SceneOp::GroupEnd,
        ])
    }

    fn vector_file(payload: &[u8], flags: ChunkFlags) -> Vec<u8> {
        encode_chunks(&[(ChunkType::VECTOR.raw(), flags.bits(), payload)])
    }

    fn explicit_policy() -> RawChunkPolicy {
        RawChunkPolicy {
            relocation: RelocationAssumption::AssumeRelocatable,
            critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
            reserved_flag_bits: ReservedBitsPolicy::Reject,
        }
    }

    fn preserve_policy() -> RawChunkPolicy {
        RawChunkPolicy {
            relocation: RelocationAssumption::Infer,
            critical_semantics: CriticalAssumption::Infer,
            reserved_flag_bits: ReservedBitsPolicy::Preserve,
        }
    }

    fn flat_document() -> Document<'static> {
        Document::new_flat(ImageAsset::new(
            2,
            2,
            ColorFormat::A8,
            2,
            Cow::Borrowed(&[1, 2, 3, 4]),
        ))
        .unwrap()
    }

    #[test]
    fn typed_push_query_and_reopen_round_trip() {
        let expected = representative_scene();
        let mut document = Document::new();
        let vector_id = document
            .push_vector_with_flags(&expected, ChunkFlags::CRITICAL)
            .unwrap();

        assert_eq!(
            document.get(vector_id).unwrap().decode_vector().unwrap(),
            expected.clone()
        );
        assert_eq!(
            document.get(vector_id).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
        let payload = document.get(vector_id).unwrap().payload_bytes().unwrap();
        assert_eq!(Scene::preflight(payload, &PayloadLimits::EMBEDDED), Ok(()));

        let encoded = document.encode(&EncodeOptions::new()).unwrap();
        let reopened = Document::open(&encoded).unwrap();
        let reopened_id = reopened.chunks().next().unwrap().id();
        assert_eq!(reopened.decode_vector_at(reopened_id).unwrap(), expected);
    }

    #[test]
    fn typed_push_promotes_flat_without_losing_the_original_image() {
        let expected = representative_scene();
        let mut document = flat_document();
        let image_pointer = document.flat_image().unwrap().main().as_ptr();

        let vector_id = document.push_vector(&expected).unwrap();

        assert_eq!(document.layout(), Layout::Chunk);
        assert_eq!(document.chunks().len(), 2);
        let image_id = document.chunks().next().unwrap().id();
        assert_eq!(
            document.get(image_id).unwrap().chunk_type(),
            ChunkType::IMAGE
        );
        assert_eq!(
            document.get(vector_id).unwrap().chunk_type(),
            ChunkType::VECTOR
        );
        assert_eq!(
            document
                .image(image_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main()
                .as_ptr(),
            image_pointer
        );
        assert_eq!(
            document
                .image(image_id)
                .unwrap()
                .raw()
                .unwrap()
                .packed()
                .unwrap()
                .main(),
            &[1, 2, 3, 4]
        );
        assert_eq!(document.decode_vector_at(vector_id).unwrap(), expected);
    }

    #[test]
    fn retained_limits_bound_reads_typed_writes_and_raw_inference() {
        let expected = representative_scene();
        let payload = expected.encode_payload().unwrap();
        let source = vector_file(&payload, ChunkFlags::NONE);
        let decoded_bytes = 5 * size_of::<SceneOp>()
            + 4 * size_of::<PathCmd>()
            + size_of::<GradientStop>()
            + 2 * size_of::<Fixed>()
            + 2 * size_of::<GlyphPlacement>()
            + 8;
        let exact = PayloadLimits::HOST
            .with_max_scene_ops(5)
            .with_max_path_commands(4)
            .with_max_gradient_stops(1)
            .with_max_dash_elements(2)
            .with_max_positioned_glyphs(2)
            .with_max_string_bytes(8)
            .with_max_decoded_bytes(decoded_bytes);
        let cases = [
            (
                exact.with_max_scene_ops(4),
                VectorReadError::TooManySceneOps { count: 5, limit: 4 },
            ),
            (
                exact.with_max_path_commands(3),
                VectorReadError::TooManyPathCommands { count: 4, limit: 3 },
            ),
            (
                exact.with_max_gradient_stops(0),
                VectorReadError::TooManyGradientStops { count: 1, limit: 0 },
            ),
            (
                exact.with_max_dash_elements(1),
                VectorReadError::TooManyDashElements { count: 2, limit: 1 },
            ),
            (
                exact.with_max_positioned_glyphs(1),
                VectorReadError::TooManyPositionedGlyphs { count: 2, limit: 1 },
            ),
            (
                exact.with_max_string_bytes(7),
                VectorReadError::StringBytesLimitExceeded {
                    needed: 8,
                    limit: 7,
                },
            ),
            (
                exact.with_max_decoded_bytes(decoded_bytes - 1),
                VectorReadError::DecodedBytesLimitExceeded {
                    needed: decoded_bytes,
                    limit: decoded_bytes - 1,
                },
            ),
        ];

        let exact_options = OpenOptions::new().with_payload_limits(exact);
        let exact_document = Document::open_with(&source, &exact_options).unwrap();
        let vector_id = exact_document.chunks().next().unwrap().id();
        assert_eq!(
            exact_document.decode_vector_at(vector_id).unwrap(),
            expected
        );

        let mut critical = Document::new_with_limits(exact);
        let critical_id = critical
            .push_raw(RawChunkInput {
                chunk_type: ChunkType::VECTOR,
                flags: ChunkFlags::CRITICAL,
                payload: PayloadInput::Borrowed(&payload),
                policy: RawChunkPolicy::infer(),
            })
            .unwrap();
        assert_eq!(critical.decode_vector_at(critical_id).unwrap(), expected);

        let mut malformed = Document::new_with_limits(exact);
        assert_eq!(
            malformed.push_raw(RawChunkInput {
                chunk_type: ChunkType::VECTOR,
                flags: ChunkFlags::NONE,
                payload: PayloadInput::Borrowed(b"\0"),
                policy: RawChunkPolicy::infer(),
            }),
            Err(EditError::InvalidVector(VectorEncodeError::InvalidPayload(
                VectorReadError::Codec(CodecError::BadMagic)
            )))
        );
        assert_eq!(malformed.chunks().len(), 0);

        for (limits, error) in cases {
            let options = OpenOptions::new().with_payload_limits(limits);
            let document = Document::open_with(&source, &options).unwrap();
            let vector_id = document.chunks().next().unwrap().id();
            assert_eq!(
                document.decode_vector_at(vector_id),
                Err(VectorAccessError::InvalidPayload(error))
            );

            let mut authored = Document::new_with_limits(limits);
            assert_eq!(
                authored.push_vector(&expected),
                Err(EditError::InvalidVector(VectorEncodeError::InvalidPayload(
                    error
                )))
            );
            assert_eq!(authored.chunks().len(), 0);
        }

        let low = exact.with_max_scene_ops(4);
        let mut inferred = Document::new_with_limits(low);
        assert_eq!(
            inferred.push_raw(RawChunkInput {
                chunk_type: ChunkType::VECTOR,
                flags: ChunkFlags::NONE,
                payload: PayloadInput::Borrowed(&payload),
                policy: RawChunkPolicy::infer(),
            }),
            Err(EditError::InvalidVector(VectorEncodeError::InvalidPayload(
                VectorReadError::TooManySceneOps { count: 5, limit: 4 }
            )))
        );
        let opaque = inferred
            .push_raw(RawChunkInput {
                chunk_type: ChunkType::VECTOR,
                flags: ChunkFlags::NONE,
                payload: PayloadInput::Borrowed(&payload),
                policy: explicit_policy(),
            })
            .unwrap();
        assert_eq!(
            inferred.decode_vector_at(opaque),
            Err(VectorAccessError::InvalidPayload(
                VectorReadError::TooManySceneOps { count: 5, limit: 4 }
            ))
        );
    }

    #[test]
    fn replacement_is_an_explicit_canonical_rewrite_and_repairs_malformed_payloads() {
        let expected = representative_scene();
        let canonical = expected.encode_payload().unwrap();
        let source = vector_file(&canonical, ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let vector_id = document.chunks().next().unwrap().id();
        let original_pointer = document
            .get(vector_id)
            .unwrap()
            .payload_bytes()
            .unwrap()
            .as_ptr();

        document.replace_vector(vector_id, &expected).unwrap();
        assert!(document.is_dirty());
        assert_eq!(
            document.get(vector_id).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
        assert_ne!(
            document
                .get(vector_id)
                .unwrap()
                .payload_bytes()
                .unwrap()
                .as_ptr(),
            original_pointer
        );
        assert_eq!(
            document.get(vector_id).unwrap().payload_bytes().unwrap(),
            canonical
        );

        let malformed_source = vector_file(b"\0", ChunkFlags::NONE);
        let mut malformed = Document::open(&malformed_source).unwrap();
        let vector_id = malformed.chunks().next().unwrap().id();
        assert!(matches!(
            malformed.decode_vector_at(vector_id),
            Err(VectorAccessError::InvalidPayload(_))
        ));
        malformed.replace_vector(vector_id, &expected).unwrap();
        assert_eq!(malformed.decode_vector_at(vector_id).unwrap(), expected);
    }

    #[test]
    fn edits_commit_only_after_explicit_commit_and_validation() {
        let expected = representative_scene();
        let source = vector_file(&expected.encode_payload().unwrap(), ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let vector_id = document.chunks().next().unwrap().id();
        let original_pointer = document
            .get(vector_id)
            .unwrap()
            .payload_bytes()
            .unwrap()
            .as_ptr();

        let mut edit = document.get_mut(vector_id).unwrap().edit_vector().unwrap();
        edit.ops.clear();
        drop(edit);
        assert_eq!(
            document
                .get(vector_id)
                .unwrap()
                .payload_bytes()
                .unwrap()
                .as_ptr(),
            original_pointer
        );
        assert!(!document.is_dirty());

        let mut edit = document.get_mut(vector_id).unwrap().edit_vector().unwrap();
        edit.ops.push(SceneOp::GroupEnd);
        assert_eq!(
            edit.commit(),
            Err(EditError::InvalidVector(VectorEncodeError::InvalidPayload(
                VectorReadError::Codec(CodecError::UnbalancedGroup)
            )))
        );
        assert_eq!(
            document
                .get(vector_id)
                .unwrap()
                .payload_bytes()
                .unwrap()
                .as_ptr(),
            original_pointer
        );
        assert!(!document.is_dirty());

        let mut edit = document.get_mut(vector_id).unwrap().edit_vector().unwrap();
        edit.ops.insert(0, SceneOp::PopClip);
        edit.commit().unwrap();
        assert!(document.is_dirty());
        assert!(matches!(
            document.decode_vector_at(vector_id).unwrap().ops.first(),
            Some(SceneOp::PopClip)
        ));
    }

    #[test]
    fn commit_limit_failure_preserves_the_original_payload() {
        let expected = representative_scene();
        let source = vector_file(&expected.encode_payload().unwrap(), ChunkFlags::NONE);
        let limits = PayloadLimits::HOST.with_max_scene_ops(5);
        let options = OpenOptions::new().with_payload_limits(limits);
        let mut document = Document::open_with(&source, &options).unwrap();
        let vector_id = document.chunks().next().unwrap().id();
        let payload = document.get(vector_id).unwrap().payload_bytes().unwrap();
        let original_pointer = payload.as_ptr();
        let original_bytes = payload.to_vec();
        let mut edit = document.get_mut(vector_id).unwrap().edit_vector().unwrap();
        edit.ops.push(SceneOp::PopClip);
        assert_eq!(
            edit.commit(),
            Err(EditError::InvalidVector(VectorEncodeError::InvalidPayload(
                VectorReadError::TooManySceneOps { count: 6, limit: 5 }
            )))
        );
        let payload = document.get(vector_id).unwrap().payload_bytes().unwrap();
        assert_eq!(payload.as_ptr(), original_pointer);
        assert_eq!(payload, original_bytes);
        assert!(!document.is_dirty());
        assert_eq!(document.decode_vector_at(vector_id).unwrap(), expected);
    }

    #[test]
    fn typed_empty_edit_canonicalizes_extension_records_and_crc() {
        let body = [0x40, 0x01, 0xa5, 0x00];
        let mut payload = vec![VectorChunkHeader::MAGIC, 1, 8, 0];
        payload.extend_from_slice(&crc32(&body).to_le_bytes());
        payload.extend_from_slice(&body);
        assert_eq!(Scene::preflight(&payload, &PayloadLimits::HOST), Ok(()));

        let source = vector_file(&payload, ChunkFlags::NONE);
        let mut document = Document::open(&source).unwrap();
        let vector_id = document.chunks().next().unwrap().id();
        assert_eq!(
            document.decode_vector_at(vector_id).unwrap(),
            Scene::default()
        );

        document
            .get_mut(vector_id)
            .unwrap()
            .edit_vector()
            .unwrap()
            .commit()
            .unwrap();

        let canonical = Scene::default().encode_payload().unwrap();
        let rewritten = document.get(vector_id).unwrap().payload_bytes().unwrap();
        assert_eq!(rewritten, canonical);
        assert_ne!(rewritten, payload);
        assert_eq!(Scene::preflight(rewritten, &PayloadLimits::HOST), Ok(()));
        assert_eq!(
            document.get(vector_id).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
        assert!(document.is_dirty());
    }

    #[test]
    fn access_and_edit_errors_keep_container_and_payload_layers_distinct() {
        let expected = representative_scene();
        let payload = expected.encode_payload().unwrap();
        let source = encode_chunks(&[
            (ChunkType::META.raw(), 0, b"meta"),
            (ChunkType::VECTOR.raw(), 0, b"\0"),
        ]);
        let mut document = Document::open(&source).unwrap();
        let mut chunks = document.chunks();
        let meta = chunks.next().unwrap().id();
        let malformed = chunks.next().unwrap().id();
        assert_eq!(
            document.decode_vector_at(meta),
            Err(VectorAccessError::UnexpectedChunkType {
                actual: ChunkType::META
            })
        );
        assert_eq!(
            document.decode_vector_at(id(99)),
            Err(VectorAccessError::InvalidChunkId)
        );
        assert!(matches!(
            document.decode_vector_at(malformed),
            Err(VectorAccessError::InvalidPayload(_))
        ));
        assert!(matches!(
            document.get_mut(malformed).unwrap().edit_vector(),
            Err(EditError::InvalidVector(_))
        ));

        let flat = flat_document();
        assert_eq!(
            flat.decode_vector_at(id(0)),
            Err(VectorAccessError::ChunkLayoutRequired)
        );

        let mut trailing_source = vector_file(&payload, ChunkFlags::NONE);
        trailing_source.extend_from_slice(b"tail");
        let options = OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);
        let trailing = Document::open_with(&trailing_source, &options).unwrap();
        let vector_id = trailing.chunks().next().unwrap().id();
        assert_eq!(trailing.decode_vector_at(vector_id).unwrap(), expected);
    }

    #[test]
    fn segmented_vector_can_be_repaired_without_decode() {
        let mut document = flat_document();
        let promoted = document.promote_to_chunk().unwrap().unwrap();
        assert_eq!(
            document.set_type(promoted, ChunkType::VECTOR, RawChunkPolicy::infer()),
            Err(EditError::NonContiguousPayload {
                chunk_type: ChunkType::VECTOR
            })
        );
        assert_eq!(
            document.get(promoted).unwrap().chunk_type(),
            ChunkType::IMAGE
        );
        document
            .set_type(promoted, ChunkType::VECTOR, explicit_policy())
            .unwrap();
        assert_eq!(
            document.decode_vector_at(promoted),
            Err(VectorAccessError::NonContiguousPayload)
        );
        assert!(matches!(
            document.get_mut(promoted).unwrap().edit_vector(),
            Err(EditError::NonContiguousPayload {
                chunk_type: ChunkType::VECTOR
            })
        ));

        let replacement = representative_scene();
        document.replace_vector(promoted, &replacement).unwrap();
        assert_eq!(document.decode_vector_at(promoted).unwrap(), replacement);
        assert_eq!(
            document.get(promoted).unwrap().payload_origin(),
            PayloadOrigin::OWNED
        );
        let DocumentState::Chunk(chunks) = &document.state else {
            panic!("replacement must retain CHUNK layout")
        };
        assert!(chunks.promoted_flat.is_none());
        assert_eq!(document.layout(), Layout::Chunk);
    }

    #[test]
    fn structural_and_reserved_flag_errors_precede_payload_work() {
        let invalid = Scene::from_ops(vec![SceneOp::GroupEnd]);
        let mut flat = flat_document();
        assert_eq!(
            flat.replace_vector(id(0), &invalid),
            Err(EditError::ChunkLayoutRequired)
        );

        let mut chunk = Document::new();
        assert_eq!(
            chunk.replace_vector(id(0), &invalid),
            Err(EditError::InvalidChunkId)
        );
        let meta = chunk
            .push_raw(RawChunkInput {
                chunk_type: ChunkType::META,
                flags: ChunkFlags::NONE,
                payload: PayloadInput::Borrowed(b"meta"),
                policy: explicit_policy(),
            })
            .unwrap();
        assert_eq!(
            chunk.replace_vector(meta, &invalid),
            Err(EditError::InvalidChunkType)
        );

        let mut exhausted = Document::new();
        exhausted.next_id = u32::MAX;
        assert_eq!(
            exhausted.push_vector_with_flags(&invalid, ChunkFlags::from_bits_retain(2)),
            Err(EditError::ChunkIdExhausted)
        );
        assert_eq!(exhausted.chunks().len(), 0);

        let mut reserved = Document::new();
        assert_eq!(
            reserved.push_vector_with_flags(&invalid, ChunkFlags::from_bits_retain(2)),
            Err(EditError::ReservedFlagBits { bits: 2 })
        );
        assert_eq!(reserved.chunks().len(), 0);
    }

    #[test]
    fn reserved_vector_flags_require_a_grant_for_typed_rewrite() {
        let expected = representative_scene();
        let reserved = ChunkFlags::from_bits_retain(2);
        let source = vector_file(&expected.encode_payload().unwrap(), reserved);
        let mut document = Document::open(&source).unwrap();
        let vector_id = document.chunks().next().unwrap().id();
        assert_eq!(
            document.replace_vector(vector_id, &expected),
            Err(EditError::ReservedFlagBits { bits: 2 })
        );
        assert!(!document.is_dirty());

        let policies = [SourcePolicy {
            chunk_type: ChunkType::VECTOR,
            policy: preserve_policy(),
        }];
        let options = OpenOptions::new().with_source_policies(&policies);
        let mut granted = Document::open_with(&source, &options).unwrap();
        let vector_id = granted.chunks().next().unwrap().id();
        granted.replace_vector(vector_id, &expected).unwrap();
        assert!(granted.is_dirty());
        assert_eq!(granted.get(vector_id).unwrap().flags(), reserved);
        assert_eq!(granted.decode_vector_at(vector_id).unwrap(), expected);
    }

    #[test]
    fn allocation_failures_map_to_document_level_errors() {
        assert_eq!(
            VectorAccessError::from(VectorReadError::AllocationFailed),
            VectorAccessError::AllocationFailed
        );
        assert_eq!(
            vector_access_error_for_edit(VectorAccessError::AllocationFailed),
            EditError::AllocationFailed
        );
        assert_eq!(
            invalid_vector_read_error(VectorReadError::AllocationFailed),
            EditError::AllocationFailed
        );
        assert_eq!(
            vector_encode_error_for_edit(VectorEncodeError::AllocationFailed),
            EditError::AllocationFailed
        );
        assert_eq!(
            vector_encode_error_for_edit(VectorEncodeError::InvalidPayload(
                VectorReadError::AllocationFailed,
            )),
            EditError::AllocationFailed
        );
    }
}
