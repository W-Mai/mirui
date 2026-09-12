# Typography

mirui shapes text into positioned glyphs, then optionally places those glyphs on a registered vector path. The same `Path` and `PathId` types drive drawing, clipping, hit testing, and text baselines.

## Text on a static path

`path!` stores its commands in the program image. Registering those commands with `insert_static` keeps the path borrowed and gives it a stable `PathId`.

```rust
use mirui::prelude::*;

static ARC: Path = path!(M 16 72 Q 96 8 176 72);

fn register_arc(app: &mut App<impl Surface>) -> PathId {
    app.paths()
        .insert_static(ARC.commands())
        .expect("path capacity")
}

#[compose]
fn curved_title(arc: PathId) {
    ui! {
        Text("Status", path: arc, font_size: 24)
    };
}
```

Passing a `PathId` uses subpath `0`, starts at distance `0`, follows the path forward, and uses the full available length.

## Place multiple lines

Each paragraph line uses one consecutive subpath, beginning at `TextPath::subpath()`. The usable length of each subpath constrains wrapping, alignment, justification, and ellipsis before glyph placement.

```rust
use mirui::prelude::*;

static LINES: Path = path!(
    M 12 36 L 172 36
    M 24 68 Q 92 104 160 68
);

fn register_lines(app: &mut App<impl Surface>) -> PathId {
    app.paths()
        .insert_static(LINES.commands())
        .expect("path capacity")
}

#[compose]
fn multiline_label(lines: PathId) {
    ui! {
        Text(
            "The first line stays straight and the next follows the curve.",
            path: lines,
            paragraph: ParagraphStyle {
                wrap: TextWrap::Word,
                max_lines: Some(2),
                ..ParagraphStyle::default()
            },
        )
    };
}
```

No baseline array or copied command buffer is created. Line-only subpaths are sampled directly from `Path`; curved subpaths reuse the bounded measurement cache. A missing or invalid subpath rejects the path layout before placed glyph or caret frames are published.

## Select a range and direction

`TextPath` describes how text consumes a registered path without copying its commands.

```rust
use mirui::prelude::*;

let placement = TextPath::new(path)
    .with_subpath(1)
    .with_range(Fixed::from_int(12)..Fixed::from_int(180))
    .with_direction(PathDirection::Reverse)
    .with_seam(Fixed::from_int(36));

ui! {
    Text("Reverse around the ring", path: placement)
};
```

`with_seam` chooses the distance treated as the start of a closed subpath. The selected range remains measured in path-length units after the seam is applied.

The builder API accepts the same value:

```rust
let label = Text::build("Status")
    .font_size(24)
    .path(placement)
    .spawn(cx.world_mut());
```

## Switch paths reactively

The DSL accepts a `Signal<PathId>` as a reactive `path` attribute. Replacing the signal value moves the existing text entity to the new path and transfers its path-change subscription.

```rust
#[compose]
fn selectable_path(first: PathId, second: PathId) {
    let selected = Signal::new(first);
    let selection = selected.clone();

    ui! {
        Column () {
            Text("Live route", path: $selection)
            View (text: "Switch", width: 80, height: 36) on Tap {
                selected.update(|path| {
                    *path = if *path == first { second } else { first };
                });
            }
        }
    };
}
```

## Derive path geometry from signals

Create one mutable path, reserve its command storage once, and bind its geometry to signals. `bind_path` installs an owner-bound effect: signal changes edit the existing path, advance its revision, and dirty every subscribed text or drawing entity.

```rust
#[compose]
fn elastic_baseline(amplitude: Signal<Fixed>) {
    let mut geometry = Path::try_with_capacity(2).expect("path storage");
    geometry
        .move_to(Point::new(0, 48))
        .quad_to(Point::new(80, 48), Point::new(160, 48));
    let path = cx.paths().insert(geometry).expect("path capacity");
    let live_amplitude = amplitude.clone();

    cx.bind_path(path, move |geometry| {
        let y = Fixed::from_int(48) - live_amplitude.get();
        geometry
            .set_command(1, mirui::render::path::PathCmd::QuadTo {
                ctrl: Point::new(80, y),
                end: Point::new(160, 48),
            })
            .expect("stable command shape");
    })
    .expect("mutable path");

    ui! {
        Text("Signal-driven curve", path: path)
    };
}
```

The effect retains the path allocation and command capacity. Prefer `set_command` when the command topology is stable; use `clear` plus path-building methods when the topology itself changes.

## Edit a path from a handler

Event handlers receive the same scoped path access API. Editing by `PathId` preserves entity identity and updates all consumers of the path.

```rust
ui! {
    Text("Tap to bend", path: path, height: 72) on Tap {
        let _ = ctx.paths().edit(path, |geometry| {
            geometry
                .set_command(1, mirui::render::path::PathCmd::QuadTo {
                    ctrl: Point::new(80, 4),
                    end: Point::new(160, 48),
                })
                .expect("stable command shape");
        });
    }
};
```

## Storage and invalidation

- `PathId` is a generational handle; removing a path invalidates stale handles.
- Static entries borrow command slices and reject edits.
- Mutable entries own or adopt their command storage and expose a monotonically advancing revision.
- Text entities subscribe to the path selected by their `TextPath` component.
- Baseline sampling, posed glyphs, caret frames, ink bounds, and hit testing are cached by path identity and revision.
- Path edits recompute affected line lengths and invalidate placement through the same path revision.
- Ordinary text without a `TextPath` stays on the linear shaping and rendering path.

Set the registry limit before inserting paths when the application needs a fixed budget:

```rust
app.with_path_capacity(24)?;
```
