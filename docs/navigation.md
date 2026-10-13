# Navigation paths

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
