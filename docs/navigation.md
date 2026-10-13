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
