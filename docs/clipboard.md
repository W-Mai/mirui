# Clipboard

`commands.clipboard().copy(editor)` copies the selection belonging to an explicit entity. TextInput and custom widgets that attach TextEditing supply this capability automatically. Reversed UTF-8 selections are normalized; an empty selection leaves the platform clipboard unchanged. Copy does not change focus, selection, document content, or input-method composition.

```rust,ignore
Button () [PointerFocus::Preserve, Text::label("Copy")]
on Tap {
    commands!().clipboard().copy(editor);
}
```

Commands execute in order. `select_all(editor)` followed by `copy(editor)` copies the full selection; reversing these calls copies the selection that existed before select-all. The copy command captures immutable data before returning, so subsequent document edits cannot change the pending platform write.

`commands.clipboard().cut(editor)` captures the same selection and waits for a successful platform write before deleting it. Empty selections do nothing. Write failures preserve the document, selection, and preedit. If the source, selection, provider, or composition changes while the write is pending, the clipboard keeps the captured data but the editor is not modified. Changes that are later reversed still invalidate the pending cut. TextEditing collapses the caret to the deleted range's start; successful edits retire the old input-method session before changing the document and notify composition end afterward.

`commands.clipboard().paste(editor)` reads platform data and replaces the target editor's selection, or inserts at its caret when no text is selected. The caret moves to the inserted text's end. The command fixes its target and editing context before reading; changes to the source, selection, provider, or composition reject the pending operation instead of applying old offsets to new content. Capacity, format, and text validation failures preserve the document, selection, and preedit. Accepted edits use the same input-method interruption and atomic replacement as cutting.

Ctrl/Command+A selects all, C copies, X cuts, and V pastes into the focused component. These defaults run after Key handlers allow propagation; a consumed Key prevents the default operation. The target and capability providers are fixed before callbacks, so changing focus cannot redirect the operation. Removed, hidden, replaced, or rebound targets are rejected. Key releases, auto-repeat, and Alt/Shift combinations do not run these defaults. Copy during composition reads the formal document selection without committing or cancelling preedit; cut and paste interrupt composition only after accepting the edit.

Software input methods can send the same keys through `commands.text_input().key(session, key)`. They use the session's editor and the same selection defaults without reentering the input method's own Key interceptor.

## Completion

Listen for `mirui::ui::clipboard::ClipboardFinished` on the target editor, not the toolbar button. Its operation ID identifies one request. `result` distinguishes `Copied`, `Cut`, `CopiedWithoutCut`, `Pasted`, `EmptySelection`, and `ClipboardError`. Copied means the platform write acknowledged success; Cut also confirms that the component accepted deletion. CopiedWithoutCut means the write succeeded but the source became unavailable or refused deletion. Pasted means the component accepted the imported data. Permission rejection, unsupported data, and platform failure are not success. Completion never retries a write or edit. Duplicate or foreign-App acknowledgments are ignored.

```rust,ignore
TextInput () on ClipboardFinished(|event: event!(ClipboardFinished)| {
    model.clipboard_finished(event.result);
})
```

The normal bounded event preset supports clipboard completion notifications. Explicit budgets can reserve `CommandCapacity::clipboard(count)` and `EventCapacity::notifications::<ClipboardFinished>(count)`. Completion is not queued without a listener; failure is still logged.

Completion belongs to the entity and retained item binding that issued the operation. Deleted targets and reused cells do not receive the previous item's result. A captured copy can still finish its platform write after the original item disappears; pending cut and paste operations cannot edit the replacement item.

## Custom selectable components

A component registers `InspectSelection` and `CopySelection` through the existing `#[event]` callbacks and View `input` list, or explicit `on` handlers. They are synchronous, single-provider requests, not bubbling gestures or queued notifications.

`InspectSelection` returns `Result<SelectionState, ClipboardError>`. Construct the state with `SelectionState::new(has_selection, context)`, where `context: Eq + 'static` owns a snapshot of source identity and content/selection revisions. Changes that are later reversed must still change the revisions. Do not store a live lookup closure. `CopySelection` returns `Result<ClipboardData, ClipboardError>` and must not modify the source. The framework compares selection context before and after export and separately validates the node, provider, component attachment, and retained-row identity. A rejected callback does not roll back arbitrary application mutations.

Components supporting deletion also register `CutSelection`, returning `Result<(), ClipboardError>`. The clipboard service calls it only after a successful write and an unchanged selection context. Include every mutable editing context in InspectSelection, such as composition state, not just content length or the currently selected value. Validate the entire operation before mutating the source. Deferred-command rejection propagates as a request error even if the callback body returned Ok. Arbitrary user mutations are not rolled back; built-in text editing logs notification-capacity failures without retrying an already committed edit.

Components accepting imported data register `PasteSelection`, returning `Result<(), ClipboardError>`. Read the immutable payload through `event.data()`. The event owns the platform result; the callback and its deferred commands borrow it until execution finishes. TextEditing accepts one item and selects its `text/plain` representation, even when the same item also offers HTML or another format. It rejects multiple items instead of silently choosing one. Custom components define their own representation and item rules.

Pixel Loom registers these requests on PixelSurface. Its selection is the editing frame, independent of the animated preview. The toolbar copies, cuts, and pastes a 12×12 indexed frame using `application/x-mirui-pixel-frame`, including dimensions and palette data. Import validates the entire snapshot before changing pixels and records one undo step. Switching frames, editing, undoing, or replacing the component invalidates pending edits; preview playback does not. Binary transfer requires a platform bridge supporting the custom format, such as Web custom clipboard formats.

## Data and allocation

ClipboardData contains items; each ClipboardItem contains MIME representations described by ClipboardFormat. Static items, formats, MIME strings, and bytes can remain borrowed for their entire lifetime. Dynamic `with_capacity` builders reserve explicit item/format counts and impose a total byte limit. Appending does not expand these limits or truncate data. Duplicate MIME representations are rejected case-insensitively. `ClipboardData::text(text)` creates an independent UTF-8 snapshot.

Explicit clipboard operations may allocate snapshots, context values, and platform conversion buffers. They do not add allocations to ordinary per-frame drawing or Model/View updates. In-flight operations use storage prepared on the first clipboard command, bounded by the App's configured command record count; one pending platform read or write counts toward this bound. Platform reads reserve storage for the returned items, representations, and bytes. This is not a fixed upper bound on platform payload size; allocation failures return Capacity. No payload is copied merely to deliver its callback.

## Platforms

Web uses the asynchronous Clipboard API and reports its acknowledgment or rejection. It writes one item with all its MIME representations; multiple-item writes return Unsupported instead of discarding items. Plain text, HTML, and PNG use standard browser formats. Other MIME types use Web custom formats when supported by the browser, with the platform prefix removed on import. Reads retain all returned items and representations for the target component to inspect. Browser clipboard access remains subject to user activation, format support, and platform permissions.

On macOS 11 or newer, SDL software, SDL GPU, and WGPU surfaces share the native pasteboard bridge. It preserves multiple items and MIME representations, including plain text, HTML, RTF, PNG, file/URL items, and custom binary data. System type identifiers are translated through UniformTypeIdentifiers. TIFF imports retain their original representation and provide PNG. The system may supply additional representations, such as plain text for RTF. A changed pasteboard generation rejects the read instead of mixing snapshots.

macOS native file transfer uses one URI per item. Several files are several ClipboardItems, each with a `text/uri-list` representation. A single item containing multiple URIs returns Unsupported before writing; the bridge does not silently change item grouping. Invalid UTF-8, invalid PNG, relative URIs, and conflicting native type mappings are rejected before clearing the pasteboard. A failure during the native write preserves the source of a cut but cannot guarantee rollback of the system clipboard.

macOS pasteboard operations run on the window's main thread and return synchronous Surface completions; calls from another thread return Unavailable. No worker, clipboard watcher, or additional per-surface queue is installed. Native calls may wait for another application's data provider; synchronous completion does not imply bounded latency. Snapshot and image conversion storage is allocated only for explicit clipboard operations.

On Windows, SDL software, SDL GPU, and WGPU share the native clipboard bridge. One item can contain multiple representations: Unicode text, HTML fragments, RTF, PNG, file URI lists, and registered MIME data. Multiple items return Unsupported before writing. PNG also supplies a native BGRA bitmap; native bitmaps are imported as PNG through Windows Imaging Component. File URI lists also supply native file paths without opening the files. Non-file URIs retain their MIME representation. Alpha or color profiles that the system codec cannot preserve return an error rather than being silently discarded.

Windows writes prepare formats and storage before clearing the clipboard, using the surface's own window handle. Reads copy a single open-clipboard snapshot and close it before image conversion. An occupied clipboard returns Unavailable without a retry loop. Explicit operations allocate conversion storage; no worker or watcher is installed. Synchronous native calls may wait for another application's data provider. Native write or close failures reject the operation, so a cut retains its source; system contents are not transactionally rolled back.

Windows registered opaque formats use the system's global-memory block size. Writes reject empty opaque payloads or allocations whose size differs from the payload, before changing clipboard contents. Reads retain the entire native block and never guess a shorter length by trimming zero bytes. Self-delimiting native formats, including Unicode text, HTML, PNG, RTF, and file lists, exclude allocation padding according to their format boundaries. Embedded NUL is rejected in Unicode text; it remains valid in opaque binary data.

On other platforms, SDL software and GPU surfaces read and write plain text through the SDL clipboard. Writes accept exactly one `text/plain` representation and explicitly reject other items or representations instead of silently discarding them; SDL also rejects embedded NUL bytes. WGPU surfaces outside macOS and Windows, and other surfaces without a clipboard bridge, return Unsupported.

Web native Copy, Cut, and Paste events use the focused component's same capabilities without synthesizing keys. Copy and cut require an acknowledged asynchronous write. Native paste owns the event's string representations and File objects before the event expires; it does not read the system clipboard again or insert hidden-textarea content. Events belonging to another DOM control are left alone.

String-only native paste remains synchronous. File payloads use the existing ordered transfer queue and freeze the target selection at input delivery, before any file read. All files must finish reading before the component receives the payload. A changed selection rejects the edit; a failed file read does not paste the remaining files or text. Closing or discarding an unhandled paste releases its pending source.

The DOM event does not expose ClipboardItem grouping: its string representations form one item, and each File forms a separate item containing the file bytes and MIME type. An absent MIME type uses `application/octet-stream`. Equal filenames do not merge items. File paths and filenames are not converted into text or loaded as external URLs. An inaccessible file rejects the operation rather than silently dropping it. The asynchronous Clipboard API retains the item grouping supplied by the browser.

A custom Surface implements `write_clipboard(ClipboardWrite)` and `read_clipboard(ClipboardRead)`. The write request owns its immutable data; a read completion takes ownership of `Result<ClipboardData, ClipboardError>`. Return `ClipboardDispatch::Complete(request.complete(result))` for synchronous operations; for asynchronous operations, retain the request, return Pending, and enqueue `InputEvent::Clipboard(request.complete(result))` when finished. Every pending request must produce one completion, including failures. An App serializes platform reads and writes together and accepts completion while suspended.

For a native menu action, enqueue `InputEvent::ClipboardInput(ClipboardInput::copy())`, `ClipboardInput::cut()`, or `ClipboardInput::paste(data)`. Paste takes `Result<ClipboardData, ClipboardError>` and owns the data before the native event expires. These actions target the focus at their input-delivery position, use the same component validation and ClipboardFinished notification, and do not deliver a fabricated Key event. Paste supplied this way is synchronous and does not wait for unrelated platform transfers; changing the selection can invalidate an earlier pending cut or paste.

## Native contract checks

The macOS format checks run on the process main thread and use isolated named pasteboards, not the general clipboard. Run them explicitly in a desktop session; ordinary test runs skip this platform check.

```bash
cargo test --test native_clipboard --features sdl -- --ignored
```

On Windows the same command checks WIC image conversion and file URI mapping without opening the system clipboard. System-exchange checks require an additional explicit flag. The following command **replaces the Windows clipboard contents** and leaves a test message there; it does not restore the previous contents. Run it only in a disposable desktop session.

```bash
cargo test --test native_clipboard --features wgpu -- --ignored --replace-system-clipboard
```
