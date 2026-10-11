# Widget initialization, input, and notifications

## Input delivery

Native and simulated input use the same delivery order. Recognized gestures, their deferred commands, business notifications, and reactive updates complete before the next raw input is handled. A Tap followed by a Key in one input batch therefore delivers Tap first. Callback changes that hide, remove, or rebind a node affect subsequent inputs. Systems and painting retain their frame schedule.

Spatial input refreshes stale layout or transformed hit geometry before dispatch. Unchanged geometry is reused; pending paint damage remains for the frame renderer. A layout failure skips spatial dispatch and cancels active input instead of hitting stale coordinates.

`Key` carries `code`, `pressed`, `modifiers: Option<KeyModifiers>`, and `repeat: Option<bool>`. Modifiers expose `shift`, `control`, `alt`, and `meta`. `Some(false)` identifies a non-repeated key event; `None` means the platform did not provide that information. SDL and Web supply both snapshots; WGPU modifiers remain unknown until a native modifier update and are cleared on focus loss or suspension. Linux uses the keyboard device state and native repeat flag. NuttX keyboard records do not provide modifiers or repeat metadata. Simulated input supplies the fields explicitly.

## Pointer identity

Raw `InputEvent` pointer records carry `kind: PointerKind` (`Mouse`, `Touch`, `Pen`, or `Unknown`) and `id`. Gesture and scroll capture match both fields; movement or release from another pointer cannot end that capture. Backends report the device category rather than inferring it from the slot number. Simulated cursor actions use `Mouse`; simulated pinch and rotation contacts use `Touch`.

`PointerEnter` and `PointerLeave` report surface boundaries without inventing coordinates. Entering waits for a position-bearing input before hit testing. Leaving clears hover and cursor feedback without releasing an active drag; captured movement still uses its actual coordinates. Touch does not produce hover feedback, and pen hover requires an explicit enter or non-contact movement.

Web and WGPU map active native pointers to stable slots: mouse uses 0, with up to 16 other pointers occupying separate slots. A full map ignores new contacts without replacing active ones. Touch slots retire on release or cancellation; Web hovering pointers retain their slots until leaving without an active press. Host focus/visibility loss clears the mapping. Web lost pointer capture cancels an active press; a capture-loss event following normal release does not cancel it again. Pointer slot numbers are not persistent device identifiers.

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

Use a tuple for multiple input callbacks: `input = (tap::callback(), drag::callback(), key::callback())`. Handlers declare their payload with `event!(Tap)`, `event!(DragMove)`, `event!(Key)`, or another supported input type. Supported gestures are Tap, LongPress, DragStart, DragMove, DragEnd, DragCancel, Pinch, and Rotate; keyboard inputs are Key and CharInput. Business notifications remain authored `on` bindings, not View input hooks.

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
