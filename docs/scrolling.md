# Scrolling

Framebuffer transfers use the difference between the current scroll offset and the last successfully painted offset. Nested transfers use the painted source window; a changed clip or incompatible geometry repaints instead. Input handlers do not maintain a separate render delta.

## Inspecting pending drawing

`app.dirty_regions()?` borrows the pending redraw rectangles and scroll transfers in logical pixels. `affected_regions()` includes both kinds of coverage; `app.dirty_region()?` returns their bounding rectangle. Queries prepare reactive state and layout without advancing animation or consuming pending drawing. Changes made after a query are included when `render_dirty()` prepares its plan again.

```rust
use mirui::prelude::*;

let mut app = App::headless(240, 120);
app.with_default_widgets();
app.spawn_root().id();
let coverage = app.dirty_region()?;
let pending = app.dirty_regions()?;
assert_eq!(coverage, pending.bounding_rect());
app.render_dirty()?;
# Ok::<(), mirui::render::RenderError>(())
```

The query is not a frozen rendering command or a promise of exact surface flush rectangles. Render hooks, subsequent changes and backend capabilities can change the final plan. Successful drawing acknowledges prepared damage; preparation or drawing failures retain pending work. First frames, resized surfaces and transient backbuffers require full coverage.

An independently held Model can still change while a query result is borrowed. That result remains a snapshot; query again to inspect the new state, or call `render_dirty()` to draw it.

Redraw rectangles are normalized into non-overlapping areas. Intersecting regions retain their gaps when the prepared region capacity permits; excess fragmentation is conservatively merged. Scroll-transfer areas remain ordered and may overlap redraw areas.

Unfilled borders retain their untouched center in the painted baseline. Transparent layout containers do not add their whole layout rectangle to damage; their children are measured independently. Scroll movement that cannot transfer framebuffer pixels contributes the children's old and new coverage, including movement inside rounded clipping. Custom drawing without a known coverage measurement retains conservative bounds.

Hidden or deleted subtrees contribute their last painted coverage before removal from the layout tree. Pending scroll transfers map those regions before erasure; successful drawing clears hidden baselines.

## Input and viewport

Scrolling does not snap to cell boundaries. Pointer and wheel input share elastic resistance and released motion; continued input controls the content without a concurrent rebound. Pointer-release inertia follows recent content movement and expires after the pointer stops. Within scroll bounds, wheel input applies its fractional deltas directly, retaining platform-provided momentum. Scroll offsets invalidate painting and hit geometry, while cell reassignment or size changes still update layout. Framebuffer reuse is restricted to whole physical-pixel shifts in untransformed rectangular viewports without borders or rounded corners; other movement repaints the affected viewport.

`InputEvent::Wheel` and the typed `Wheel` payload use logical pixels for `dx`, `dy`, `x`, and `y`. Positive `dx` increases the horizontal scroll offset; positive `dy` decreases the vertical offset. Backends normalize displacement before converting it to fixed point. Custom surfaces and simulated wheel input must provide pixel displacement, not wheel ticks. `Rotary.delta` remains an encoder step count.

Web pixel deltas follow the canvas's current CSS-to-logical scale; line deltas use 20 logical pixels per line, and page deltas use the surface's logical width and height. WGPU pixel deltas are divided by the window scale factor once. SDL precise wheel values and integer wheel ticks from Linux/NuttX use the same 20-pixel step. No additional in-bounds momentum is applied to wheel input. Linux's integer wheel adapter does not consume high-resolution evdev wheel events.

For SDL runtime requirements and macOS trackpad setup, see the [desktop setup](quickstart.md#desktop-sdl).

## Observing content movement

Use `on ScrollChanged` to observe actual offsets and `on ScrollStarted` / `on ScrollEnded` to observe motion segments. These notifications include programmatic writes and retain fractional and elastic positions. `ScrollEnded` follows inertia and rebound, not the pointer release. All three support inline bodies, closures, and `#[event]` callbacks on nodes with `ScrollOffset`; see [scroll notifications](widget-events.md#scroll-notifications).

Read `query!(ScrollBounds)` for the viewport, content extent, and maximum offsets from the last successful layout. Configuration changes appear after layout succeeds; reading this component does not perform layout.

## Virtualized cells

`LazyList` creates cells after its first nonempty layout and retains them across scrolling. Supply `items`, the cell extent, and one `on CreateCell` callback returning a new cell root. `items` accepts a row count (`u32`) or a registered ModelList/ModelMap instance through automatic `Into` conversion. A count generates indexed rows without creating Models. The list owns pool sizing, positions, and recycling; authors do not prebuild slots or maintain a pool.

Visible rows intersect the viewport with positive area, including partial rows at either edge. `visible_start` is the first intersecting data index, not the start of the retained buffer. Spare slots hold up to two extra rows on either side without displacing visible rows or filling past the data range. Elastic offsets remain unchanged; empty viewports and offsets entirely outside the data hide retained rows. The total content extent must fit layout coordinates.

Cells already bound inside the retained range keep their entities when the viewport changes. Missing indices use free cells; unused cells remain hidden for reuse. `item_extent` specifies height for a vertical list and width for a horizontal list, selected by `ScrollConfig.direction`. The list derives the virtual content extent from its count and cell extent.

`CreateCell` runs only when a new physical cell is needed. Its `item` is a library-owned `ListItem` Model whose observed `index()` changes when that cell is reused. Property bindings read the current index and update through the normal reactive and dirty mechanisms. Input callbacks also read the index when invoked, rather than capturing a construction-time integer.

```rust,ignore
use mirui::ui::widgets::lazy_list::CreateCell;

#[event]
fn create_row(event: event!(CreateCell)) -> Entity {
    const LABELS: [&str; 4] = ["ALPHA", "BETA", "GAMMA", "DELTA"];
    let item = event.item.clone();
    ui! {
        Text(text: ${ LABELS[item.index() as usize % LABELS.len()] })
    }
}

#[compose]
fn rows_panel() {
    ui! {
        LazyList (height: 240, items: 100_u32, item_extent: Fixed::from_int(24))
        on CreateCell(create_row::callback())
    };
}
```

Blocks, typed inline callbacks, named callback factories, and `tree.on_event` use the same construction route. `entity!()` identifies the list. Cell attachment sees the list as its parent. The callback returns exactly one new root; existing roots and construction of multiple roots are rejected. Invalid construction, unwinding, and failed deferred command submission dispose the newly created subtree. Replacing the callback retires the old pool and constructs cells with the new callback. The request does not bubble, add input feedback, or consume business notification records; App listeners and `commands.emit` cannot produce it.

Construction callbacks use Model and value captures, event/entity roles, and deferred commands. They cannot retain `query!` or parameter `res!` borrows while creating nodes. Short resource operations use the same `res!(T).with(...)` expressions as composition, so the borrow ends before construction continues. Ordinary input and notification callbacks retain their typed query/resource capabilities.

Complex cells can call a `#[compose]` function with `item: model!(ListItem)` and a separate business Model. Pass the collection directly: `LazyList(items: todo.items(), item_extent: 40)`. The list subscribes to membership changes; `item.id()` supplies the current stable member identity, and `collection.get_by_id(id)` reads that member. Maps follow key order. Read the current ID inside properties and callbacks instead of capturing a member once during cell creation. Generated lists can bind a changing count with `items: ${data.count()}`.

Ancestor property updates precede retained-cell effects. Inactive or mismatched cells stop evaluating properties and accepting input until rebound. Index and ID update together. Rebinding cancels old pointer capture, focus, and pending input without cancelling an ancestor list's scroll drag. Appending unrelated members and editing member fields preserve a cell's current press. Persistent selection and editing state belong in the business Model, keyed by data identity rather than the recycled Entity.

Changing the source instance or switching between generated and collection-backed rows retires the old pool. Replace any creation callback that captured the old source as well; ordinary Rust captures are not rewritten. Collection sources must belong to the same App. A retired source cannot supply rows. Source conversion, subscription, callback replacement, and pool construction are cold operations; repeated cell rebinding reuses prepared storage.

First layout and resize settle cell creation, positions, and reactive properties before drawing and hit testing. List and cell snapshots retain their capacity; warmed scrolling, including hiding and restoring cells, adds no framework heap allocations. Callback installation, Model registration, and pool growth allocate during construction. User-owned formatted strings have separate allocation costs. Removing a list releases its cells, Model registrations, and owned effects.
