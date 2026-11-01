# Navigation paths

## Following widget bounds

Attach `FollowBounds` to a widget to position it around another widget. It uses prepared `LayoutGeometry` outputs, including scrolling and transforms, and maps the target into the follower parent's coordinates. The follower does not occupy flex space; its children retain ordinary layout and padding.

```rust
use mirui::prelude::*;
use mirui::ui::widgets::FollowBounds;

#[compose]
fn selection() {
    ui! {
        View (width: 240, height: 120) {
            View (id: "selected", width: 80, height: 40, bg_color: Color::rgb(30, 70, 120))
            View (border_width: 2, border_color: Color::rgb(80, 160, 240)) [
                FollowBounds::new(id("selected")).with_outset(3),
            ]
        }
    };
}

let mut app = App::headless(240, 120);
app.with_default_widgets();
let root = app.spawn_root().id();
app.compose(root, selection());
app.render().unwrap();
```

`with_content(true)` selects the target's content box after resolved padding. `with_outset(value)` expands the mapped rectangle; negative values inset it. `with_offset(x, y)` translates it. Both use the follower parent's logical coordinates. Rotation maps to an axis-aligned bounding rectangle rather than matching an arbitrary outline.

The follower respects its own ancestor clips and the target's ancestor clip. Hidden, deleted, detached, cyclic, or unmappable targets make the follower unavailable for drawing, input, and output reads until the relation becomes valid. Self-targets and targets inside the follower subtree are cyclic. Removing `FollowBounds` restores the authored `Style`; the component never overwrites that style. Use `follow_bounds::view()` for selective widget registration, or `with_default_widgets()` to include it automatically.

Bounds update immediately by default. Add `.with_transition(mirui::anim::SNAPPY)` or pass a `TweenConfig` to animate position and size changes. The first valid placement and recovery from an unavailable target are immediate. Retargeting preserves each axis's displayed position; springs also retain velocity. Rendering and hit testing use the displayed bounds, clipped to the current source viewport. `transition: None` or zero duration switches immediately. Only animated followers allocate motion component storage, prepared when animation is enabled; advancing and retargeting reuse it.

## Interactive example

Page Studio is available in the Gallery's Showcase section as `page_studio`. Its collection, place details and note editor demonstrate stack navigation and retained controls; the scene panel demonstrates `PageView`. The toolbar separates None/Slide/Fade effects from Tween/Spring motion. Tween offers Linear, Ease out and Ease in-out at 300 ms; Spring offers Smooth, Snappy, Bouncy and Interactive. The Follow Bounds panel moves an independent outline between three differently sized buttons using `FollowBounds`; each selection uses the toolbar's current motion and preset.

```sh
cargo run -p gallery --example page_studio_demo
cargo run -p gallery --example page_studio_demo --no-default-features --features wgpu
```

Nearby demonstrates a local game launcher with favorites, platform and game browsers, search and connection flows. Press and release X to launch a game, or hold X for multiplayer. Nested overlays provide game actions, settings, system controls, tools, exit and power confirmation. Settings include local sound, theme, volume and brightness state; unavailable actions show their reason. The simulation controls provide a Launcher button without replacing the shared game collection. Page changes use explicit forward and backward slide directions. The native window starts at 640×480 and reflows when resized. Device, network and game-process operations are simulated. The default backend is SDL; the same UI also runs with SDL-GPU or WGPU.

Nearby starts with the forest palette; settings switch between forest, sunset and night. Pixel scenery uses the same theme roles. Initial viewport height selects native 12 px or 24 px typography and compact spacing; Flex handles subsequent window resizing. Tab switches favorites and catalog, F1 opens settings or toggles the search keyboard, and Space opens the connection flow. Y sorts favorites, opens catalog search, edits favorites in game results, or deletes a search character. Connection choice pages use Y to switch themes. F2 toggles the simulation controls above the application.

Connection flows opened from the launcher use an animated panel while retaining the browser below. Closing the panel restores the same collection and selection. Modal backgrounds retain completed widget output within the demo's shared offscreen budget.

Discovery shows nearby rooms above the connection options. Down at the end of the room list expands the options; Up from the options restores the rooms. Clicking either pane activates it. The local mock advances room resolution into the matching-game picker with Next; choosing a game continues the connection. Entry in the F2 controls switches between launcher and direct in-game entry.

X starts the selected game on release, or opens its multiplayer flow automatically when held. Navigation cancels an unfinished press; holding X through a modal or page change cannot launch the new selection.

```sh
cargo run -p gallery --example nearby_demo
cargo run -p gallery --example nearby_demo --no-default-features --features sdl-gpu
cargo run -p gallery --example nearby_demo --no-default-features --features wgpu
```

## Navigation state

`NavigationPath<R>` is an observable Model containing pushed destinations. The root page is not part of the path. Register an instance with `App::add_model` or store it in an owned Model field.

```rust
use mirui::app::App;
use mirui::ui::widgets::navigation::NavigationPath;

#[derive(Clone)]
enum Route {
    Devices,
    Device(u32),
}

let mut app = App::headless(320, 240);
let path = app
    .add_model(NavigationPath::<Route>::with_capacity(8))
    .finish();

path.push(Route::Devices).unwrap();
path.push(Route::Device(7)).unwrap();
assert_eq!(path.len(), 2);
path.pop().unwrap();
assert_eq!(path.len(), 1);
path.clear().unwrap();
assert!(path.is_empty());
```

`len()`, `is_empty()` and `revision()` are observable. Successful structural changes publish Model observations, including derived values on an owning parent Model. Removing from an empty path does not publish a change.

`with_capacity(n)` prepares storage for at most `n` destinations. `push` moves the destination into prepared storage; it does not grow storage or clone the route. Equal destinations are separate entries. `pop` and `clear` retain capacity. A rejected mutation leaves the path unchanged.

## Retained navigation stacks

`NavigationStack` binds a path to a page factory. Its ordinary children are the root page; the factory handles every pushed destination. Register `navigation::view::<Route>()` once per route type, not once per page. Multiple independent navigation areas use separate path instances.

```rust
use mirui::prelude::*;
use mirui::ui::widgets::{Button, Text};
use mirui::ui::widgets::navigation::{self, CreatePage, NavigationPath, NavigationStack, PageTransition};
use mirui::anim::SMOOTH;

#[derive(Clone, Copy)]
enum Route { Devices, Device(u32), Settings }

#[compose]
fn shell(path: model!(NavigationPath<Route>)) -> Entity {
    ui! {
        NavigationStack(path: path.clone(), grow: 1.0,
            transition: PageTransition::slide(SMOOTH)) {
            Column(grow: 1.0) {
                Text("Home")
                Button(height: 40) [Text::label("Devices")]
                on Tap { path.push(Route::Devices).unwrap(); }
                Button(height: 40) [Text::label("Settings")]
                on Tap { path.push(Route::Settings).unwrap(); }
            }
        } on CreatePage(create_page::callback(path))
    }
}

#[event]
fn create_page(event: event!(CreatePage<Route>), path: model!(NavigationPath<Route>)) -> Entity {
    match *event.route() {
        Route::Devices => ui! { devices(path) },
        Route::Device(id) => ui! { detail(path, id) },
        Route::Settings => ui! { settings(path) },
    }
}

#[compose]
fn devices(path: model!(NavigationPath<Route>)) -> Entity {
    ui! {
        Column(grow: 1.0) {
            Text("Devices")
            Button(height: 40) [Text::label("Device 7")]
            on Tap { path.push(Route::Device(7)).unwrap(); }
            back(path)
        }
    }
}

#[compose]
fn detail(path: model!(NavigationPath<Route>), id: u32) -> Entity {
    ui! {
        Column(grow: 1.0) {
            Text(text: format!("Device {id}"))
            back(path)
        }
    }
}

#[compose]
fn settings(path: model!(NavigationPath<Route>)) -> Entity {
    ui! { Column(grow: 1.0) { Text("Settings") back(path) } }
}

#[compose]
fn back(path: model!(NavigationPath<Route>)) -> Entity {
    ui! {
        Button(height: 40) [Text::label("Back")]
        on Tap { path.pop().unwrap(); }
    }
}

fn main() {
    let mut app = App::headless(320, 240);
    app.with_default_widgets();
    app.with_widget(navigation::view::<Route>());
    let root = app.spawn_root().id();
    let path = app.add_model(NavigationPath::with_capacity(8)).finish();
    app.compose(root, shell(path.clone()));
    app.render().unwrap();
    path.push(Route::Devices).unwrap();
    app.render().unwrap();
    path.push(Route::Device(7)).unwrap();
    app.render().unwrap();
    path.pop().unwrap();
    app.render().unwrap();
}
```

Returning to a visited entry restores its existing widgets and scroll position. Hidden pages do not receive input; their reactive effects resume when shown. App-level systems remain installed. Removed entries release their page subtrees. An initial deep path constructs only its visible destination; an unvisited ancestor is constructed when first shown. A path can drive only one live stack; a second binding warns and remains on its root page.

Page creation may allocate, including cloning an owned route payload. Path mutation and unchanged-page coordination reuse prepared framework storage. A failed factory preserves the displayed page without retrying every frame; the next path revision or factory replacement permits another attempt. The application root and shared Models are not replaced by navigation.

## Pages selected by application state

`PageView` follows an application value without keeping a history. Register its View once for each route type, then provide a `CreatePage` callback. The callback returns one newly constructed page root. Equal page values retain the existing subtree; a different value constructs its replacement before retiring the old subtree. Failed construction leaves the displayed page intact and is not retried until the selection or factory changes.

```rust
use mirui::prelude::*;
use mirui::ui::widgets::Text;
use mirui::ui::widgets::page_view::{self, CreatePage, PageView};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page { Home, Settings }

#[model]
struct Session {
    #[observe]
    page: Page,
}

#[model]
impl Session {
    fn show_settings(&mut self) { self.page = Page::Settings; }
}

#[event]
fn create_page(event: event!(CreatePage<Page>)) -> Entity {
    match event.route() {
        Page::Home => ui! { Text("Home", grow: 1.0) },
        Page::Settings => ui! { Text("Settings", grow: 1.0) },
    }
}

#[compose]
fn panel(session: model!(Session)) -> Entity {
    ui! {
        PageView(page: ${ session.page() }, grow: 1.0)
        on CreatePage(create_page::callback())
    }
}

fn main() {
    let mut app = App::headless(320, 240);
    app.with_default_widgets();
    app.with_widget(page_view::view::<Page>());
    let root = app.spawn_root().id();
    let session = app.add_model(Session { page: Page::Home }).finish();
    app.compose(root, panel(session.clone()));
    app.render().unwrap();
    session.show_settings();
    app.render().unwrap();
}
```

`PageView::new(page)` supports non-macro composition. Its `page` field stores `Option<R>`; `None` selects no page, and ordinary DSL field conversion accepts an `R` directly. Routes need `Clone + PartialEq`, not `Default`. Page construction can allocate; an unchanged selection does not clone its route or rebuild its widgets. The factory also accepts the ordinary inline closure and `on CreatePage { ... }` forms.

## Page transitions

```rust
use mirui::anim::{BOUNCY, Easing, Transition, TweenConfig, ease};
use mirui::types::Fixed;
use mirui::ui::widgets::navigation::{PageDirection, PageTransition};

const MOVE: TweenConfig = TweenConfig::new(300, &ease::OUT_CUBIC);
let slide = PageTransition::slide(MOVE);
let returning = PageTransition::slide(MOVE).with_direction(PageDirection::Backward);
let fade = PageTransition::fade(BOUNCY);

fn custom_progress(t: Fixed) -> Fixed { t * t }
static CUSTOM: Easing = Easing::new(custom_progress);
let config: Transition = TweenConfig::new(240, &CUSTOM).into();
let mut motion = config.motion(Fixed::ZERO, Fixed::ONE);
motion.tick(16);
config.retarget(&mut motion, Fixed::from_int(2));
```

`transition: ${ expression }` binds the transition configuration reactively on either container. Changes apply to subsequent transitions; active motion keeps its current configuration and velocity.

`FollowBounds::with_transition` and `PageTransition::slide` / `fade` accept the shared `anim::Transition` configuration. `Transition::Tween(TweenConfig)` and `Transition::Spring(SpringConfig)` select the motion algorithm; either configuration converts directly. `TweenConfig::new(300, &ease::OUT_CUBIC)` selects a 300 ms tween; `SMOOTH`, `SNAPPY`, `BOUNCY`, and `INTERACTIVE` are spring presets. The configuration creates and redirects existing `Motion` state; page effects determine how its value is applied to the page subtrees.

Both containers accept `transition: PageTransition::slide(config)` or `PageTransition::fade(config)`, using either tween or spring configuration. The default is `PageTransition::None`; first mount and configurations with zero duration switch immediately. Stack pushes and returns automatically slide in opposite directions. `PageView` defaults to forward movement without inferring navigation history.

Use `PageTransition::slide(config).with_direction(PageDirection::Backward)` to enter from the left and move the current page to the right; `Forward` does the opposite. The explicit direction overrides the container's automatic direction. Bind the complete configuration through the existing `transition: ${state.transition()}` property and update it with the page in one Model operation. Direction changes alone do not replace a page or alter an active pair. Returning to either member of that pair redirects the existing motion; a pending third page uses the latest configuration when its transition starts. Fade and None ignore direction.

Repeated targets with unchanged configuration do not restart motion. Tween redirection starts at the displayed value over the full configured duration, without inheriting velocity. Spring redirection preserves position and velocity. Changing algorithms starts the selected motion at the displayed value. Page containers retain the active transition's configuration through reversal; property changes apply to the next transition. `FollowBounds` applies configuration changes to its current motion. Loop and ping-pong modes remain available for ordinary animations, not finite transitions.

Transitions finish when the shared `Motion` completes: at the tween's duration or when the spring settles. Reversing the active pair redirects its motion without rebuilding either page. Other changes retain only the latest destination until the pair finishes. Both transitioning pages reject input; the selected page resumes input when the transition completes. Page-local transforms remain intact, and slide offsets follow the current container size. Slide retains overshoot within the clipped viewport; fade clamps only the composited opacity.

Slide requires no pixel buffers. Fade composites complete subtrees using the offscreen pool configured through `app.with_offscreen_pool_budget(bytes)`. Two W × H RGBA buffers at scale 1 require 8 × W × H pixel bytes, in addition to other retained outputs and metadata. Insufficient storage or unsupported output composition logs a warning and completes the page switch without animation; it does not increase the budget.

Sliding a page retains unchanged local offscreen content. Moving a cached child inside another capture invalidates the containing capture, while edits to the child's content invalidate both. Dirty-region queries retain pending damage without rebuilding unchanged textures.
