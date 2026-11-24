use crate::ecs::{Entity, World};
use crate::text::TextGeometry;
use crate::types::{Dimension, Fixed, Rect, Transform, Transform3D};
use crate::ui::geometry::widget_render_geometry;
use crate::ui::layout::LayoutNode;
use crate::ui::{self, Outputs, Style, Widget};

/// Stage at which a text widget failed to prepare its layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextLayoutStage {
    Measure,
    Layout,
}

/// Cause of a text preparation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextLayoutFailureKind {
    Text(crate::text::TextLayoutError),
    FontStackCapacity { required: usize, limit: usize },
    PathBaselineBudget { required: usize, budget: usize },
    PathBaselineAllocation,
    PathBaseline,
    MissingLayout,
    OutputPublication,
}

/// The text widget and operation that stopped a layout pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextLayoutFailure {
    pub entity: Entity,
    pub stage: TextLayoutStage,
    pub kind: TextLayoutFailureKind,
}

impl TextLayoutFailure {
    pub(crate) const fn text(
        entity: Entity,
        stage: TextLayoutStage,
        error: crate::text::TextLayoutError,
    ) -> Self {
        Self {
            entity,
            stage,
            kind: TextLayoutFailureKind::Text(error),
        }
    }

    pub(crate) const fn font_stack(
        entity: Entity,
        stage: TextLayoutStage,
        required: usize,
        limit: usize,
    ) -> Self {
        Self {
            entity,
            stage,
            kind: TextLayoutFailureKind::FontStackCapacity { required, limit },
        }
    }

    pub(super) const fn baseline(
        entity: Entity,
        stage: TextLayoutStage,
        error: crate::text::baseline::PathBaselineError,
    ) -> Self {
        use crate::text::baseline::PathBaselineError;

        let kind = match error {
            PathBaselineError::CacheBudget { required, budget } => {
                TextLayoutFailureKind::PathBaselineBudget { required, budget }
            }
            PathBaselineError::Allocation => TextLayoutFailureKind::PathBaselineAllocation,
            _ => TextLayoutFailureKind::PathBaseline,
        };
        Self {
            entity,
            stage,
            kind,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) enum TextInputReuse {
    #[default]
    Disabled,
    CleanNodes,
    UnchangedInputs,
}

impl TextInputReuse {
    pub(super) fn enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

struct LaidOutText {
    handle: crate::text::TextLayoutHandle,
    measure: crate::text::TextMeasure,
}

fn layout_text(
    world: &World,
    entity: Entity,
    width: Fixed,
    reuse_inputs: TextInputReuse,
) -> Result<Option<LaidOutText>, TextLayoutFailure> {
    use crate::render::font::ResolvedFontStack;
    use crate::ui::widgets::text::Text;

    let Some(text) = world.get::<Text>(entity) else {
        return Ok(None);
    };
    let Some(style) = world.get::<Style>(entity) else {
        return Ok(None);
    };
    let Some(resource) = world.resource::<Outputs<TextGeometry>>() else {
        return Ok(None);
    };
    let content_width = crate::ui::widgets::text::linear_text_content_rect(
        style,
        Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: width,
            h: Fixed::ZERO,
        },
    )
    .w;
    if reuse_inputs.enabled()
        && reuse_text_inputs(world, entity, reuse_inputs)
        && let Some(handle) = world.get::<crate::text::TextLayoutHandle>(entity).copied()
        && let Some(measure) = resource.storage_mut().layouts.reuse_layout(
            handle,
            text_layout_owner(entity),
            crate::types::fixed::to_textflow(content_width.max(Fixed::ZERO)),
        )
    {
        return Ok(Some(LaidOutText { handle, measure }));
    }
    #[cfg(test)]
    super::local_layout_tests::record_text_preparation(false);
    let face_limit = resource.storage().layouts.limits().fallback_faces;
    let typography = ui::Typography::inherited(world, entity, style);
    let Some(fonts) = ResolvedFontStack::resolve(
        world.resource::<crate::render::font::FontManager>(),
        typography.font_stack,
        typography.font_size,
        face_limit,
    )
    .map_err(|error| {
        TextLayoutFailure::font_stack(entity, TextLayoutStage::Layout, error.required, error.limit)
    })?
    else {
        return Ok(None);
    };
    let font = fonts.primary();
    let content = text.resolve(world.resource());
    let metrics = font.metrics(font.size);
    let text_path = world.get::<crate::text::TextPath>(entity).copied();
    let request = text.paragraph().layout_request(
        &content,
        metrics,
        text_path.is_none().then_some(content_width),
    );
    let language = text
        .paragraph()
        .language
        .as_ref()
        .map(crate::ui::widgets::LanguageTag::as_str);
    let font_fingerprint = fonts.layout_fingerprint(language, text.paragraph().shaping);
    let handle = match text_path {
        Some(path) => {
            let Some(paths) = world.resource::<crate::render::path::PathStore>() else {
                return Ok(None);
            };
            let mut storage = resource.storage_mut();
            let crate::text::geometry::TextGeometryStorage {
                layouts,
                paths: baselines,
            } = &mut *storage;
            let line_limit = request.max_lines.min(layouts.limits().lines);
            baselines
                .with_lines(
                    paths,
                    path,
                    crate::text::baseline::DEFAULT_TOLERANCE,
                    line_limit,
                    |widths| {
                        let mut request = request;
                        request.max_lines = request.max_lines.min(widths.line_count());
                        request.line_widths = Some(widths);
                        fonts.with_typefaces(language, text.paragraph().shaping, |typefaces| {
                            layouts.layout_cached(
                                text_layout_owner(entity),
                                font_fingerprint,
                                request,
                                typefaces,
                            )
                        })
                    },
                )
                .map_err(|error| {
                    TextLayoutFailure::baseline(entity, TextLayoutStage::Layout, error)
                })?
                .map_err(|error| TextLayoutFailure::text(entity, TextLayoutStage::Layout, error))?
        }
        None => fonts
            .with_typefaces(language, text.paragraph().shaping, |typefaces| {
                resource.storage_mut().layouts.layout_cached(
                    text_layout_owner(entity),
                    font_fingerprint,
                    request,
                    typefaces,
                )
            })
            .map_err(|error| TextLayoutFailure::text(entity, TextLayoutStage::Layout, error))?,
    };
    let measure = resource
        .storage()
        .layouts
        .get(handle)
        .map(|layout| layout.measure())
        .ok_or(TextLayoutFailure {
            entity,
            stage: TextLayoutStage::Layout,
            kind: TextLayoutFailureKind::MissingLayout,
        })?;
    Ok(Some(LaidOutText { handle, measure }))
}

pub(super) fn prepare_input_layout(
    world: &mut World,
    entity: Entity,
    rect: Rect,
) -> Result<bool, TextLayoutFailure> {
    use ui::widgets::text_editing::geometry::{TextEditingLayoutHandle, prepare_layout};

    match prepare_layout(world, entity, rect)? {
        Some(handle) => {
            if world
                .get::<TextEditingLayoutHandle>(entity)
                .map(|value| value.0)
                != Some(handle)
            {
                world.insert(entity, TextEditingLayoutHandle(handle));
                return Ok(true);
            }
        }
        None => {
            return Ok(world.remove::<TextEditingLayoutHandle>(entity).is_some());
        }
    }
    Ok(false)
}

pub(super) fn prepare_text_geometry(
    world: &mut World,
    entities: &[Entity],
) -> Result<bool, TextLayoutFailure> {
    let mut changed = false;
    if world.has_any_by_id(core::any::TypeId::of::<ui::widgets::TextEditing>()) {
        for &entity in entities {
            if !world.has::<ui::widgets::TextEditing>(entity)
                || crate::ui::branch::is_hidden_in_tree(world, entity)
            {
                continue;
            }
            let Some(rect) = world.get::<ui::ComputedRect>(entity).map(|rect| rect.0) else {
                continue;
            };
            changed |= prepare_input_layout(world, entity, rect)?;
        }
    }
    prepare_path_text_placements(world, entities)?;
    Ok(changed)
}

pub(super) fn publish_text_geometry(
    world: &World,
    entities: &[Entity],
) -> Result<(), TextLayoutFailure> {
    use crate::render::font::ResolvedFontStack;
    use crate::text::baseline::{DEFAULT_TOLERANCE, PlacementKey};
    use crate::text::geometry::TextGeometryRecord;
    use crate::text::{TextLayoutHandle, TextPath};
    use crate::ui::widgets::text::{Text, linear_text_content_rect, vertical_offset};

    let Some(outputs) = world.resource::<Outputs<TextGeometry>>() else {
        return Ok(());
    };
    if !outputs.has_requests() {
        return Ok(());
    }
    for &entity in entities {
        if !outputs.is_requested(entity) {
            continue;
        }
        let record = (|| {
            if crate::ui::branch::is_hidden_in_tree(world, entity) {
                return None;
            }
            let text = world.get::<Text>(entity)?;
            let style = world.get::<Style>(entity)?;
            let handle = world.get::<TextLayoutHandle>(entity).copied()?;
            let (rect, affine, projective) = widget_render_geometry(world, entity)?;
            let storage = outputs.storage();
            let layout = storage.layouts.get(handle)?;
            let typography = ui::Typography::inherited(world, entity, style);
            let fonts = ResolvedFontStack::resolve(
                world.resource::<crate::render::font::FontManager>(),
                typography.font_stack,
                typography.font_size,
                storage.layouts.limits().fallback_faces,
            )
            .ok()??;
            let metrics = fonts.primary().metrics(fonts.primary().size);
            let text_path = world.get::<TextPath>(entity).copied();
            let placement = match text_path {
                Some(path) => Some(PlacementKey::new(
                    entity,
                    handle,
                    path,
                    world
                        .resource::<crate::render::path::PathStore>()?
                        .revision(path.path())
                        .ok()?,
                    DEFAULT_TOLERANCE,
                )),
                None => None,
            };
            let local = Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: rect.w,
                h: rect.h,
            };
            let mut content_rect = if text_path.is_some() {
                local
            } else {
                linear_text_content_rect(style, local)
            };
            if text_path.is_none() {
                content_rect.y += vertical_offset(
                    text.paragraph().vertical_align,
                    content_rect.h,
                    crate::types::fixed::from_textflow(layout.measure().height),
                );
            }
            let transform = if projective.is_identity() {
                Transform3D::from(affine)
            } else {
                projective
            }
            .compose(&Transform3D::from(Transform::translate(rect.x, rect.y)));
            let caret_ready =
                placement.is_some_and(|key| storage.paths.caret_frames(key, |_| ()).is_some());
            Some(TextGeometryRecord {
                layout: handle,
                placement,
                metrics,
                content_rect,
                transform,
                text_path,
                caret_ready,
            })
        })();
        if let Some(record) = record {
            outputs
                .publish_text(entity, record)
                .map_err(|_| TextLayoutFailure {
                    entity,
                    stage: TextLayoutStage::Layout,
                    kind: TextLayoutFailureKind::OutputPublication,
                })?;
        } else {
            outputs.reset(entity);
        }
    }
    Ok(())
}

fn prepare_path_text_placements(
    world: &World,
    entities: &[Entity],
) -> Result<(), TextLayoutFailure> {
    use crate::text::baseline::{DEFAULT_TOLERANCE, PathBaselineError, PlacementKey};
    use crate::text::{TextLayoutHandle, TextPath};

    let Some(outputs) = world.resource::<Outputs<TextGeometry>>() else {
        return Ok(());
    };
    let mut storage = outputs.storage_mut();
    let crate::text::geometry::TextGeometryStorage {
        layouts,
        paths: baselines,
    } = &mut *storage;
    baselines.begin_placements();
    if !world.has_any_by_id(core::any::TypeId::of::<TextPath>()) {
        return Ok(());
    }
    let (Some(_fonts), Some(paths)) = (
        world.resource::<crate::render::font::FontManager>(),
        world.resource::<crate::render::path::PathStore>(),
    ) else {
        return Ok(());
    };
    let key_for = |entity| -> Result<Option<(PlacementKey, TextLayoutHandle)>, TextLayoutFailure> {
        if !world.has::<crate::ui::widgets::Text>(entity)
            || crate::ui::branch::is_hidden_in_tree(world, entity)
        {
            return Ok(None);
        }
        let (Some(path), Some(handle)) = (
            world.get::<TextPath>(entity).copied(),
            world.get::<TextLayoutHandle>(entity).copied(),
        ) else {
            return Ok(None);
        };
        let revision = paths.revision(path.path()).map_err(|error| {
            TextLayoutFailure::baseline(
                entity,
                TextLayoutStage::Layout,
                PathBaselineError::Store(error),
            )
        })?;
        Ok(Some((
            PlacementKey::new(entity, handle, path, revision, DEFAULT_TOLERANCE),
            handle,
        )))
    };
    for &entity in entities {
        if let Some((key, _)) = key_for(entity)? {
            baselines.pin_placement(key);
        }
    }
    for &entity in entities {
        let Some((key, handle)) = key_for(entity)? else {
            continue;
        };
        let layout = layouts.get(handle).ok_or(TextLayoutFailure {
            entity,
            stage: TextLayoutStage::Layout,
            kind: TextLayoutFailureKind::MissingLayout,
        })?;
        baselines
            .prepare_glyph_frames(paths, key, &layout)
            .map_err(|error| TextLayoutFailure::baseline(entity, TextLayoutStage::Layout, error))?;
        // Caret storage is optional; exhausted interaction geometry does not discard glyphs.
        let _ = baselines.prepare_caret_frames(paths, key, &layout);
    }
    Ok(())
}

pub(super) fn layout_text_tree(
    node: &mut LayoutNode,
    world: &mut World,
    entities: &[Entity],
    index: &mut usize,
    reuse_inputs: TextInputReuse,
) -> Result<bool, TextLayoutFailure> {
    if *index >= entities.len() {
        return Ok(false);
    }
    let entity = entities[*index];
    *index += 1;
    let mut intrinsic_changed = layout_text_node(node, world, entity, reuse_inputs)?;
    for child in &mut node.children {
        intrinsic_changed |= layout_text_tree(child, world, entities, index, reuse_inputs)?;
    }
    Ok(intrinsic_changed)
}

pub(super) fn layout_text_node(
    node: &mut LayoutNode,
    world: &mut World,
    entity: Entity,
    reuse_inputs: TextInputReuse,
) -> Result<bool, TextLayoutFailure> {
    #[cfg(test)]
    super::local_layout_tests::record_input_visit(true);
    use crate::types::fixed::from_textflow;

    let mut intrinsic_changed = false;
    if let Some(layout) = layout_text(world, entity, node.rect.w, reuse_inputs)? {
        let measured_width = from_textflow(layout.measure.width);
        let measured_height = from_textflow(layout.measure.height);
        let (width, height) = if world.get::<crate::text::TextPath>(entity).is_none() {
            world
                .get::<Style>(entity)
                .map_or((measured_width, measured_height), |style| {
                    crate::ui::widgets::text::padded_text_intrinsic_size(
                        style,
                        measured_width,
                        measured_height,
                    )
                })
        } else {
            (measured_width, measured_height)
        };
        intrinsic_changed =
            node.intrinsic_width != Some(width) || node.intrinsic_height != Some(height);
        node.set_intrinsic_size(width, height);
        if world.get::<crate::text::TextLayoutHandle>(entity) != Some(&layout.handle) {
            world.insert(entity, layout.handle);
        }
    }
    prepare_input_layout(world, entity, node.rect)?;
    Ok(intrinsic_changed)
}

pub(crate) fn apply_text_intrinsic(
    world: &World,
    entity: Entity,
    node: &mut LayoutNode,
) -> Result<(), TextLayoutFailure> {
    use crate::render::font::ResolvedFontStack;
    use crate::types::fixed::from_textflow;
    use crate::ui::widgets::text::Text;

    let Some(text) = world.get::<Text>(entity) else {
        return Ok(());
    };
    #[cfg(test)]
    super::local_layout_tests::record_text_preparation(true);
    let style = world.get::<Style>(entity).expect("text widget style");
    let Some(cache) = world.resource::<Outputs<TextGeometry>>() else {
        return Ok(());
    };
    let face_limit = cache.storage().layouts.limits().fallback_faces;
    let typography = ui::Typography::inherited(world, entity, style);
    let Some(fonts) = ResolvedFontStack::resolve(
        world.resource::<crate::render::font::FontManager>(),
        typography.font_stack,
        typography.font_size,
        face_limit,
    )
    .map_err(|error| {
        TextLayoutFailure::font_stack(
            entity,
            TextLayoutStage::Measure,
            error.required,
            error.limit,
        )
    })?
    else {
        return Ok(());
    };
    let font = fonts.primary();
    let content = text.resolve(world.resource());
    let metrics = font.metrics(font.size);
    let text_path = world.get::<crate::text::TextPath>(entity).copied();
    let request = text.paragraph().layout_request(&content, metrics, None);
    let language = text
        .paragraph()
        .language
        .as_ref()
        .map(crate::ui::widgets::LanguageTag::as_str);
    let font_fingerprint = fonts.layout_fingerprint(language, text.paragraph().shaping);
    let measure = match text_path {
        Some(path) => {
            let Some(paths) = world.resource::<crate::render::path::PathStore>() else {
                return Ok(());
            };
            let mut storage = cache.storage_mut();
            let crate::text::geometry::TextGeometryStorage {
                layouts,
                paths: baselines,
            } = &mut *storage;
            let line_limit = request.max_lines.min(layouts.limits().lines);
            baselines
                .with_lines(
                    paths,
                    path,
                    crate::text::baseline::DEFAULT_TOLERANCE,
                    line_limit,
                    |widths| {
                        let mut request = request;
                        request.max_lines = request.max_lines.min(widths.line_count());
                        request.line_widths = Some(widths);
                        fonts.with_typefaces(language, text.paragraph().shaping, |faces| {
                            layouts.measure_cached(
                                text_layout_owner(entity),
                                font_fingerprint,
                                request,
                                faces,
                            )
                        })
                    },
                )
                .map_err(|error| {
                    TextLayoutFailure::baseline(entity, TextLayoutStage::Measure, error)
                })?
                .map_err(|error| TextLayoutFailure::text(entity, TextLayoutStage::Measure, error))?
        }
        None => fonts
            .with_typefaces(language, text.paragraph().shaping, |faces| {
                cache.storage_mut().layouts.measure_cached(
                    text_layout_owner(entity),
                    font_fingerprint,
                    request,
                    faces,
                )
            })
            .map_err(|error| TextLayoutFailure::text(entity, TextLayoutStage::Measure, error))?,
    };
    let measured_width = from_textflow(measure.width);
    let measured_height = from_textflow(measure.height);
    let (width, height) = if text_path.is_none() {
        crate::ui::widgets::text::padded_text_intrinsic_size(style, measured_width, measured_height)
    } else {
        (measured_width, measured_height)
    };
    node.set_intrinsic_size(width, height);
    Ok(())
}

pub(super) fn text_layout_owner(entity: Entity) -> u64 {
    u64::from(entity.id) | (u64::from(entity.generation) << 32)
}

pub(super) fn reuse_text_inputs(world: &World, entity: Entity, reuse: TextInputReuse) -> bool {
    (matches!(reuse, TextInputReuse::UnchangedInputs) || !world.has::<ui::dirty::Dirty>(entity))
        && !world.has::<crate::text::TextPath>(entity)
        && world
            .get::<crate::ui::widgets::Text>(entity)
            .is_some_and(|text| !text.is_localized())
}

pub(super) fn reuse_text_intrinsic(
    world: &World,
    entity: Entity,
    style: &Style,
    node: &mut LayoutNode,
) -> bool {
    let Some(cache) = world.resource::<Outputs<TextGeometry>>() else {
        return false;
    };
    let Some(measure) = cache
        .storage_mut()
        .layouts
        .reuse_measurement(text_layout_owner(entity))
    else {
        return false;
    };
    let (width, height) = crate::ui::widgets::text::padded_text_intrinsic_size(
        style,
        crate::types::fixed::from_textflow(measure.width),
        crate::types::fixed::from_textflow(measure.height),
    );
    node.set_intrinsic_size(width, height);
    true
}

pub(super) fn visible_text_owner(world: &World, visible_entities: &[Entity], owner: u64) -> bool {
    let entity = Entity {
        id: owner as u32,
        generation: (owner >> 32) as u32,
    };
    visible_entities
        .binary_search_by_key(&(entity.id, entity.generation), |candidate| {
            (candidate.id, candidate.generation)
        })
        .is_ok()
        && world.get::<Widget>(entity).is_some()
        && world.get::<Style>(entity).is_some()
}

/// A single line input with no height of its own is as tall as its line and
/// the padding above and below it, three pixels more than a field's inner line
/// when that padding is left unwritten, so the line sits with room around it.
pub(super) fn apply_input_intrinsic(
    world: &World,
    entity: Entity,
    style: &Style,
    node: &mut LayoutNode,
) {
    use ui::widgets::text_editing::geometry::AUTO_HEIGHT_PADDING;

    if !world.has::<ui::widgets::TextInput>(entity) {
        return;
    }
    let typography = ui::Typography::inherited(world, entity, style);
    let Some(font) =
        crate::render::font::resolve_or_default(world, typography.font_stack.primary())
    else {
        return;
    };
    let ppem = font.ppem_for(typography.font_size);
    let rows = world
        .get::<ui::widgets::TextInput>(entity)
        .map_or(1, |input| i32::from(input.lines.max(1)));
    let line = font.metrics(ppem).line_height * rows;
    let pad = |side: Dimension| match side {
        Dimension::Px(value) => value.max(Fixed::ZERO),
        _ => Fixed::from_int(AUTO_HEIGHT_PADDING),
    };
    node.set_intrinsic_height(
        line + pad(style.layout.padding.top) + pad(style.layout.padding.bottom),
    );
}

pub(super) fn apply_static_glyph_intrinsic(
    world: &World,
    entity: Entity,
    style: &Style,
    node: &mut LayoutNode,
) {
    let Some(run) = world.get::<crate::ui::widgets::StaticGlyphRun>(entity) else {
        return;
    };
    let typography = ui::Typography::inherited(world, entity, style);
    let Some(font) =
        crate::render::font::resolve_or_default(world, typography.font_stack.primary())
    else {
        return;
    };
    let ppem = font.ppem_for(typography.font_size);
    let metrics = font.metrics(ppem);
    let (width, height) = run.measure(metrics.line_height);
    node.set_intrinsic_size(width, height);
}
