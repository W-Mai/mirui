# Clipboard

`commands.clipboard().copy(editor)` copies the selection belonging to an explicit entity. TextInput and custom widgets that attach TextEditing supply this capability automatically. Reversed UTF-8 selections are normalized; an empty selection leaves the platform clipboard unchanged. Copy does not change focus, selection, document content, or input-method composition.

```rust,ignore
Button () [PointerFocus::Preserve, Text::label("Copy")]
on Tap {
    commands!().clipboard().copy(editor);
}
```

Commands execute in order. `select_all(editor)` followed by `copy(editor)` copies the full selection; reversing these calls copies the selection that existed before select-all. The copy command captures immutable data before returning, so subsequent document edits cannot change the pending platform write.

## Completion

Listen for `mirui::ui::clipboard::ClipboardFinished` on the target editor, not the toolbar button. Its operation ID identifies one request. `result` distinguishes `ClipboardOutcome::Copied`, `ClipboardOutcome::EmptySelection`, and `ClipboardError`. Copied means the platform write acknowledged success; permission rejection, unsupported data, and platform failure are not success. Completion never retries a write. Duplicate or foreign-App acknowledgments are ignored.

```rust,ignore
TextInput () on ClipboardFinished(|event: event!(ClipboardFinished)| {
    model.clipboard_finished(event.result);
})
```

The normal bounded event preset supports clipboard completion notifications. Explicit budgets can reserve `CommandCapacity::clipboard(count)` and `EventCapacity::notifications::<ClipboardFinished>(count)`. Completion is not queued without a listener; failure is still logged.

## Custom selectable components

A component registers `InspectSelection` and `CopySelection` through the existing `#[event]` callbacks and View `input` list, or explicit `on` handlers. They are synchronous, single-provider requests, not bubbling gestures or queued notifications.

`InspectSelection` returns `Result<SelectionState, ClipboardError>`. Construct the state with `SelectionState::new(has_selection, context)`, where `context: Eq + 'static` owns a snapshot of source identity and content/selection revisions. Changes that are later reversed must still change the revisions. Do not store a live lookup closure. `CopySelection` returns `Result<ClipboardData, ClipboardError>` and must not modify the source. The framework compares selection context before and after export and separately validates the node, provider, component attachment, and retained-row identity. A rejected callback does not roll back arbitrary application mutations.

## Data and allocation

ClipboardData contains items; each ClipboardItem contains MIME representations described by ClipboardFormat. Static items, formats, MIME strings, and bytes can remain borrowed for their entire lifetime. Dynamic `with_capacity` builders reserve explicit item/format counts and impose a total byte limit. Appending does not expand these limits or truncate data. Duplicate MIME representations are rejected case-insensitively. `ClipboardData::text(text)` creates an independent UTF-8 snapshot.

Explicit copy operations may allocate snapshots, context values, and platform conversion buffers. They do not add allocations to ordinary per-frame drawing or Model/View updates. In-flight operations use storage prepared on the first copy, bounded by the App's configured command record count; one pending platform write counts toward this bound.

## Platforms

Web uses the asynchronous Clipboard API and reports its acknowledgment or rejection. SDL software and GPU surfaces use the SDL text clipboard. These bridges accept exactly one `text/plain` representation and explicitly reject other items or representations instead of silently discarding them; SDL also rejects embedded NUL bytes. Surfaces without a clipboard bridge return Unsupported.

A custom Surface implements `write_clipboard(ClipboardWrite)`. The request owns its immutable data. Return `ClipboardDispatch::Complete(request.complete(result))` for synchronous writes; for asynchronous writes, retain the request, return Pending, and enqueue `InputEvent::Clipboard(request.complete(result))` when finished. Every pending request must produce one completion, including failures. An App serializes platform writes and accepts completion while suspended.
