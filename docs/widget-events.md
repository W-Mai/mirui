# Widget initialization, input, and notifications

## Input delivery

Native and simulated input use the same delivery order. Recognized gestures, their deferred commands, business notifications, and reactive updates complete before the next raw input is handled. A Tap followed by a Key in one input batch therefore delivers Tap first. Callback changes that hide, remove, or rebind a node affect subsequent inputs. Systems and painting retain their frame schedule.

Spatial input refreshes stale layout or transformed hit geometry before dispatch. Unchanged geometry is reused; pending paint damage remains for the frame renderer. A layout failure skips spatial dispatch and cancels active input instead of hitting stale coordinates.

`Key` carries `code`, `pressed`, `modifiers: Option<KeyModifiers>`, and `repeat: Option<bool>`. Modifiers expose `shift`, `control`, `alt`, and `meta`. `Some(false)` identifies a non-repeated key event; `None` means the platform did not provide that information. SDL and Web supply both snapshots; WGPU modifiers remain unknown until a native modifier update and are cleared on focus loss or suspension. Linux uses the keyboard device state and native repeat flag. NuttX keyboard records do not provide modifiers or repeat metadata. Simulated input supplies the fields explicitly.

## Text editing

`TextCommitted` delivers one complete UTF-8 fragment through the focused node, its ancestors, and App fallback. Read it with `event.text()`. Inline `on TextCommitted`, closures, and `#[event]` callbacks share this route. Returning `false` lets the default TextInput handler apply the fragment; consuming it prevents default editing.

Construct simulated or custom input with `TextCommitted::new("你好").into()` or move an existing `String` into `TextCommitted::new(text)`. Static strings are borrowed and owned strings are moved. Callback execution and deferred editing borrow the event until the current invocation completes; retaining business data requires its own storage. Cloning owned event text copies it. Platform text conversion and input-queue growth are separate from prepared callback and editing storage.

SDL preserves each native text event as one fragment; WGPU preserves each text or IME commit fragment rather than splitting it into characters. Native fragments are not guaranteed to be entire composition sessions or grapheme clusters. The Web keyboard source ignores composing key events; composition lifecycle and candidate-window handling are not supplied by that keyboard source.

`TextInput` keeps its UTF-8 storage private. Read content with `as_str()`, `len()`, and `is_empty()`, and read the caret with `cursor()`. Lengths and positions are byte offsets. `set_selection(TextSelection { anchor, cursor })` validates both endpoints before changing either; reversed selections preserve their direction.

`replace_selection(text)` replaces the entire selection and collapses it to the end of the inserted text. Capacity errors and control characters reject the whole replacement without changing content or selection. A successful result is `true` when content changed and `false` when only the selection collapsed. Editing does not allocate.

The default capacity is 32 inline bytes. Use `TextInput(capacity: 256)` in `ui!`, `TextInput::new().with_capacity(256)?`, or `TextInput::build().capacity(256)?` to prepare a larger buffer. `capacity()` reports the accepted byte limit. `set_capacity(bytes)` explicitly changes that limit, preserving content and selection; shrinking below the current length fails. Shrinking retains storage for reuse. Zero capacity accepts only empty content, and capacities above `min(u32::MAX, isize::MAX)` are rejected. Allocation failure leaves the existing input unchanged.

```rust,ignore
use mirui::ui::widgets::text_input::{TextInput, TextSelection};

let mut input = TextInput::new().with_capacity(256)?;
input.replace_selection("Hello world")?;
input.set_selection(TextSelection { anchor: 6, cursor: 11 })?;
input.replace_selection("Mirui")?;
assert_eq!(input.as_str(), "Hello Mirui");
```

Committed text replaces the current selection as one operation. An accepted fragment produces at most one `TextInput::Changed { len: u32 }`; invalid or oversized fragments are rejected in full with a warning. Backspace and Delete remove a nonempty selection first; otherwise they remove one Unicode scalar. Left and Right collapse a selection to its beginning or end, and otherwise move by one scalar, not a full grapheme cluster. `Changed` reports content accepted by input handlers, not caret-only changes. Calling component methods directly updates component state without synthesizing input notifications.

`TextInput::SelectionChanged { anchor: u32, cursor: u32 }` reports the resulting directed UTF-8 byte offsets when an input handler changes the selection. An edit that changes both content and selection reports `Changed` first. Replacing selected text with identical text can collapse the selection without reporting a content change. Rejected and unchanged operations do not notify.

`TextInput::Submitted` reports a Return press accepted by the default key handler, including repeated presses. It does not insert a newline, clear content, move the selection, or change focus. An authored key handler can consume Return before the default handler. Both notifications support inline bodies, closures, typed callbacks, and widget builders. Notifications without listeners are skipped; queue failure does not roll back an edit or replay it later.

## Pointer identity

Raw `InputEvent` pointer records carry `kind: PointerKind` (`Mouse`, `Touch`, `Pen`, or `Unknown`) and `id`. Gesture and scroll capture match both fields; movement or release from another pointer cannot end that capture. Backends report the device category rather than inferring it from the slot number. Simulated cursor actions use `Mouse`; simulated pinch and rotation contacts use `Touch`.

`PointerEnter` and `PointerLeave` report surface boundaries without inventing coordinates. Entering waits for a position-bearing input before hit testing. Leaving clears hover and cursor feedback without releasing an active drag; captured movement still uses its actual coordinates. Touch does not produce hover feedback, and pen hover requires an explicit enter or non-contact movement.

Web and WGPU map active native pointers to stable slots: mouse uses 0, with up to 16 other pointers occupying separate slots. A full map ignores new contacts without replacing active ones. Touch slots retire on release or cancellation; Web hovering pointers retain their slots until leaving without an active press. Host focus/visibility loss clears the mapping. Web lost pointer capture cancels an active press; a capture-loss event following normal release does not cancel it again. Pointer slot numbers are not persistent device identifiers.

## Node hover

`HoverChanged { hovered, x, y }` reports whether a mouse or hovering pen's hit path includes the node. It supports inline bodies, closures, and `#[event]` callbacks:

```rust,ignore
#[event]
fn hovered(event: event!(HoverChanged), state: model!(CardState)) {
    state.set_hovered(event.hovered);
}

Column() {
    Button("Open") on Tap { open(); }
} on HoverChanged(hovered::callback(state))
```

Subscribing does not make an otherwise non-interactive node hit-testable or add visual feedback. Containers observe their existing interactive descendants. Touch and unknown pointer sources do not hover. Multiple hovering pointers share membership; a node leaves only when none remain. Pressing does not clear hover, and leaving the surface does not end drag capture.

Leaves are delivered from child to parent, followed by entries from parent to child. Shared ancestors do not toggle when moving between children. Coordinates are logical surface coordinates, not local node coordinates. Hidden, disabled, removed, and rebound nodes lose eligibility; published layout changes update stationary hover. Registering a listener does not replay prior transitions. Notifications share `EventCapacity`; failed enqueue attempts are logged and discarded without retry.

## Press lifecycle

`Pressed { pointer_id, x, y }` starts a single-pointer press. `Released` carries the same fields for a normal release, including releases outside the original bounds; it does not imply `Tap`. `PressCancelled` adds `reason: PressCancelReason`: `Scroll`, `Gesture`, `Platform`, `Suspended`, or `Unavailable`.

```rust,ignore
Button("Open")
    on Pressed { state.set_pressed(true); }
    on Released { state.set_pressed(false); }
    on PressCancelled { state.set_pressed(false); }
```

All three notifications also accept closures and `#[event]` callbacks. They do not consume input, add hit targets, or enable visual feedback. Coordinates are logical surface coordinates; `pointer_id` is a backend contact slot, not a persistent device identifier. Capture internally distinguishes pointer kinds as well as slots.

The hit node and its listening ancestors are captured at the initial down and notified from child to parent. Moving, leaving the surface, reparenting, or adding a listener does not redirect that path. Long press retains capture. Scroll takeover, drag or multi-pointer recognition, platform cancellation, suspension, and target invalidation cancel it instead of releasing it. Removed or rebound receivers do not receive notifications for their old identities. Each press has at most one termination; a full notification queue discards delivery without retrying or changing recognition.

## Scroll geometry

`mirui::input::event::scroll::ScrollBounds` is a layout-produced component with `viewport_width`, `viewport_height`, `content_width`, `content_height`, `max_x`, and `max_y`. Read it through `query!(ScrollBounds)` in events, systems, or views. It is absent before the first successful layout; configuration edits take effect in this snapshot after layout succeeds. A failed layout leaves the previous value available.

```rust,ignore
#[event]
fn inspect_scroll(node: entity!(), bounds: query!(ScrollBounds), state: model!(ScrollReadout)) {
    if let Some(bounds) = bounds.get(node) {
        state.set_max_offset(bounds.max_y);
    }
}
```

Configure scrollable content through `ScrollConfig`, not `ScrollBounds`. Retained descendants, clipping, and explicit extents determine the range; LazyList uses the entire logical item extent rather than the visible cell pool. A successful layout removes published bounds for nodes no longer in its tree or without `ScrollOffset`.

## Scroll notifications

`ScrollStarted { offset }` marks the first actual displacement. `ScrollChanged { old, new }` reports content offsets, including fractional and elastic positions. `ScrollEnded { offset, reason }` follows completion of input, inertia, and rebound; `reason` is `ScrollEndReason::Settled` or `Cancelled`. Releasing a pointer does not end a segment while its content still moves.

```rust,ignore
#[event]
fn track_scroll(change: event!(ScrollChanged), readout: model!(ScrollReadout)) {
    readout.set_position(change.new.y);
}

#[compose]
fn panel(readout: model!(ScrollReadout)) {
    ui! {
        Column(height: 240, clip_children: true) [
            ScrollOffset::default(), ScrollConfig::default(),
        ] on ScrollChanged(track_scroll::callback(readout)) {
            Text("Content", height: 480)
        }
    };
}
```

Inline bodies and closures use the same notifications. Nodes must have `ScrollOffset`; subscriptions do not add hit targets, input consumption, or visual feedback. Wheel, pointer, rotary, and programmatic writes share observation after callback completion. A discrete write without an active motion produces `Started`, `Changed`, then `Ended`; writes restored to their original offset within the same callback produce no notification.

Switching scroll targets or cancelling input ends the old segment with `Cancelled`. Removed, hidden, detached, or rebound sources discard their old observation. New listeners do not replay earlier movement, and a full notification budget drops delivery without retrying or rolling back the scroll position.

## Node focus

`FocusChanged { focused, related }` is a direct node notification. `related` identifies the other node in the transition and may no longer exist when read. Subscribe with an inline body, closure, or `#[event]` callback:

```rust,ignore
#[event]
fn focus_changed(event: event!(FocusChanged), state: model!(EditorState)) {
    state.set_active(event.focused);
}

TextInput() on FocusChanged(focus_changed::callback(state))
```

`FocusState.focused` remains the focus resource. Writes within one callback publish the final value; separate Event, System, and Timer callbacks preserve separate transitions. The previous node loses focus before the new node gains it. Tap selects focus and delivers its notifications before authored Tap handlers. Changes made by focus listeners are queued, not delivered recursively. Hidden, disabled, removed, or rebound targets lose eligibility; notifications for destroyed or retired identities are discarded. Registering a listener does not replay existing focus or add pointer feedback. Host focus remains independent.

Focus notifications share `EventCapacity` storage and its flush limit. Failed enqueue attempts are logged and discarded without rolling back focus or retrying the notification.

## Surface changes

Register `ViewportChanged` and `SafeAreaChanged` through `App::on_event`. Both carry `old` and `new` values. `ViewportChanged` includes changes to physical size or scale even when logical dimensions stay the same; safe-area insets are in logical pixels.

```rust,ignore
app.on_event(ViewportChanged::EVENT, resized::callback());
```

Surface resources are updated before these callbacks and before subsequent input dispatch. Node layout may still be pending; use `SizeChanged` to observe a node's resolved size. Initial state and unchanged values do not emit notifications, and later subscribers receive no replay. App payload storage is prepared when listeners are installed, independently of the business notification budget.

## Host state

`HostFocusChanged { focused }` and `HostVisibilityChanged { visible }` are App notifications. The first platform report establishes the state; repeated values are ignored and later listeners receive no replay. Platforms without a report leave that state unknown.

SDL reports window focus and shown/minimized state. WGPU reports window focus and occlusion. Web reports canvas focus while its document is focused, and document visibility; it does not infer CSS visibility or overlapping elements.

Losing host focus or visibility cancels active input before delivering the notification. Widget focus is retained. Neither notification suspends or resumes the App; `AutoSuspendPlugin` responds only to explicit platform suspend/resume requests.

## Initialization

Use `attach = initialize::callback()` for initialization during widget assembly. The callback uses `#[event]` with the same entity, query, resource, Model, and command roles as other callbacks. It does not require an event payload.

```rust,ignore
#[event]
fn initialize(node: entity!(), commands: commands!()) {
    commands.insert(node, TouchAction::PanY);
}

#[view(component = Slider, attach = initialize::callback())]
fn slider_behavior() {}
```

Static components, parent links, and IDs are installed before initialization runs. Callback borrows end and deferred commands commit before the next attachment executes. Reactive properties, authored event bindings, and children follow their normal assembly order. Initialization does not bubble, add pointer feedback, or consume business notification storage.

Each registered View prepares its initializer on the first matching entity and reuses it for later instances in that App. Configure event capacity before constructing widgets. Preparation failure logs a warning and disables that initializer; execution failure skips the failed invocation without rolling back completed mutations. Niche declarations retain their composition lifecycle and slot construction.

## Input

Declare a widget's default input behavior with `#[view(input = ...)]`. Each handler uses the existing `#[event]` parameters; `entity!()` identifies the current widget and the gesture payload retains the original hit target.

```rust,ignore
#[event]
fn toggle(
    node: entity!(),
    _tap: event!(Tap),
    boxes: query!(Checkbox),
    commands: commands!(),
) -> bool {
    let Some(checkbox) = boxes.get(node) else {
        return false;
    };
    let now = !checkbox.checked;
    commands.set::<mirui::ui::property::prop::CheckboxChecked>(node, now);
    commands.emit(node, mirui::ui::widgets::checkbox::Toggled { now });
    true
}

#[view(component = Checkbox, input = toggle::callback())]
fn checkbox_input() {}
```

Register this descriptor through `App::with_widget`. A custom descriptor for an existing component replaces its default input implementation only when the default descriptor is not also registered. An empty body supplies no painting. Input and painting can belong to the same descriptor.

Use a tuple for multiple input callbacks: `input = (tap::callback(), drag::callback(), key::callback())`. Handlers declare their payload with `event!(Tap)`, `event!(DragMove)`, `event!(Key)`, or another supported input type. Supported gestures are Tap, LongPress, DragStart, DragMove, DragEnd, DragCancel, Pinch, and Rotate; keyboard inputs are Key and TextCommitted. Business notifications remain authored `on` bindings, not View input hooks.

Callbacks are prepared when the first matching widget is assembled, after the App's event capacity has been configured. Matching instances share prepared callback storage; they do not each allocate command queues. Each callback invocation receives its current entity and borrows parameters afresh. Stable input dispatch reuses prepared storage.

Gesture hooks run before authored listeners on the same node. Keyboard hooks run after authored listeners on the focused node and its ancestors, before App fallback. Within a View, matching hooks run in tuple order. Returning `true` or `()` consumes input; returning `false` continues the route. Keyboard-only hooks do not add pointer hit targets or interaction feedback. Entity lifetime and retained-cell guards apply to the frozen route.

## Notification delivery

Slider reports `DragStarted`, `ValueChanged`, and either `DragEnded` or `DragCancelled`. Cancellation preserves the last value and does not also report a normal end.

`commands.emit(node, payload)` records a deferred command. At command commit, a matching listener must exist for that exact entity and payload type; otherwise the payload is dropped. Skipped events do not occupy business notification storage, and later subscribers do not receive them. Payload expressions still execute normally when the command is recorded.

`commands.emit_always(node, payload)` records an unconditional enqueue request. A listener installed after enqueue but before dispatch can receive the notification. Dispatch without listeners discards the event; the queue does not preserve history. Invalid entities, missing source components, and insufficient notification capacity still fail through the normal callback error path.

Both methods use `CommandCapacity::emit::<Payload>(count)` for command records and payload storage. Notifications that are actually enqueued also use the App's business notification budget. Recording or submission errors are handled by the common executor; completed mutations are not rolled back. Commands run in order after callback borrows end, so a property update recorded before a notification is applied before that notification is dispatched.

Queued notifications retain the source cell's data-binding identity. Rebinding or deactivating that cell discards its older notifications, including unread callbacks of a notification already being dispatched. New notifications use the current binding. Discarded payloads are released without replay.

An explicit `EventCapacity` replaces the App's default preset. Its command budget also applies to registered widget initialization and input callbacks, including component insertion, property changes, animation commands, and emitted notifications; it is not limited to authored `on` handlers.

Use a Model subscription when a later subscriber needs current state. A notification describes an occurrence; neither emission method stores past occurrences for future subscribers.
