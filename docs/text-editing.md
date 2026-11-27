# Composing text editors

TextEditing supplies editor-local selection, copying, acknowledged cutting, and selection-aware pasting through the [clipboard commands](clipboard.md). Shared documents retain independent selections in each editor.

`TextDocument` stores the UTF-8 content and its byte capacity. `TextEditing` binds that document to one editor's selection, focus, and input handling. `TextInput` draws the editor. Custom components can combine these capabilities on the same entity without accessing World or implementing their own keyboard handling.

<div class="arch-diagram" role="img" aria-label="Text editor composition from shared TextDocument through TextEditing and TextInput to native or software IME sessions">
  <svg viewBox="0 0 940 168" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="edit-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="20" width="208" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="30" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">01 · DOCUMENT MODEL</text>
    <text x="30" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">Shared UTF-8 Storage</text>
    <text x="30" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">TextDocument · revision()</text>
    <text x="30" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">set_capacity · replace</text>
    <text x="30" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">Independent of editor view</text>
    <path d="M224 84H244" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#edit-arrow)" />
    <rect x="250" y="20" width="208" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="264" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">02 · EDITING STATE</text>
    <text x="264" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">Selection &amp; Input</text>
    <text x="264" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">TextEditing · TextSelection</text>
    <text x="264" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">Changed · Submitted</text>
    <text x="264" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">Per-editor caret &amp; preedit</text>
    <path d="M458 84H478" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#edit-arrow)" />
    <rect x="484" y="20" width="208" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="498" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">03 · APPEARANCE</text>
    <text x="498" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">Visual Runs &amp; Caret</text>
    <text x="498" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">TextInput · Placeholder</text>
    <text x="498" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">attach = callback()</text>
    <text x="498" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">No duplicate text copy</text>
    <path d="M692 84H712" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#edit-arrow)" />
    <rect x="718" y="20" width="206" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="732" y="46" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">04 · IME BRIDGE</text>
    <text x="732" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">Session &amp; Keys</text>
    <text x="732" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">TextInputSession</text>
    <text x="732" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">preedit · commit</text>
    <text x="732" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">Native / Software</text>
  </svg>
</div>

Dragging from an editor fixes the selection anchor at the starting caret and extends the other endpoint. Release or cancellation retains the selected range. Dragging stops when the editor is no longer focused or its content or selection has changed externally. Editors install `TouchAction::PanY` unless an explicit policy is present, keeping horizontal drags available for selection and vertical drags available for ancestor scrolling.

Use `commands.selection().select_all(editor)` to select an editor's complete document without changing keyboard focus. `editor` is the target entity; `TextEditing` supplies its own selection behavior, including for custom editors that attach it. Other editors sharing the document retain their independent selections. Toolbars can attach `PointerFocus::Preserve` to keep the editor focused. Selection changes use the existing `SelectionChanged` notification.

## A custom search editor

This example owns the query in an application Model. `SearchEditor()` attaches the editing capability and standard appearance; a second `TextInput()` shares the query while keeping its own selection. Return invokes the application's submit method through the existing notification API.

<MiruiSnippet recipe="editor-shared" label="TextDocument & SearchEditor" file="recipes.rs" height="210px">

<template #exercise>

Type into either editor (or click `Set 'SPI'`): both editors share the same `TextDocument` while keeping independent carets and selections. Press Return or click `Submit` to increment the submission counter.

</template>

</MiruiSnippet>

Registering a View installs its behavior; inserting a Component supplies per-node state. These are separate operations. `App::with_default_widgets()` already registers TextInput and TextEditing, so an application using that preset only adds its custom View. The empty `search_view` body declares no paint hook. Its attachment preserves explicitly supplied editing and appearance components instead of overwriting them.

`SearchEditor()` without a document creates a local document with 32 inline bytes. For a larger local buffer, attach `TextEditing::default().with_capacity(256)?` explicitly when composing the node. For a shared document, prepare capacity on the registered document before composing its editors. Capacity is a UTF-8 byte limit, not a character count; rejected edits leave the content unchanged. Capacity preparation may allocate, while replacements within prepared storage reuse that storage.

Programmatic changes go through `form.query()` and publish to its observers. They do not synthesize `Changed` or `Submitted` input notifications. Each editor keeps its own UTF-8 selection; external changes clamp invalid positions rather than moving them relative to an edit history. To derive application values from the query, observe its `revision()` and borrow the content with `read`.

The `document` field supplies the binding when the custom component is attached. Replacing that field alone is not a request to rebind an already attached TextEditing; replace the TextEditing component through `commands.insert` when changing an existing editor's source.

## Software input methods

`InputMethod::software` installs a `TextInputChanged` callback and an optional `Key` callback once per App. `TextInputChanged::state` contains the focused editor's session and caret geometry when available. A changed session invalidates old candidates; changes to caret geometry alone do not. `None` means the provider has no active editor.

```rust
use mirui::prelude::*;
use mirui::ui::widgets::{Button, Text, TextInput};

#[model]
#[derive(Default)]
struct Keyboard { session: Option<TextInputSession> }

#[model]
impl Keyboard {
    fn session(&self) -> Option<TextInputSession> { self.session }
    fn set_session(&mut self, session: Option<TextInputSession>) { self.session = session; }
}

#[event]
fn context(event: event!(TextInputChanged), keyboard: model!(Keyboard)) {
    keyboard.set_session(event.state.map(|state| state.session));
}

#[compose]
fn keyboard_panel(keyboard: model!(Keyboard)) {
    ui! {
        Column(row_gap: 8) {
            TextInput(width: 240, height: 36, capacity: 128, preedit_capacity: 256)
            Row(height: 36, column_gap: 8) [PointerFocus::Preserve] {
                Button(width: 80) [Text::label("ni")]
                    on Tap {
                        if let Some(session) = keyboard.session() {
                            commands!().text_input().preedit(session, "ni", Some(2..2));
                        }
                    }
                Button(width: 80) [Text::label("你")]
                    on Tap {
                        if let Some(session) = keyboard.session() {
                            commands!().text_input().commit(session, "你");
                        }
                    }
            }
        }
    };
}

# fn main() {
let mut app = App::headless(320, 160);
app.with_default_widgets();
let keyboard = app.add_model(Keyboard::default()).finish();
app.with_input_method(InputMethod::software((context::callback(keyboard.clone()),)));
let root = app.spawn_root().id();
app.compose(root, keyboard_panel(keyboard));
# }
```

Select the editor, then use the keyboard buttons. Preedit replaces the selected range only in that editor's display; other editors sharing the document retain its committed content. `commit` sends `TextCommitted` through the original editor's normal interception and editing route. `cancel` clears the preview without editing the document. `key` sends an editing key to the editor without returning it to the software provider.

`commands.text_input().set_mode(TextInputMode::Native)` selects platform text input; `Software` selects the installed software provider. Switching modes cancels the current composition and invalidates its session. Software mode ignores platform-committed text. The optional provider `Key` callback handles supported editing keys before the editor; returning `false` leaves normal editor handling enabled. This callback does not supply raw character translation or keyboard-layout mapping.

`capacity` limits committed UTF-8 bytes. `preedit_capacity` limits the uncommitted fragment and defaults to twice `capacity`; explicit values are independent. Display storage is prepared at attachment or explicit capacity changes and reused for updates. Oversized previews are cleared, not truncated, and a later valid preview can display normally. Static fragments are borrowed; an existing `String` can be moved into a request. Dynamically constructing text remains the input source's responsibility.

Requests are delivered after callback borrows are released. The default event preset includes text-input commands; explicit configurations use `CommandCapacity::text_input(count)` and a nonzero flush limit. The software request queue is prepared from the configured command record count. Invalid sessions are discarded; capacity exhaustion logs a warning and drops the new request without replay or rollback.

See [Widget input and notifications](widget-events.md) for notification ordering. Gallery's [Editor Lab](../src/gallery/demos/editor_lab) includes search filtering, command submission, shared-document editing, and a software keyboard with a static n → ni → 你 sequence.

Run Editor Lab with SDL or WGPU, then select **System** to use the operating system's input method:

```bash
cargo run -p gallery --example editor_lab_demo --no-default-features --features sdl
cargo run -p gallery --example editor_lab_demo --no-default-features --features wgpu
```

## Native input backends

App calls `Surface::set_text_input(Option<TextInputState>)` before polling input and after layout and rendering. `Some` selects a native editor session and its logical surface-space caret rectangle; `None` stops native text input. Moving the caret does not replace the session. Surfaces without a text-input service can use the default no-op implementation.

A backend captures `state.session` with its native input and returns `session.preedit(text, selection)`, `session.commit(text)`, or `session.cancel()` from `poll_event`. Preedit selections use UTF-8 byte offsets. These constructors move owned strings or borrow static strings; dispatch borrows the payload. Retired sessions are discarded rather than reassigned to the currently focused editor.

SDL software and GPU surfaces share this protocol. Session changes clear platform composition and queued text events before restarting the native input method. Caret-only updates do not restart it. Empty previews remain part of the composition until a queued commit arrives or the SDL queue becomes idle. SDL does not attach an editor identity to later OS callbacks; the adapter relies on platform cancellation for input not yet captured. The SDL binding exposes fixed-size native text events, not extended preedit events.

WGPU surfaces capture the editor session when Winit enables composition. Preedit, commit and disable events retain that session; changing focus does not retag captured input. Ordinary keyboard text also carries its capture-time session. Empty preedit is cancelled at the end of the native event batch unless a commit follows in that batch. `MobileSurface::finish_input_batch` connects this boundary to the platform loop.

Winit 0.30 provides desktop preedit and caret positioning. Its iOS and Android implementations expose keyboard text and soft-keyboard visibility, but do not deliver preedit events or support the caret-area API. Native text composition on these mobile backends is therefore limited by the platform adapter. Application-owned software input methods use the same editor sessions on all three paths.

Web Canvas uses a transparent DOM textarea for each native editor session. Composition input updates the preview; composition end commits or cancels it. Ordinary DOM input commits text, while keyboard events carry editing keys rather than duplicate characters. DOM UTF-16 selection offsets are converted to UTF-8 boundaries. Replacing a session removes its textarea and listeners; captured events retain their original session.

The web runner delivers input during browser event bubbling and renders on animation frames. Canvas and its textarea share host focus; switching between them does not cancel the editor. The textarea is positioned at the logical caret through the canvas CSS bounds. Browser activation policies still govern software-keyboard display. CSS rotation and skew are not represented by this axis-aligned candidate anchor.

The DOM textarea transports new text, not a copy of the document. Native surrounding-text prediction, document-wide DOM selection and rich clipboard operations are not provided by this transport. Creating a session allocates its DOM node and listeners; browser string conversion and CSS caret updates may allocate. Model/View updates and editor buffer reuse retain their existing memory contracts.
