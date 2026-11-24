# Reuse a component and its styles

Give repeated controls consistent spacing and let a completed row pick its own colors.

## Work through the code {#source}

1. Use `#[compose]` to give a reusable filter control a typed Model and a display prop. Callers can pass a literal label or a reactive value to `prop!(String)`; the control binds it once at the display site.
2. Register shared `task`, `task.complete`, and `filter` rules with `StyleSheetPlugin`. Select classes from reactive state so each completed row changes independently. Inherited typography stays on the container.

<<< @/examples/first-app/src/interface.rs#filter-button{rust}

## Run and check {#run}

Run this in the repository checkout for the current documentation version.

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 4
```

Toggle a row and compare its appearance with the rows that remain open.

<TutorialDemo :chapter="4" />

A direct widget field has priority over a class field. Keep shared color choices in one place when you want the class to control them.
