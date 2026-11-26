---
pageClass: tutorial-page
aside: false
title: Common patterns
---

# Common patterns

Four everyday UI building blocks—reactive thresholds, flex sizing, spring-driven selection rings, and custom vector views—each running live beside its source.

---

## Reactive state & threshold styling {#reactive}

Drive a numeric readout, a percentage bar width, and a threshold color from one observed integer field without manual callbacks.

<MiruiSnippet recipe="reactive" label="Model" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-reactive{rust}

<template #exercise>

Tap `+ 10` past `80 %` to watch the fill color switch automatically, or tap `42` to reset.

</template>

</MiruiSnippet>

Marking `level` with `#[observe]` generates the `gauge.level()` getter. Both `width: ${ Dimension::percent(gauge.level()) }` and `bg_color: ${ ... }` subscribe to that field and recompute only when `gauge.step(...)` mutates it.

---

## Flex sizing & space distribution {#layout}

Toggle a primary card between compact and expanded widths while its sibling claims the remaining row space with `grow: 1.0`.

<MiruiSnippet recipe="layout" label="Flexbox" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-layout{rust}

<template #exercise>

Tap `Toggle width`, or drag the circular grip at the bottom-right corner to resize the viewport and see `grow: 1.0` in action.

</template>

</MiruiSnippet>

Explicit dimensions (`Dimension::px`) coexist with flex distribution (`grow: 1.0`). When the reactive width changes, layout invalidates only the affected row.

---

## Spring selection rings (`FollowBounds`) {#spring}

Float a selection frame above a row of variable-width buttons so rapid taps retarget mid-flight while preserving velocity.

<MiruiSnippet recipe="spring" label="FollowBounds" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-spring{rust}

<template #exercise>

Tap quickly across `MCU`, `Desktop`, and `Web`—the outline reverses and resizes in mid-air without snapping.

</template>

</MiruiSnippet>

`FollowBounds` maps the target entity's layout rectangle into the follower's parent space. Updating `follower.target = target` inside an `#[event]` handler hands the new destination to the `INTERACTIVE` spring.

---

## Custom vector widgets (`#[view]`) {#vector}

Author a custom widget tag for `ui!` that strokes static vector paths scaled to the widget's layout box.

<MiruiSnippet recipe="vector" label="#[view]" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-vector{rust}

<template #exercise>

Tap any of the three tiles to cycle its stroke width (`2 px → 3 px → 4 px`) and color.

</template>

</MiruiSnippet>

`path!(...)` builds a zero-allocation static `Path`. `#[view(component = VectorGlyph)]` exposes `VectorGlyph` directly inside `ui!` and scales its `100 × 100` local coordinates to `paint.rect()` during rendering.
