# Navigation paths {#navigation-paths}

<div class="arch-diagram" role="img" aria-label="Comparison of mirui's three navigation and motion primitives: FollowBounds, NavigationStack, and PageView">
  <svg viewBox="0 0 880 168" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="nav-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="20" width="268" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="32" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">01 · FOLLOWBOUNDS</text>
    <text x="32" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">Layout-Delta Spring &amp; Tween</text>
    <text x="32" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">FollowBounds::new(target)</text>
    <text x="32" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">with_outset · with_transition</text>
    <text x="32" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">Velocity-preserving mid-flight retarget</text>
    <rect x="306" y="20" width="268" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="322" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">02 · NAVIGATIONSTACK&lt;R&gt;</text>
    <text x="322" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">Retained Push / Pop Lifecycle</text>
    <text x="322" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">NavigationPath::with_capacity</text>
    <text x="322" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">push · pop · clear · CreatePage</text>
    <text x="322" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">Restores hidden subtrees &amp; scroll state</text>
    <rect x="596" y="20" width="268" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="612" y="46" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="11" font-weight="600">03 · PAGEVIEW&lt;P&gt;</text>
    <text x="612" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">State-Driven Active Page</text>
    <text x="612" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">PageView(page: ${state.page()})</text>
    <text x="612" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">PageTransition::slide / fade</text>
    <text x="612" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">Reversible &amp; zero-copy slide</text>
  </svg>
</div>

## Following widget bounds {#following-widget-bounds}

Attach `FollowBounds` to a widget to position it around another widget. It uses prepared `LayoutGeometry` outputs, including scrolling and transforms, and maps the target into the follower parent's coordinates. The follower does not occupy flex space; its children retain ordinary layout and padding.

<MiruiSnippet recipe="spring" label="FollowBounds" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-spring{rust}

<template #exercise>

Tap across `MCU`, `Desktop`, and `Web` to retarget `FollowBounds` mid-flight with the `INTERACTIVE` spring.

</template>

</MiruiSnippet>

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

### Simulating proximity in tests {#simulating-proximity-in-tests}

Gallery's **Nearby** (`nearby_demo`) showcases `FollowBounds`, directional `PageView` deck slides, and retained modal overlays in a local handheld game launcher, paired with a deterministic proximity/network simulation inspector (`F2` or **Simulate** above the Web canvas):

| Simulation Control / Flow | Shortcut / Trigger | Behavior & State Contract |
| --- | --- | --- |
| **Toggle Simulation Inspector** | `F2` / Web `Simulate` | Expands a scrollable 300 px inspector to the left of the 640×480 launcher without altering application state |
| **Connection Flows & 9 Case Presets** | `J` or hold `X` / Inspector | Starts game creation/joining, local matching, LAN, or direct entry across 9 presets (success, connection/restore failure, existing/no rooms, reconnecting, remote closure, no match, unsupported) |
| **Step & Local Confirmation** | `Deliver reply` / `Local A` / `Local B` | `Deliver reply` advances one remote operation with an expected-result hint; `Local A` / `Local B` drive host approval and start confirmation through the active route |
| **State-Gated Fault Injection** | Fault buttons / `Repeat case` / `Stop case` | Injects failure, timeout, host rejection, remote departure, connection loss, or restore results; `Repeat case` restarts the scenario and `Stop case` restores the retained launcher |
| **Navigation & Deck Shortcuts** | `Tab` / `K` / `Y` / `X` | `Tab` switches favorites & retained catalog; `K` opens settings/search keyboard; `Y` sorts/searches/switches themes; `X` launches on release or opens multiplayer on hold (cancelled if navigation intervenes) |

Nearby starts with the forest palette (switching between forest, sunset, and night in settings) and plays ten bundled sound cues across navigation and connection transitions. Connection panels animate over the retained browser below, caching modal backgrounds within the shared offscreen budget, and returning from tools or completed simulations restores the exact catalog position and selection.

## Interactive example {#interactive-example}

Page Studio is available in the Gallery's Showcase section as `page_studio`. Its collection, place details and note editor demonstrate stack navigation and retained controls; the scene panel demonstrates `PageView`. The toolbar separates None/Slide/Fade effects from Tween/Spring motion. Tween offers Linear, Ease out and Ease in-out at 300 ms; Spring offers Smooth, Snappy, Bouncy and Interactive. The Follow Bounds panel moves an independent outline between three differently sized buttons using `FollowBounds`; each selection uses the toolbar's current motion and preset.

```sh
cargo run -p gallery --example page_studio_demo
cargo run -p gallery --example page_studio_demo --no-default-features --features wgpu
```

Run Nearby with SDL, SDL-GPU, or WGPU:

```sh
cargo run -p gallery --example nearby_demo
cargo run -p gallery --example nearby_demo --no-default-features --features sdl-gpu
cargo run -p gallery --example nearby_demo --no-default-features --features wgpu
```

## Navigation state {#navigation-state}

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

## Retained navigation stacks {#retained-navigation-stacks}

`NavigationStack` binds a path to a page factory. Its ordinary children are the root page; the factory handles every pushed destination. Register `navigation::view::<Route>()` once per route type, not once per page. Multiple independent navigation areas use separate path instances.

<MiruiSnippet recipe="nav-stack" label="NavigationStack" file="recipes.rs" height="220px">

<template #exercise>

Click `Push Devices →`, then `Inspect #7 →` to push two routes onto `NavigationPath`, and use `← Pop one` or `Clear root` to slide back while retaining the root page's state.

</template>

</MiruiSnippet>

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

## Pages selected by application state {#pages-selected-by-application-state}

`PageView` follows an application value without keeping a history. Register its View once for each route type, then provide a `CreatePage` callback. The callback returns one newly constructed page root. Equal page values retain the existing subtree; a different value constructs its replacement before retiring the old subtree. Failed construction leaves the displayed page intact and is not retried until the selection or factory changes.

<MiruiSnippet recipe="nav-pageview" label="PageView" file="recipes.rs" height="210px">

<template #exercise>

Click `Overview`, `Metrics`, or `Logs` to switch `session.page()` and drive `PageView` with a `SNAPPY` slide transition.

</template>

</MiruiSnippet>

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

## Page transitions {#page-transitions}

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

Followed bounds reconcile through the registered View during layout preparation, before drawing and input. Position and clipping changes reuse the prepared layout; size changes settle through the existing bounded layout loop. Inactive relation plans release their storage, and hidden relations retain an empty plan without repeated allocation.

Both containers accept `transition: PageTransition::slide(config)` or `PageTransition::fade(config)`, using either tween or spring configuration. The default is `PageTransition::None`; first mount and configurations with zero duration switch immediately. Stack pushes and returns automatically slide in opposite directions. `PageView` defaults to forward movement without inferring navigation history.

Use `PageTransition::slide(config).with_direction(PageDirection::Backward)` to enter from the left and move the current page to the right; `Forward` does the opposite. The explicit direction overrides the container's automatic direction. Bind the complete configuration through the existing `transition: ${state.transition()}` property and update it with the page in one Model operation. Direction changes alone do not replace a page or alter an active pair. Returning to either member of that pair redirects the existing motion; a pending third page uses the latest configuration when its transition starts. Fade and None ignore direction.

Repeated targets with unchanged configuration do not restart motion. Tween redirection starts at the displayed value over the full configured duration, without inheriting velocity. Spring redirection preserves position and velocity. Changing algorithms starts the selected motion at the displayed value. Page containers retain the active transition's configuration through reversal; property changes apply to the next transition. `FollowBounds` applies configuration changes to its current motion. Loop and ping-pong modes remain available for ordinary animations, not finite transitions.

Transitions finish when the shared `Motion` completes: at the tween's duration or when the spring settles. Reversing the active pair redirects its motion without rebuilding either page. Other changes retain only the latest destination until the pair finishes. Both transitioning pages reject input; the selected page resumes input when the transition completes. Page-local transforms remain intact, and slide offsets follow the current container size. Slide retains overshoot within the clipped viewport; fade clamps only the composited opacity.

Slide requires no pixel buffers. Fade composites complete subtrees using the offscreen pool configured through `app.with_offscreen_pool_budget(bytes)`. Two W × H RGBA buffers at scale 1 require 8 × W × H pixel bytes, in addition to other retained outputs and metadata. Insufficient storage or unsupported output composition logs a warning and completes the page switch without animation; it does not increase the budget.

Sliding a page retains unchanged local offscreen content. Moving a cached child inside another capture invalidates the containing capture, while edits to the child's content invalidate both. Dirty-region queries retain pending damage without rebuilding unchanged textures.
