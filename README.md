# figs_rs

A fast, declarative tool for composing paper figures and posters. You describe
**structure and constraints** in TOML; the engine computes every coordinate and
size for you (no manual positioning), then renders to PNG/PDF. Think
Flutter/XAML-style relative layout, aimed at research figures.

> Status: **Phase 1 in progress.** The layout engine core (parse → resolve →
> layout → IR) is implemented and tested. Text shaping, PNG/PDF renderers, the
> CLI watcher and the egui live-preview window are landing milestone by
> milestone (see [the plan](#milestones)).

## Why

Making figures by hand-placing images in PowerPoint is slow and not reusable.
Python/matplotlib is slow to start, HTML is verbose, and Typst can't be edited
visually. figs_rs aims for: sub-second startup, reusable `.toml` templates, an
automatic layout engine, and (Phase 2) a WYSIWYG editor that round-trips back to
TOML.

## Schema at a glance

Nodes are a **flat, id-keyed map**; the tree lives in `children` id arrays. This
keeps documents shallow, makes reorder/reparent a one-line edit, and gives every
node a stable id for the future visual editor.

```toml
[page]
width = 20
height = 20
unit = "cm"          # cm | mm | in | pt | px
root = "root"
background = "#ffffff"

[nodes.root]
type = "column"       # column | row | image | text | rect
spacing = 1
padding = 1
cross_axis_alignment = "stretch"
children = ["top", "bottom"]

[nodes.top]
type = "row"
flex = 1              # weight for sharing free space
children = ["a", "b"]
# ... each node listed flatly
```

You provide: hierarchy, style, content, and constraints (`flex`, `aspect_ratio`,
`width`/`height`). The engine computes: coordinates, sizes, spacing.

### Properties

- **Shared** (all nodes): `flex`, `width`, `height`, `aspect_ratio`, `margin`,
  `padding`. `margin`/`padding` accept a scalar (`1.5`) or a table
  (`{ top = 1, left = 2 }`).
- **Container** (`column`/`row`): `children`, `spacing`, `main_axis_alignment`
  (`start`/`center`/`end`/`space_between`/`space_around`/`space_evenly`),
  `cross_axis_alignment` (`start`/`center`/`end`/`stretch`).
- **text**: `content`, `font_size` (pt), `font_family`, `font_weight`, `color`,
  `align`, `line_height` (multiple of font size).
- **image**: `src`, `fit` (`contain`/`cover`/`fill`).
- **rect**: `fill`, `stroke`, `stroke_width`, `corner_radius`.

Lengths are in the page `unit`; `font_size` is always points. Internally
everything is converted to points (1 pt = 1/72 in).

### Equal spacing & edge-to-edge

To place panels at **equal intervals** in a `row`/`column`, set
`main_axis_alignment` to a distribution mode and give the children **fixed
sizes** so the container has leftover space to spread:

- `space_between` — equal gaps between panels, none at the ends.
- `space_around` — equal gaps around each panel (half-gap at the ends).
- `space_evenly` — equal gaps everywhere, including the ends.

```toml
[nodes.row]
type = "row"
main_axis_alignment = "space_evenly"
children = ["a", "b"]

[nodes.a]
type = "image"
src = "a.png"
width = 8          # fixed size leaves slack for the gaps
```

Caveat: distribution only acts on *leftover* space. If a child uses `flex` it
eats the slack and `space_*` has no visible effect — in that case use `flex = 1`
on each child plus a `spacing` value, which already yields equal-size panels with
equal gaps.

For **edge-to-edge** figures, simply omit `padding`/`margin` on the root (and any
outer container): both default to zero, so content reaches the page border. The
GUI's starter document no longer adds an outer inset. See
[`examples/grid_4panel.toml`](examples/grid_4panel.toml) for both at once.

## Scripting with Rhai (`.figs`)

For variables, arithmetic and reusable components, author a
**[Rhai](https://rhai.rs) script** instead of TOML. A script *evaluates to the same
document* — it builds the node tree with `page` / `col` / `row` / `text` / `rect` /
`image`, then flows through the identical resolve → layout → render pipeline. `let`
defines variables, `fn` defines components, and arithmetic is native:

```rhai
let accent = "#e0635a";
let base   = 16;                       // base font size (pt)

fn captioned(src, caption) {           // a reusable component
    col([
        image(src).aspect(1.0),
        text(caption).size(14).center(),
    ]).flex(1).spacing(0.5)
}

page(20, 12,
    col([
        text("Results").size(base * 2).bold().color(accent),
        captioned("fig.png", "(a) Result"),
    ]).pad(1.0).spacing(0.5).cross("center"),
).unit("cm").background("#ffffff")
```

Build nodes with `text` / `rect` / `image` / `col` / `row`, set properties by
**chaining methods**, and finish with a single `page(width, height, root)` call.
(Rhai has no keyword arguments, so method chaining is how properties read like named
ones — no map literals.) Numbers may be integer or float; colors are hex strings. The
setters:

- **layout** (any node): `.flex(n)`, `.width(n)`, `.height(n)`, `.aspect(n)`,
  `.margin(n)`, `.pad(n)`.
- **text**: `.size(n)`, `.weight(n)` / `.bold()`, `.color(hex)`, `.font(name)`,
  `.align("left"|"center"|"right"|"justify")` / `.center()`, `.line_height(n)`.
- **container** (`col`/`row`): `.spacing(n)`, `.main("space_between"|…)`,
  `.cross("start"|"center"|"end"|"stretch")` / `.stretch()`.
- **rect**: `.fill(hex)`, `.stroke(hex)`, `.stroke_width(n)`, `.radius(n)`.
- **image**: `.fit("contain"|"cover"|"fill")`.
- **page**: `.unit("cm"|…)`, `.background(hex)`, `.dpi(n)`, `.font(name)`.

Build it like any document (extension selects the front-end):

```sh
figs build poster.figs -o poster.png   # .figs / .rhai → script; .toml → TOML
```

In the editor (`figs-editor`), the **Script** tab is the source of truth: press
**Evaluate** (Ctrl+Enter) to lower the script into the editable **TOML** tab, the
inspector and the live preview. Authoring flows Rhai → (TOML ↔ GUI) one way; use
*Export TOML* to write out the lowered document.

## Architecture

```
poster.toml                        figure.figs (Rhai)
  → schema::parse   (TOML → RawDocument)   → script::eval_script (→ RawDocument)
  → schema::resolve (validate ids/cycles, convert units → typed Document)
  → layout          (two-pass measure/arrange → ComputedLayout IR)
  → renderers       (PNG via tiny-skia, PDF via printpdf)   [upcoming]
```

`figs-core` is I/O-free beyond fonts/images so the Phase 2 Tauri backend can
reuse it verbatim. The `ComputedLayout` IR is the single contract shared by both
renderers and the editor.

## Layout

Modeled on Flutter's `BoxConstraints`/`RenderFlex`:

- **measure** (bottom-up): each node's wrap-content size; flex children
  contribute their intrinsic size.
- **arrange** (top-down): fixed children take their measured size, flex children
  split the remainder by weight; alignment distributes any leftover;
  `aspect_ratio` derives the cross axis from the (possibly flex-imposed) main
  axis. Overflow clamps and warns — it never panics.

## Build & test

```sh
cargo test          # unit + layout integration tests
cargo clippy --all-targets
```

## Milestones

M0 workspace · **M1 schema+resolve** ✓ · **M2 layout (column/row/rect, flex,
align, aspect, overflow)** ✓ · M3 text (cosmic-text) · M4 PNG backend · M5 image
+ aspect/fit · M6 PDF backend · M7 CLI + file watch · M7′ egui live preview ·
M8 hardening.

## License

MIT OR Apache-2.0.
