# figs_rs

A fast, declarative tool for composing paper figures and posters. You describe
**structure and constraints** in TOML; the engine computes every coordinate and
size for you, then renders to PNG/PDF. Where you need exact placement (panel
labels, insets, annotations), a `stack` lets you pin children at absolute offsets. Think
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
type = "column"       # column | row | stack | image | text | rect | line | arrow | ellipse | path
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
  `padding`, `x`, `y`. `margin`/`padding` accept a scalar (`1.5`) or a table
  (`{ top = 1, left = 2 }`). `x`/`y` are only read by a `stack` parent (see
  below); under `column`/`row` and on the root they are ignored.
- **Container** (`column`/`row`): `children`, `spacing`, `main_axis_alignment`
  (`start`/`center`/`end`/`space_between`/`space_around`/`space_evenly`),
  `cross_axis_alignment` (`start`/`center`/`end`/`stretch`).
- **stack**: `children` (plus the shared `padding`).
- **text**: `content`, `font_size` (pt), `font_family`, `font_weight`, `color`,
  `align`, `line_height` (multiple of font size).
- **image**: `src`, `fit` (`contain`/`cover`/`fill`).
- **rect**: `fill`, `stroke`, `stroke_width`, `corner_radius`.
- **line** / **arrow** / **path**: `points`, `stroke`, `stroke_width`, `dash`,
  `cap`, `head`, `head_size`; **path** adds `fill`, `closed`.
- **ellipse**: `fill`, `stroke`, `stroke_width`, `dash`, `cap`.

Lengths are in the page `unit`; `font_size` is always points. Internally
everything is converted to points (1 pt = 1/72 in).

### Overlays & absolute offsets (`stack`)

A `stack` lays its children on top of each other inside its content box (its
rect inset by `padding`). They paint in `children` order — later is on top — and
each child places its border box (after its margin) at offset `x`, `y` (page
units, default `0`) from the content box's top-left.

Sizing, per axis: an explicit `width`/`height` wins; otherwise an axis with **no
offset** (`x` unset for width, `y` unset for height) **fills** the content box,
and an axis with an offset keeps the child's natural (intrinsic) size. So a child
with no offset and no size is a full-size base layer, a label placed with both
`x` and `y` wraps its content, and `y = 3, height = 0.2` is a full-width band.
`aspect_ratio` counts a filled side as known and derives the other from it; if
both sides would fill, the child is the largest box of that ratio that fits,
anchored top-left. As in rows/columns, sizes are clamped to the space between the
offset and the far edge; negative offsets may hang off the top/left.

Measured as wrap-content (e.g. inside a start-aligned column), a stack is as
large as its furthest child extent (`offset + margins + size`) plus padding;
filling children contribute their intrinsic size there, like flex children do.

```toml
[nodes.panel]
type = "stack"
children = ["img", "label"]

[nodes.img]            # no x/y, no size -> fills the stack
type = "image"
src = "a.png"

[nodes.label]          # pinned 0.2 from the top-left, natural size
type = "text"
content = "(a)"
x = 0.2
y = 0.2
```

See [`examples/overlay_labels.figs`](examples/overlay_labels.figs).

### Vector shapes (annotations)

Four leaf nodes draw arrows, highlights and callouts:

| Type | Draws |
|---|---|
| `line` | A stroke between exactly two `points`. |
| `arrow` | A `line` with a filled arrowhead at the end (`head = "end"` by default). |
| `path` | A polyline through two or more `points`; `closed = true` makes it a polygon. Segments are straight (no Béziers — generate dense points in a script for curves). |
| `ellipse` | The ellipse inscribed in its box (a circle when the box is square). |

```toml
[nodes.pointer]
type = "arrow"
points = [[0, 0], [3, 1.5]]   # [x, y] pairs, page units
stroke = "#e0635a"
stroke_width = 0.05
dash = [0.2, 0.1]             # on/off lengths; omit for a solid stroke
```

- **Coordinates.** `points` are page units relative to the node's own content
  box (its rect inset by `padding`), and are **not** scaled when the box is
  resized. The intrinsic size of a `line`/`arrow`/`path` is the extent of its
  points from that origin (`max x`, `max y`, plus padding), so a shape placed with
  `x`/`y` in a `stack` gets a box sized to its points, while an unpositioned one
  fills the stack and its points are simply stack coordinates. Explicit
  `width`/`height` override the box (not the drawing). Negative coordinates may
  paint outside the box; shapes are not clipped. An `ellipse` has no intrinsic
  size — give it `width`/`height` or let a parent size it.
- **Stroke.** `stroke` is a color; `stroke_width` is in page units and defaults
  to 1 pt. `line`/`arrow` are stroked black unless `stroke` is set; a `path` or
  `ellipse` gets a black stroke only when it has neither `fill` nor `stroke`.
  `dash` is a list of alternating on/off lengths in page units (an odd list is
  repeated, as in SVG); `cap` is `butt` (default), `round` or `square`.
- **Arrowheads.** `head` is `none` / `start` / `end` / `both` (default `end` for
  `arrow`, `none` for `line`/`path`, so `path` + `head` gives a bent arrow).
  `head_size` is the head's length in page units (default 5 × the stroke width);
  its base is 0.8 × as wide. Heads are filled with the stroke color and stay solid
  on dashed strokes; closed paths have no heads.

See [`examples/annotations.figs`](examples/annotations.figs).

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
document* — it builds the node tree with `page` / `col` / `row` / `stack` / `text` /
`rect` / `image`, then flows through the identical resolve → layout → render pipeline. `let`
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

Build nodes with `text` / `rect` / `image` / `col` / `row` / `stack([...])` and the vector
shapes `line(x1, y1, x2, y2)` / `arrow(x1, y1, x2, y2)` / `ellipse()` /
`path([[x, y], …])`, set properties by
**chaining methods**, and finish with a single `page(width, height, root)` call.
(Rhai has no keyword arguments, so method chaining is how properties read like named
ones — no map literals.) Numbers may be integer or float; colors are hex strings. The
setters:

- **layout** (any node): `.flex(n)`, `.width(n)`, `.height(n)`, `.aspect(n)`,
  `.margin(n)`, `.pad(n)`.
- **offset** (read by a `stack` parent): `.at(x, y)`, `.x(n)`, `.y(n)` — e.g.
  `stack([image("a.png"), text("(a)").at(0.2, 0.2)])`.
- **text**: `.size(n)`, `.weight(n)` / `.bold()`, `.color(hex)`, `.font(name)`,
  `.align("left"|"center"|"right"|"justify")` / `.center()`, `.line_height(n)`.
- **container** (`col`/`row`): `.spacing(n)`, `.main("space_between"|…)`,
  `.cross("start"|"center"|"end"|"stretch")` / `.stretch()`.
- **rect**: `.fill(hex)`, `.stroke(hex)`, `.stroke_width(n)`, `.radius(n)`.
- **shapes** (`line`/`arrow`/`ellipse`/`path`): `.stroke(hex)`, `.stroke_width(n)`,
  `.fill(hex)` (path/ellipse), `.dash([on, off, …])`, `.cap("butt"|"round"|"square")`,
  `.head("none"|"start"|"end"|"both")`, `.head_size(n)`, `.closed()` (path) — e.g.
  `stack([image("a.png"), arrow(1, 4, 3, 2).stroke("#e0635a")])`.
- **image**: `.fit("contain"|"cover"|"fill")`.
- **page**: `.unit("cm"|…)`, `.background(hex)`, `.dpi(n)`, `.font(name)`.

Build it like any document (extension selects the front-end):

```sh
figs build poster.figs -o poster.png   # .figs / .rhai → script; .toml → TOML
```

### Framework (prelude)

Scripting has **two layers**:

- **Core** — the builders and setters above. Explicit, unopinionated, and
  1px-precise: nothing is implied, every size and gap is yours.
- **Framework** — a prelude of standard components with opinionated defaults for
  ordinary paper figures, so the common case is a few lines. It is plain Rhai
  ([`prelude.rhai`](crates/figs-core/src/script/prelude.rhai)) built *from* the
  core builders and merged into every script automatically (no `import`).

All syntactic sugar lives in the framework; the core stays strict. Every
framework component returns an ordinary core node or page, so you can always
chain core setters on it (`grid(2, xs).pad(0.5)`, `title("T").size(30)`) or drop
down to `col`/`row`/`page` wherever the defaults don't fit.

A labeled 2×2 figure for a two-column journal
([`examples/framework_demo.figs`](examples/framework_demo.figs)):

```rhai
page("double", grid(2, panels([
    image("a.png"), image("b.png"),
    image("c.png"), image("d.png"),
])))
```

Components (lengths in cm, font sizes in pt):

| Function | Result |
|---|---|
| `page(preset, root)` | Page of a named size, unit `cm`: `"single"` (8.5 cm column), `"double"` (17.5 cm), `"a5"`, `"a4"`, `"a3"`, `"a0"` (poster), `"letter"`; append `-landscape` to swap sides. Journal widths get a 4:3 height. The core `page(w, h, root)` is unchanged. |
| `paper_size(preset)` | `[w, h]` of a preset, to drop down to core: `let s = paper_size("double"); page(s[0], 5, root).unit("cm")`. |
| `title(s)` / `caption(s)` / `label(s)` / `body(s)` | Semantic text: 1.5× base bold centered / base centered / 1.25× base bold / base. |
| `vbox(items)` / `hbox(items)` | `col`/`row` with the default gap, stretched cross axis, and bare strings allowed. |
| `grid(ncols, items)` / `grid(ncols, gap, items)` | Rows of `ncols` equal-flex cells (rows share height equally); the last row is padded with empty cells so widths stay equal. |
| `panels(items)` / `panels(style, items)` | Each item overlaid with an auto label, `label_inset()` in from its `label_corner()` (via core `stack`). `style` is a template whose last `a`/`A`/`1` is the counter: `"(a)"` (default), `"a"`, `"A"`, `"a."`, `"Fig. 1"`. |
| `panels(items, captions)` / `panels(style, items, captions)` | The same labeled panels, each with its caption below (`captioned`). `captions` must match `items` in length. |
| `labeled(item, s)` / `panel_label(style, i)` | One labeled panel / the `i`-th (0-based) label string. |
| `circle(r)` / `circle(cx, cy, r)` | A square `ellipse` of radius `r`; the 3-argument form is centered on (`cx`, `cy`) of the enclosing `stack` (via `.at`). |
| `captioned(item, c)` | `item` above caption `c` (a string becomes `caption(c)`, a node is used as is), `caption_gap()` apart; the item takes the spare height. |
| `nodes(items)` / `as_node(x)` | Bare-string sugar: a string becomes `body(s)`. Use `col(nodes([...]))` to get it in core containers. |

**Bare strings.** Inside framework containers (`vbox`, `hbox`, `grid`, `panels`)
a string in the children array is a text node: `vbox(["Results", image("a.png")])`.
Core `col`/`row` deliberately still require nodes (a stray string there is an
error), and Rhai can't shadow a native function with a script one without
recursing into itself — so the bridge is explicit: `col(nodes([...]))`.

**Defaults are hooks.** The defaults are prelude functions; define one with the
same name in your script to override it everywhere:

```rhai
fn base_size() { 9 }        // pt; all semantic text scales from it (default 8)
fn gap() { 0.3 }            // cm between siblings in framework containers (default 0.2)
fn label_style() { "A" }    // panel labels A, B, C … (default "(a)")
fn label_inset() { 0.3 }    // cm from the panel corner (default base_size() * 0.02 = 0.16)
fn label_corner() { "top-right" }   // or "bottom-left" / "bottom-right" (default "top-left")
fn label_background() { "#ffffff" } // box behind the label (default "": none)
fn label_pad() { 0.1 }      // cm between label text and its box (default base_size() * 0.01)
fn caption_gap() { 0.15 }   // cm between a panel and its caption (default gap() / 2)
```

The label and caption defaults derive from `base_size()` / `gap()`, so
overriding those rescales them too. The inset keeps labels off the panel edge;
the caption gap is half the grid gap so a caption reads as belonging to the panel
above it rather than the one below.

**Labeled & captioned panels.** A labeled panel is a core `stack`: the item
fills it and the label is overlaid. Stack offsets measure from the top-left, so
a top-left label is simply `label(s).at(inset, inset)`; for the other corners
the label sits in a column that fills the stack (no offset, no size), padded by
the inset and aligned with `.main("end")` / `.cross("end")` as needed. With
`label_background()` set, the label becomes a small stack of a filled `rect`
under the text (padded via the text's margin). All of it is ordinary nodes, so
you can restyle or replace any piece. Captions go below with
`panels(items, captions)`
([`examples/captioned_panels.figs`](examples/captioned_panels.figs)):

```rhai
fn label_background() { "#ffffff" }   // readable over dark micrographs

page("double", grid(2, panels(
    [image("a.png"), image("b.png")],
    ["As-grown film", "After annealing"],
)))
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
  axis. Overflow clamps and warns — it never panics. A `stack` instead places
  each child at its `x`/`y` offset, filling un-offset, unsized axes.

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
