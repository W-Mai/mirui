# Events and callbacks

Declare typed payloads, bind callbacks, and configure bounded notification and command storage. For widget-specific input and notifications, see [Widget events](widget-events.md); for document editing and input methods, see [Text editing](text-editing.md); for selection snapshots and platform writes, see [Clipboard](clipboard.md).

## Event type contracts

Selection requests use `ui::selection::SelectAll` and return `Result<(), RequestError>`. `commands.selection().select_all(entity)` invokes one provider on that node without bubbling, App fallback, or notification storage. TextEditing registers a default provider. Custom components can register `#[event]` callbacks in a View's `input` tuple, or use any existing `on SelectAll` callback form to override the default on a specific node. Multiple matching View providers are rejected before invocation unless an explicit node handler resolves the ambiguity. Request-only bindings do not add pointer feedback or hit targets. Commands run after callback parameter borrows end and before the next command; unavailable, conflicting, or recursive requests warn and are skipped.

Requests retain their resolved provider and validate its identity before and after execution. Replacing the provider, its View component, or a declared `WidgetSource` component invalidates the request; ordinary field writes do not replace an attachment. Retained-row rebinding also invalidates the original request. A rejected result does not roll back changes already made by the callback.

`input::event::contract::Event` declares a payload's route, source, and options. `EventKey<E>` stores only the options and preserves the payload type; it does not own a payload, a model, or a callback. A key with `Options = ()` occupies no memory.

Widget notification types use `EntityNotification` and `WidgetSource<W>`:

```rust
use mirui::input::event::contract::{EntityNotification, Event, WidgetSource};

#[mirui::component]
struct Pad;

struct Hit {
    strength: u8,
}

impl Event for Hit {
    type Route = EntityNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}
```

The source capability accepts only `Pad`, not another widget or an App listener. `EntityBubble` describes target-to-ancestor gestures; `InputFallback` describes input reaching App listeners after focus or scroll handling; `AppNotification` describes non-consumable App notifications. Source/route pairs are checked when the payload implements `Event`.

`EventResult<R>` encodes the route's result policy. Consumable routes accept `()`, `bool`, or `BubbleControl`; notifications accept only `()`. `EventOptions` validates typed arguments and returns explicit errors before storing options in an event key. Declaring a payload does not require an event-definition macro or changes to the UI macro's event catalog.

Built-in notification payloads live in their widget modules: `slider::ValueChanged`, `slider::DragStarted`, `slider::DragEnded`, `slider::DragCancelled`, `switch::Toggled`, `checkbox::Toggled`, `progress_bar::ValueChanged`, `tabbar::SelectionChanged`, and `text_editing::Changed`. The corresponding constants, such as `Slider::ValueChanged`, are typed event keys. Qualifying a selector does not permit installing a different widget's notification: `Switch::Toggled` retains `WidgetSource<Switch>`.

The ordinary prelude exports `Tap`, `LongPress`, `DragStart`, `DragMove`, `DragEnd`, `DragCancel`, `Pinch`, and `Rotate`, plus `Event` and `EventKey`. Their fields match the recognizer's gesture data, including its original target. `GestureSelectors` provides the corresponding typed constants for any component. The default Tap key matches every tap. An explicit count matches only that multiplicity, so `Tap::EVENT.configure((1,))` differs from `Tap::EVENT`. Counts outside 1 through 255 return `TapCountError`. Other gesture keys have no options.

Event blocks, inline callbacks, ordinary Rust payload functions, and named `#[event]` callback factories use the same installer and prepared executor. A block reads its payload through `event`; a single untyped closure parameter receives the payload inferred from the selector. A function passed directly receives `&E` and returns a result permitted by the event's route. Callback parameters use `event!(E)`, `entity!()`, `res!(...)`, `query!(...)`, and `commands!()` for invocation-time access. Models bind through the enclosing composition or a named callback factory.

```rust,ignore
Button(text: "DROP") on Tap { game.drop_ball(); }

View () on Tap(|input| { game.select_at(input.x, input.y); })

View () on DragMove(|input: event!(DragMove), node: entity!(), items: query!(mut Draggable)| {
    if let Some(item) = items.get(node) {
        item.move_by(input.dx, input.dy);
    }
})

View () on DragMove(moved::callback(game, step))

View () on Tap(handle_tap)
```

An event block treats all parenthesized arguments as selector options. Without a block, the last argument is the callback and any preceding arguments are options: `on Tap(2, callback)`. Node children precede trailing callback bindings: `View () { child_panel() } on Tap(callback)`.

`on Tap(n) { ... }` evaluates its `u32` configuration once when installing the handler, using the same validation as a Tap key. Invalid counts log a warning and skip that binding without failing UI construction or suppressing other valid handlers. A node with no valid gesture bindings receives no authored gesture handler or extra hit target. Dispatch compares the observed tap count with the stored options without reevaluating `n` or repeating installation warnings.

Ordinary and counted Tap handlers run in declaration order, stopping at the first consuming handler. An ordinary Tap is not a fallback for counted handlers. All nodes in a bubbling gesture use the original target's count, frozen before internal or authored callbacks run.

```rust,ignore
View ()
    on Tap { game.record_tap(); false }
    on Tap(2) { game.record_double_tap(); }
```

The first handler runs on every click and allows the second handler to run on a double tap. Returning `()` or `true` instead stops that sequence.

`Key` carries keyboard metadata; `TextCommitted::text()` borrows a complete UTF-8 fragment. Both support entity and App input source capabilities. `InputSelectors` supplies their component-node keys; focused nodes and ancestors run before App fallback. `Wheel` carries its position and deltas; `Rotary` carries its device id and delta, without fabricated pointer coordinates. Wheel and rotary callback sources are App-only.

SDL, WGPU, and Web forward Ctrl/Command+A, C, X, and V as `Key` with `KEY_A`, `KEY_C`, `KEY_X`, and `KEY_V`. Ordinary letters remain text input. After Key propagation is allowed, the UI invokes the focused component's selection or clipboard capability; `()` or `true` consumes the Key and prevents that default. Clipboard operations still require the Surface's platform bridge. See [Clipboard](clipboard.md) for selection and completion behavior.

Native clipboard menu actions arrive as `InputEvent::ClipboardInput` and use the focused component's clipboard requests directly. They do not represent a physical key and therefore do not invoke Key listeners. Native paste supplies its owned data; copying and cutting still require a successful platform write.

SDL and WGPU preserve press and release for supported editing keys. Character input is produced by text input or key presses, not key releases. Escape retains its quit-request behavior; releasing it does not generate a second request.

`Started`, `Suspended`, `Resumed`, `MemoryWarning`, `HostInteraction`, and `Quit` are App notification payloads with unit options and results. They do not implement platform-input projection: raw suspend/resume/quit requests must not be treated as notifications of completed lifecycle transitions.

Active input simulation suppresses competing pointer, wheel, rotary, keyboard, and character input before widget and plugin input handlers. Suspend, resume, and quit requests remain available to the application lifecycle path.

## Layout size events

`SizeChanged` reports a node's first available logical size and subsequent width or height changes. `event.old` is `None` for a listener's first delivery; `event.new` is a `LayoutSize` with `width` and `height`. Position, scroll offsets, output scaling, and visual transforms are not size changes. Hidden or detached nodes are not measured for this notification.

```rust,ignore
View(width: Dimension::percent(100), height: 360) on SizeChanged {
    model.set_stage_size(event.new.width, event.new.height);
}

View() on SizeChanged(|change: event!(SizeChanged)| {
    model.set_stage_size(change.new.width, change.new.height);
})

#[event]
fn resized(model: model!(PanelModel), change: event!(SizeChanged)) {
    model.set_stage_size(change.new.width, change.new.height);
}

View() on SizeChanged(resized::callback(model.clone()))
```

`SizeChanged`, `LayoutSize`, and `LayoutSelectors` are exported by the ordinary prelude. Typed builders use `on_event(SizeChanged::EVENT, callback)`. Size notifications return `()`, do not bubble, and do not add hit targets or interaction feedback. They are produced by layout, cannot be submitted through `commands.emit`, and cannot be installed on App.

All sizes in a pass are captured after computed rectangles are written, before callbacks run in installation order through the shared event executor. Query and resource parameters retain their existing access rules; commands apply after parameter borrows end. Reactive updates participate in the same frame's layout settling. A listener can receive several distinct sizes in one frame: this is not a final-layout event. Feedback is bounded; non-convergence logs a warning and retains pending layout work while rendering and hit testing use the last completed snapshot. Callback mutations are not rolled back.

Observation storage is prepared from the actual number of installed size listeners and reused during capture and delivery. Size snapshots do not consume business notification payload capacity; notifications emitted by their callbacks still use `EventCapacity`. Callback registration and structural construction retain their normal allocations.

Curve Text uses a size event to update `CurveModel.stage_size`; its three `Text.path` properties observe that value with `$` bindings. Resizing updates their path ranges even while animation is paused. Use `@` dependencies for cross-node layout properties and size events for geometry-driven Model state; neither requires an author callback that receives World.

## App callbacks and event capacity

`App::on_event(E::EVENT, callback)` installs a named callback for App lifecycle notifications or input fallback. The callback factory infers the installation context; authors do not name an internal context type or write a World callback.

```rust
use mirui::prelude::*;

#[model]
struct Shortcuts {
    enabled: bool,
}

#[model]
impl Shortcuts {
    fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }
}

#[event]
fn handle_key(input: event!(Key), shortcuts: model!(Shortcuts)) -> bool {
    if input.pressed && input.code == mirui::input::event::input::KEY_RETURN {
        shortcuts.toggle();
        true
    } else {
        false
    }
}

let mut app = App::headless(320, 240);
let shortcuts = app.add_model(Shortcuts { enabled: false }).finish();
app.on_event(Key::EVENT, handle_key::callback(shortcuts));
```

Focused keyboard handlers and default text editing run before App key and character fallback. Wheel and rotary events first try the scroll route. An accepted input is not dispatched again through a second handler path. App fallback also works without a widget root. Plugin consumption can stop user input, but does not stop lifecycle requests from reaching other plugins or bypass cancellation.

Atlas Restoration, Echo Walker, Pocket Post, Twin Beacons, and Folding Ark bind keyboard controls with `#[event]`, `event!(TextCommitted)`, and `model!` parameters. Their shortcuts match a complete single-key string at App fallback, without splitting pasted text into game actions. Registered instances are shared with the rest of each game; input consumed by a focused editor does not trigger the shortcut.

On Web, a focused canvas delivers non-composing character key presses as `TextCommitted`, including Space. A focused text editor receives its text through DOM input instead, so keyboard input is not committed twice. Control and Command combinations do not produce character commits.

WGPU delivers platform keyboard text to App callbacks when no native editor session is active. With an active editor, keyboard text and IME commits use that editor's session. Late IME commits retain their original session and cannot become App shortcuts after the editor loses focus.

Twin Beacons and Folding Ark install their hint services with `App::add_resource` before binding the keyboard callbacks, which borrow them through `res!` on each invocation. At setup, each game's keyboard and hint controls share the same search workspace; subsequent keyboard calls read the current installed service.

`Started` follows the startup hooks once, including applications driven directly by `tick`. Suspend and resume notify only after an actual state change and their plugin hooks; suspension and quit cancel active pointer, scroll, and gesture state. Memory warnings and host interactions notify after their hooks. Quit hooks and notifications run once even if `tick` is called again. These notifications join the same FIFO as previously submitted widget notifications.

App provides a bounded event preset automatically. Ordinary widget notifications and short deferred callbacks need no capacity declaration. The preset reserves 8 business notification records per buffer, 5 deferred command records per callback, and at most 16 notification deliveries in one outer flush. Notification payload storage is sized and aligned for 8 `ClipboardFinished` values, including space for focus notifications. The command payload budget is calculated from one theme change, one transform change, insertions of TextEditing and TextInput, and one text-input request. The actual component layouts determine payload size and alignment on each target.

The automatic preset prepares storage only when installation needs it. Without an explicit override, applications using only direct input and Model callbacks without deferred commands do not allocate business notification storage; callbacks without `commands!()` do not allocate command queues. A deferred callback prepares notification storage because commands can emit notifications. An explicit capacity declaration prepares its requested notification storage immediately. Notification buffers are shared by the App, while each deferred callback owns a separate reusable command queue. The command quota applies to one callback invocation, not the sum of commands from every callback in a frame.

`EventCapacity::default()` selects that preset. `EventCapacity::new()` starts an explicit empty declaration; it does not inherit the preset. Applications needing larger payloads, longer notification chains, or batches of commands can replace the preset through `App::with_event_capacity` before installing the first callback. `EventCapacity` is available from `mirui::app` and the ordinary prelude:

```rust,ignore
app.with_event_capacity(
    EventCapacity::new()
        .notifications::<PadHit>(MAX_PENDING_HITS)
        .commands(CommandCapacity::new().emit::<PadHit>(MAX_HITS_PER_CALLBACK))
        .flush_limit(MAX_DELIVERIES),
);
```

Explicit notification counts describe combined pending payload storage, not per-type runtime quotas. A nonzero notification declaration needs an explicit flush limit. Omitted notification or command storage in an explicit declaration does not inherit the corresponding preset budget. An explicitly declared zero quota supplies no storage for that operation.

The first successfully installed callback freezes the capacity configuration. Reapplying the same configuration is a no-op; an invalid or different configuration logs a warning and preserves the installed configuration. A rejected binding logs a warning and skips that binding. Dispatch never grows the prepared buffers. Runtime submission and flush failures retain their structured error contract internally and are logged by App; completed data changes are not rolled back, and failed work is not automatically retried. An interrupted notification flush discards its remaining notifications and callback writes.

App notification listeners reserve one additional record and payload storage sized and aligned for their largest registered App event, independent of business storage. Business notifications cannot borrow that slot. Listener and route scratch storage are prepared from actual installed listeners, matching gesture hooks, and keyboard handlers at structural boundaries. Stable dispatch does not grow those buffers; route snapshots and callback borrows finish before further preparation.

## Notification payload storage

The notification service and its retained handle are private. Runtime preparation reserves an existing service in place instead of replacing it; generic resource commands cannot remove or replace that service. Preparing capacity during an active flush returns `Busy`, while normal emission continues into the separate write buffer.

Internal notification storage owns payloads in two explicitly reserved buffers. A callback reads one stable payload window while emitted notifications append to the other buffer. Remaining records in the current batch run before callback writes, preserving FIFO order without recursive delivery.

Record count, payload bytes, and alignment are logical limits; unused allocator capacity does not raise them. Writes within those limits do not allocate. Rejected writes return the original payload, and interrupted or feedback-limited flushes destroy pending values and restore reusable storage. Notification references are read-only and cannot escape their invocation.

Drain callbacks return a result. Storage errors and listener errors remain distinct; listener errors are returned without cloning or replacing the error value. Failure discards unread and newly emitted notifications and leaves the buffers reusable. Model and resource updates already applied by a listener are not rolled back.

This storage facility does not install listeners, resolve widget sources, or validate an application's owner and entity generations. Those checks belong to event installation and routing, not the payload arena.

## Prepared callback execution

`CommandCapacity` is available from `mirui::ecs` and the ordinary prelude. Its typed operation counts describe deferred storage without exposing arena bytes or alignment. `insert`, `remove`, `despawn`, `insert_resource`, and `remove_resource` describe structural operations; `set`, `set_theme`, `emit`, and `text_input` include their actual deferred apply payloads. Capacity construction is allocation-free and supports const declarations.

`commands.despawn(entity)` removes the entity and its attached `Parent`/`Children` subtree after callback borrows are released. It unlinks the parent, clears node-owned IDs and reactive bindings, and invalidates the vacated display. Entities without children are removed individually; arbitrary entity references and shared Models do not extend the deletion scope. Registered View systems remain installed. Deleting the active App root or an ancestor containing it logs a warning and skips that command without modifying the tree. Repeating a deletion for an expired entity cannot delete a replacement using the same slot.

```rust
use mirui::prelude::*;

#[component]
struct Marker;

const COMMANDS: CommandCapacity = CommandCapacity::new()
    .insert::<Marker>(1)
    .despawn(2);
```

Each count reserves a combination of operations, not a per-type runtime quota. The declared combination fits in any recording order, including mixed alignments and empty operations. Recording a different, larger combination can still exceed the total record, byte, or alignment limit. Arithmetic overflow is returned by bounded preparation rather than wrapping or reducing the declaration. Prepared queues retain only the resolved limits, not the capacity-construction statistics.

Named event factories derive installation inputs, typed invocation parameters, access metadata, and body arguments from one ordered parameter plan. Authored types remain in their declaration scope; relative paths are relocated only when emitting the generated factory module. Model and ordinary value arguments retain their declaration order.

Internal typed callbacks share the system parameter fetch and access validator. Preparation rejects component/resource aliases and foreign, mixed, or retired declared Model bindings with structured installation errors. App logs these errors and skips the affected binding. Successful preparation binds the callback to one App and reserves the current query change-record budget. The prepared callback and its access plan cannot be replaced independently.

Event factories accept the same shared binding values as composition. Optional bindings recursively contribute their registered Model identity; `None` contributes none. Signal, Computed, and audio handles retain their own access rules instead of acquiring a Model registration. Capture requirements merge without retaining Model cells or allocating owner storage. This is an installation identity check, not a new lifetime lease or poison check.

Ordinary Rust captures retain their value semantics. The framework does not inspect or reject manually constructed containers to discover Models inside them. Model access still enforces the registered App and lifetime; an invocation-time failure does not roll back values already changed by the callback.

Invocation checks the App identity and entity generation before fetching parameters. Entity contexts receive an `Entity`; App contexts receive no entity. Payloads are read-only borrows, resources and query rows cannot escape the body, and callback results follow the payload's route contract. Snapshot copies share the installed callback allocation. Execution within reserved capacities does not allocate callback storage.

Deferred callbacks receive a command facade backed by installation-time storage limits. The body records commands; every live parameter borrow ends before the executor applies them in source order. A recording error discards that batch and is returned from invocation. It does not roll back Model or resource values already updated by the body. Callback and apply unwinding release pending payloads and leave storage reusable. A busy command buffer returns an error rather than recursively borrowing it. Plain callbacks have no command buffer.

Deferred operations can fail during submission as well as recording. `CommandError::Commit` distinguishes submission failures from command-record limits. Submission stops at the first error, drops unexecuted payloads, and preserves completed operations. No callback borrow remains active during submission.

Built-in systems expose `name::system()` descriptors for registration; their raw World callbacks are crate-private. `SystemScheduler::run_all` returns `Result<(), SystemRunError>`. A failure identifies the system and its `CommandError`, stops the remaining systems in that batch, and preserves completed updates. Failed calls contribute timing when a clock is installed; unexecuted calls do not. App logs scheduling failures and completes rendering and the frame. Owner, access-plan, and required-resource validation remain programming-error checks.

Timer callbacks consume their invocation after the body completes. Command submission errors still advance periodic schedules, decrement repeat counts, and remove completed one-shot or deadline timers. The error stops the remaining timers and returns through `SystemScheduler::run_all`; already completed updates are not rolled back or automatically retried. Timer command storage remains reusable after failure. Ordinary `#[system]` and `#[timer]` functions retain their authored signatures; fallible execution belongs to the internal wrappers.

This execution layer does not install or route listeners. Its ABI is confined to hidden macro support, not the ordinary authoring prelude.

## Listener selection and notification delivery

The internal listener pool installs prepared callbacks for entity and App targets. Widget notification installation checks the source component and entity generation. Captured Model owners are validated by preparation; the pool rejects a different App before installation or dispatch.

Typed widget contexts constrain a notification source to the widget's component type at compile time. Generic node contexts validate the source component after attachments have been installed. Both contexts use the same installer, prepared callback, and listener pool; an event qualifier does not prove that a component exists. App-only sources cannot acquire a node context.

Listener entries and snapshot steps have independent explicit count limits. Matching produces an installation-order snapshot without borrowing the World or retaining a borrow of the entry list. Removing a listener does not change the current selection; a new listener participates in later selections. Each callback checks entity generation before running. Notification callbacks fan out; consumable input stops on a prevention result.

Installation and fresh route selection reclaim listeners for retired entity generations before checking the installation quota or choosing callbacks. Reclamation retains entry storage and live installation order. Captures are dropped outside the entry borrow; an existing snapshot keeps its shared callback until that route ends. App listeners remain installed until removal or pool disposal. Removing a source component alone is not entity retirement.

Notification draining uses this pool and the same prepared executor. Payloads are borrowed without cloning, and reserved invocation scratch is cleared on success, error, and unwinding. Reentrant use of that scratch returns an error. Selection and draining within reserved capacities allocate no listener storage.

Gesture selection freezes the complete Parent chain before running any body. Each node contributes matching internal View hooks in registration order, then its authored listeners in installation order. Reparenting, removing a filter component, or changing listeners does not alter that selection. Each step still checks entity generation before execution. Hidden or disabled nodes and ancestors suppress the route before any steps are selected. Eligibility is frozen with the route; later state changes do not rewrite its selected steps. Business notifications do not use input eligibility and still reach live disabled or hidden widgets. Parent cycles and insufficient step capacity fail before any body runs.

Keyboard selection freezes the focused node, ancestor chain, and default editing hooks before running any body. Each node contributes authored listeners first, then its default editor; App fallback comes last. A default editor's acceptance stops the route even when its content or caret does not change. Hidden, disabled, missing, or generation-stale focus skips entity input but still permits App fallback. Key and character payloads share this route; wheel and rotary payloads cannot use it. The keyboard route reuses the same step storage, generation checks, owner scope, and error cleanup as gesture and notification delivery.

The internal scroll handler reports wheel and rotary acceptance separately from offset changes. A wheel spring submitted on a matching axis is accepted even at a clamped boundary. Rotary input requires a live retained target with a scroll offset; retired targets and targets without that component are not marked dirty. The listener pool's scroll route runs that handler first and selects App listeners only when scrolling is not accepted. It retains wheel coordinates and rotary hardware fields, uses the same prepared callbacks and reserved step storage, and rejects foreign owners or busy scratch before scrolling. App fallback uses this route after scroll acceptance is checked.

Internal hooks and listeners share the reserved step storage and cleanup guard. A failed reservation does not raise either logical quota. The pool's ABI is confined to hidden macro support. App callbacks, widget builders, and all three `on` forms install into the same prepared listener pool.

## Deferred widget notification submission

`commands.emit` enqueues a notification only when that entity has a listener for the exact payload type at command commit time. Without a listener, the payload is dropped without using business notification capacity or retaining history. `commands.emit_always` enqueues regardless of listeners; it still validates the entity, source, and storage. Both use `CommandCapacity::emit` for deferred command storage. Programmatic property updates do not emit user notifications. Missing or exhausted capacity never triggers hidden growth, and completed state changes are not rolled back. See [Widget input and notifications](widget-events.md) for View callbacks and delivery timing.

The internal command facade records `emit(entity, payload)` without reading the World or borrowing notification storage. The payload must declare an entity notification route; input and App lifecycle payloads cannot be emitted through this capability. Recording a request does not mean that it has been delivered.

After callback borrows end, submission checks the entity generation, actual source component, and notification storage owner. The owning payload moves from command storage into the pending notification arena without cloning. Missing sources, missing storage, and storage limits return structured errors through the prepared event executor. Source removal earlier in the command batch is visible to a later submission. Already completed data changes are not rolled back.

Pending storage is retained by an owner-bound shared runtime resource. Draining retains a shared reference before acquiring the World, so callbacks can submit further notifications without removing that resource. The listener pool drains the queue in FIFO order with stable read payloads and separate write storage. App prepares business storage from the automatic preset or an explicit `EventCapacity` and flushes at input, system, render-hook, and lifecycle boundaries.

## Recognized gesture batches

App input, `SimulatedInput`, and `SimTimeline` dispatch recognized gestures with the current clock time. Tap focus is updated before internal and authored handlers; hidden targets leave focus unchanged. Simulated recognizer input and tap-count tracking use the same tick timestamp, so successive taps retain their multiplicity.

App input and both simulation routes share a retained gesture batch handoff. The installed `GestureSystem` prepares a write buffer and a separate read buffer. Dispatch swaps their storage and drains the selected read batch without borrowing the World resource during callbacks or collecting a temporary Vec. Events queued by a callback stay in the write buffer, outside that batch.

Recursive draining of the same read buffer returns `Busy`. Dispatch errors and unwinding clear unread events while retaining capacity and newly queued write events. Removing or replacing `GestureSystem` does not invalidate the selected batch or consume the replacement's events. Standalone `GestureEvents` retains only its ordinary output buffer.

Each complete gesture route releases its listener snapshot before flushing widget notifications. The next gesture in the retained batch runs afterward; an arbitrary platform poll burst therefore does not accumulate all its widget notices into one business buffer. Inner notification delivery keeps FIFO order and its explicit feedback limit.

This handoff allocates no storage during dispatch. Recognizer writes retain their existing growth policy. The handoff does not impose a bounded input-burst quota or change callback selection.

## Named event callback declarations

`#[event]` generates a module with `callback(...)`. The factory takes registered Model instances and ordinary `Copy + 'static` values in declaration order. Resources, queries, the payload, the current entity, and deferred commands are invocation parameters, not factory arguments.

Types and const arguments using `self::` or `super::` retain their authored module scope in the generated factory. Imported and absolute paths are unchanged.

For example:

```rust,ignore
#[event]
fn moved(
    game: model!(Game),
    step: u16,
    input: event!(DragMove),
    node: entity!(),
    positions: query!(mut Position),
) -> bool {
    let Some(position) = positions.get(node) else {
        return false;
    };
    game.move_to(position, input.x, input.y, step);
    true
}
```

`moved::callback(game, step)` describes the callback without exposing World or generated handle types. `event!(E)` borrows a concrete payload; omitting it leaves the event type to the binding site. `entity!()` excludes App-only bindings. Notification callbacks return `()`; consumable routes also accept `bool` or `BubbleControl`. Named factories and inline `on` callbacks share System resource/query lowering and the prepared executor.
