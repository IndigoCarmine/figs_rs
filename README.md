# figs_rs

A fast, declarative tool for composing paper figures and posters. You describe
**structure and constraints** in TOML; the engine computes every coordinate and
size for you, then renders to PNG, PDF or SVG. Where you need exact placement (panel
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
  `padding`, `x`, `y`, `anchor`. `margin`/`padding` accept a scalar (`1.5`) or a table
  (`{ top = 1, left = 2 }`). `x`/`y` are only read by a `stack` parent (see
  below); under `column`/`row` and on the root they are ignored.
- **Container** (`column`/`row`): `children`, `spacing`, `main_axis_alignment`
  (`start`/`center`/`end`/`space_between`/`space_around`/`space_evenly`),
  `cross_axis_alignment` (`start`/`center`/`end`/`stretch`).
- **stack**: `children`, `viewbox` (plus the shared `padding`).
- **text**: `content`, `font_size` (pt), `font_family`, `font_weight`, `color`,
  `align`, `line_height` (multiple of font size).
- **image**: `src`, `fit` (`contain`/`cover`/`fill`).
- **rect**: `fill`, `stroke`, `stroke_width`, `corner_radius`.
- **line** / **arrow** / **path**: `points`, `segments` (curves), `viewbox`, `stroke`,
  `stroke_width`, `dash`, `cap`, `head`, `head_size`; **path** adds `fill`,
  `closed`.
- **ellipse**: `fill`, `stroke`, `stroke_width`, `dash`, `cap`.

Lengths are in the page `unit`; `font_size` is always points. Internally
everything is converted to points (1 pt = 1/72 in).

### Auto page height

`height = "auto"` makes the page as tall as its content (the width stays fixed).
It is an explicit opt-in — `height` is still required, and a number keeps the
old fixed behaviour. In Rhai: `page(17.5, "auto", root)`.

```toml
[page]
width = 17.5
height = "auto"      # fit the content
unit = "cm"
root = "root"
```

The root is measured with the page width fixed and the height unbounded, the
measured height becomes the page height, then layout proceeds as usual. Along
that unbounded vertical axis:

- **flex is ignored in content-sized columns.** There is no free space to share,
  so each child — flex or not — takes its measured (intrinsic) height, like
  Flutter flex children in an unbounded axis. A grid of equal-flex rows thus gets
  rows sized by their content; a sizeless `flex` rect is 0 tall. Rows and stacks
  inside still use flex for widths.
- **widths are decided as usual** (stretch / flex shares), and `aspect_ratio`
  derives each height from its width: 2×2 square images on a 17.5 cm page with a
  0.2 cm gap give a page `2 × 8.65 + 0.2` cm tall.
- **images without `aspect_ratio`** use the file's natural aspect (read from the
  image header by the CLI and the editor), or **4:3** when that is unknown (e.g. a
  missing file drawn as a placeholder) — so an image never adds zero height once
  its width is known.
- if the content still has no height, the page falls back to 4:3 (width × 3/4)
  with a warning. Layout never panics.

Renderers and the editor read the final height from the layout IR
(`ComputedLayout.page.height_pt`, with `page.auto_height` set). In the editor,
the page inspector's **auto** checkbox toggles it.

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

### User coordinates (`viewbox`) & anchors

`viewbox = [x0, y0, x1, y1]` lays a coordinate system of your own over a node's
content box: `x0` maps to the left edge, `x1` to the right, `y0` to the top and
`y1` to the bottom, linearly — reverse a pair to flip that axis
(`[0, 1, 10, 0]` is a plot's "x 0..10, y 0..1 upwards"). The mapping is done at
layout time, so it follows whatever size the layout gives the node.

- On a **stack**, the children's `x`/`y` offsets are in viewbox units.
- On a **line** / **arrow** / **path**, `points` and `segments` (control points,
  arc radii) are. Such a shape has no intrinsic size; unpositioned in a stack it
  fills it, so its viewbox spans the stack.
- Only **positions** map. Sizes (`width`, `height`), strokes, dashes and
  arrowheads stay in the page unit, so a 2 mm marker stays round and a 1 pt line
  stays 1 pt however the data is stretched. `x0 == x1` or `y0 == y1` is an error.

`anchor = [ax, ay]` (fractions of the child's own box, default `[0, 0]`) picks
which point of a stack child sits at its `x`/`y`: `[0.5, 0.5]` centres it,
`[1, 0.5]` right-aligns it to the point. An anchored child — or any child of a
viewbox stack — sits *around* its point rather than hanging from it, so it is
not shrunk to the space after its offset; it keeps its size and may extend past
any edge. A viewbox stack's intrinsic size ignores the
(not yet mappable) offsets: it wraps its largest child.

```toml
[nodes.area]
type = "stack"
viewbox = [0, 1, 10, 0]        # x 0..10, y 0..1 upwards
children = ["curve", "dot"]

[nodes.curve]                  # fills the stack; points in data units
type = "path"
viewbox = [0, 1, 10, 0]
points = [[0, 0], [5, 0.8], [10, 0.6]]

[nodes.dot]                    # a marker 0.2 page units wide, centred on (5, 0.8)
type = "ellipse"
width = 0.2
height = 0.2
x = 5
y = 0.8
anchor = [0.5, 0.5]
```

In scripts: `.viewbox(x0, y0, x1, y1)` and `.anchor(ax, ay)`. The framework's
[plots](#plots) are built entirely from these.

### Vector shapes (annotations)

Four leaf nodes draw arrows, highlights and callouts:

| Type | Draws |
|---|---|
| `line` | A stroke between exactly two `points`. |
| `arrow` | A `line` with a filled arrowhead at the end (`head = "end"` by default). |
| `path` | A polyline through two or more `points`, optionally continued by curved [`segments`](#curves); `closed = true` makes it a polygon. |
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

### Curves

`points` is the straight-polyline shorthand. To add curves, list further
`segments` that **continue from the last of `points`** (so a curve needs at least
one point as its start). Each segment ends at `to`, with at most one curve key
(SVG-path semantics):

```toml
[nodes.wave]
type = "path"
points = [[0, 0]]                                 # start point
segments = [
  { to = [2, 0] },                                # straight
  { quad = [3, -1], to = [4, 0] },                # quadratic Bézier: one control point
  { cubic = [[5, 1], [6, -1]], to = [7, 0] },     # cubic Bézier: two control points
  { arc = 1, to = [9, 0] },                       # circular arc, radius 1
  { arc = [2, 1], rotation = 30, large = true, sweep = "ccw", to = [9, 3] },
]
```

- **Arcs** use SVG's endpoint form: `arc` is the radius (`r`, or `[rx, ry]` with
  `rotation` in degrees for an ellipse); of the arcs of that size joining the two
  points, `large = true` takes the one spanning more than 180° and `sweep` the
  turning direction as seen on the page, `"cw"` (default) or `"ccw"`. A radius
  too small to reach `to` is scaled up (so `arc = 0.001` is always a semicircle).
  Arcs are converted to cubic Béziers at resolve time, so every renderer only
  draws lines and Béziers.
- **Validation.** `large`/`sweep`/`rotation` need `arc`; radii must be positive;
  unknown keys are errors. Without `segments`, `line`/`arrow` take exactly two
  points; with them, a `line`/`arrow` is still **one segment** — one point plus one
  `segments` entry — so a curved arrow is `type = "arrow"`, `points = [[0, 0]]`,
  `segments = [{ quad = [2, -1], to = [4, 0] }]`. A `path` takes any mix;
  `closed` joins the end back to the first point with a straight line.
- **Size.** The intrinsic size uses the exact extent of the curves (Bézier
  extrema), not their control points.
- **Arrowheads** point along the curve's tangent at the tip (from the last
  distinct control point), and the shaft is shortened along the curve into the
  head. Dashes follow the curve.

In scripts, curve builders continue a `path`, each appending a segment from the
current end (`line(…)`/`arrow(…)` already hold two points, so a curved arrow is
`path([[x, y]]).quad(…).head("end")`, or the prelude's `curved_arrow`):

```rhai
path([[0, 0]])
    .to(2, 0)                          // straight
    .quad(3, -1, 4, 0)                 // control, end
    .cubic(5, 1, 6, -1, 7, 0)          // control 1, control 2, end
    .arc(1, 9, 0)                      // radius, end (small, clockwise)
    .arc([2, 1], 9, 3, #{ large: true, sweep: "ccw", rotation: 30 })
```

The prelude adds sugar on top (positive `bend`/`depth` goes left of the direction
of travel — upwards for a left-to-right chord):

| Function | Result |
|---|---|
| `curve(x1, y1, x2, y2, bend)` | A quadratic `path` whose midpoint sits `bend` off the straight chord. |
| `curved_arrow(x1, y1, x2, y2, bend)` | `curve(...)` with a head at the end. |
| `brace(x1, y1, x2, y2, depth)` | A curly brace with its ends on the two points and its tip `depth` off the chord. |
| `bracket(x1, y1, x2, y2, depth)` | A square bracket whose back is `depth` off the chord. |

See [`examples/curves.figs`](examples/curves.figs).

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
- **user coordinates**: `.viewbox(x0, y0, x1, y1)` (stack/line/arrow/path) and
  `.anchor(ax, ay)` (read by a `stack` parent) — see
  [User coordinates](#user-coordinates-viewbox--anchors).
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
- **curves** (continue a path from its last point): `.to(x, y)`,
  `.quad(cx, cy, x, y)`, `.cubic(c1x, c1y, c2x, c2y, x, y)`, `.arc(r, x, y)` /
  `.arc(r, x, y, #{ sweep, large, rotation })` (`r` a number or `[rx, ry]`) —
  see [Curves](#curves).
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

A labeled 2×2 figure for a two-column journal — the page is 17.5 cm wide and
exactly as tall as the four panels need
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
| `page(preset, root)` | Page of a named size, unit `cm`. Journal presets `"single"` (8.5 cm column) and `"double"` (17.5 cm) fix only the width and get an [auto height](#auto-page-height) that fits the content. Paper presets `"a5"`, `"a4"`, `"a3"`, `"a0"` (poster), `"letter"` are fixed; append `-landscape` to swap their sides (journal presets have no landscape form). The core `page(w, h, root)` is unchanged. |
| `paper_size(preset)` | `[w, h]` of a preset, to drop down to core: `let s = paper_size("double"); page(s[0], 5, root).unit("cm")`. For journal presets `h` is the string `"auto"`, so `page(s[0], s[1], root)` stays auto. |
| `title(s)` / `caption(s)` / `label(s)` / `body(s)` | Semantic text: 1.5× base bold centered / base centered / 1.25× base bold / base. |
| `vbox(items)` / `hbox(items)` | `col`/`row` with the default gap, stretched cross axis, and bare strings allowed. |
| `grid(ncols, items)` / `grid(ncols, gap, items)` | Rows of `ncols` equal-flex cells (rows share height equally); the last row is padded with empty cells so widths stay equal. |
| `panels(items)` / `panels(style, items)` | Each item overlaid with an auto label, `label_inset()` in from its `label_corner()` (via core `stack`). `style` is a template whose last `a`/`A`/`1` is the counter: `"(a)"` (default), `"a"`, `"A"`, `"a."`, `"Fig. 1"`. |
| `panels(items, captions)` / `panels(style, items, captions)` | The same labeled panels, each with its caption below (`captioned`). `captions` must match `items` in length. |
| `labeled(item, s)` / `panel_label(style, i)` | One labeled panel / the `i`-th (0-based) label string. |
| `circle(r)` / `circle(cx, cy, r)` | A square `ellipse` of radius `r`; the 3-argument form is centered on (`cx`, `cy`) of the enclosing `stack` (via `.at`). |
| `curve` / `curved_arrow` / `brace` / `bracket` `(x1, y1, x2, y2, bend_or_depth)` | Curved annotations built on the core curve builders — see [Curves](#curves). |
| `annotate(item, marks)` | `stack([item, ..marks])`: overlays annotation nodes (an array, or one node) on a panel; coordinates inside are cm from the panel's top-left. Composes with labels: `panels([annotate(..), ..])`, `labeled(annotate(..), "(a)")`. |
| `scalebar(len, s)` | A `len` cm bar (white, `scalebar_thickness()` thick) with label `s` centered above it (`""` for none), `scalebar_inset()` in from `scalebar_corner()` (default bottom-right). |
| `scalebar(len, field, s)` | The same bar in image/physical units: `len` out of a field `field` wide (same unit), so exactly `len / field` of the panel width whatever the layout — `scalebar(5, 50, "5 μm")` for a 50 μm field, `scalebar(80, 1024, "1 μm")` for 80 px of a 1024 px image. |
| `callout(tx, ty, x, y, s)` | The core `arrow(tx, ty, x, y)` in `accent()` with label `s` at its tail, beside it on the side away from the target and vertically centered on `ty`. |
| `highlight(x, y, w, h)` / `spot(cx, cy, r)` | Dashed box with its top-left at (`x`, `y`) / dashed circle centered on (`cx`, `cy`), in `accent()` (a closed `path` / an `ellipse`; `.dash([])` makes them solid). |
| `pin(node, corner)` / `pin(node, corner, inset)` | Any node pinned `inset` (default `label_inset()`) in from a corner of the enclosing stack — what `labeled` and `scalebar` use. |
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
fn accent() { "#ffd400" }   // callout / highlight / spot colour (default "#ff5a3c")
fn annotation_width() { 0.05 }      // their stroke, cm (default base_size() * 0.005)
fn annotation_dash() { [0.2, 0.1] } // highlight/spot dash, cm (default 3x / 2x the width)
fn scalebar_color() { "#000000" }   // bar + label (default "#ffffff")
fn scalebar_thickness() { 0.1 }     // cm (default base_size() * 0.01 = 0.08)
fn scalebar_corner() { "bottom-left" } // (default "bottom-right")
fn scalebar_inset() { 0.3 }         // cm from the corner (default label_inset())
fn scalebar_halo() { "" }           // outline + label shadow colour (default "#000000a0")
fn scalebar_frac_inset() { 0.05 }   // side inset of scalebar(len, field, s), fraction of width (default 0.025)
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

**Annotations.** `annotate(item, marks)` turns a panel into a core `stack` with
the item at the bottom and each mark overlaid, so one line adds scale bars,
callouts and highlights; wrap it in `panels`/`labeled` and the label goes on top
([`examples/annotated_figure.figs`](examples/annotated_figure.figs)):

```rhai
fn label_background() { "#ffffff" }   // labels readable on dark micrographs

page("double", grid(2, panels([
    annotate(image("a.png"), [
        scalebar(2, "500 nm"),                          // 2 cm bar, bottom-right
        callout(3.3, 1.3, 4.6, 2.6, "grain boundary"),  // arrow from label to target
        spot(5.2, 3.2, 0.7),                            // dashed circle
    ]),
    annotate(image("b.png"), [
        scalebar(5, 50, "5 μm"),         // 5 μm of a 50 μm wide field
        highlight(4.6, 1.2, 2.6, 2.0),   // dashed box
    ]),
])))
```

Every mark is ordinary core nodes (a `rect` bar and `text` in a pinned column,
an `arrow` + `text`, a closed `path`, an `ellipse`), so core setters still chain
— `spot(..).stroke("#00ff00")`, `highlight(..).dash([])`. The defaults are
opinionated for micrographs: the scale bar is white (TEM/SEM/fluorescence
images are mostly dark), with a thin translucent-black outline and a drop shadow
under its label (`scalebar_halo()`) so it stays legible over bright regions
without covering the image with a box; callouts, highlights and spots use a warm
red-orange `accent()` that reads on both light and dark backgrounds, with a
stroke tied to `base_size()`. Notes:

- `scalebar(len, s)` takes `len` in cm. When you only know the image's field of
  view, use `scalebar(len, field, s)`: the bar is laid out as flex shares of the
  panel width, so it is exact however the layout sizes the panel — but only if
  the image spans the whole panel width (its natural aspect, as on auto-height
  pages; not letterboxed by `contain` in a fixed cell, nor cropped by `cover`).
  Because that form is all fractions, its side inset is `scalebar_frac_inset()`
  of the width rather than cm, and a label wider than the bar plus its insets
  wraps — keep it short.
- `callout` measures nothing: (`tx`, `ty`) is where the arrow starts, and the
  label is placed beside that point (right-aligned to it when the target is to
  the right, else left-aligned), centered on a box of about three font sizes, so
  a label sitting right at the panel's bottom edge may drift.

**Errors point at your script.** An error raised inside the framework is
reported at *your* call into it, named after the function you called — never at
a line of `prelude.rhai`:

```text
line 3, col 5: grid: ncols must be a positive integer (got 0)
in call to function 'figure' (line 5, col 16)
```

If the call is inside one of your own functions, the position is that inner
line and the trailing lines trace your calls outward.

### Plots

Small data plots — the kind a paper figure needs next to its micrographs — are
part of the framework, so no matplotlib round trip: `plot(opts, series)` returns
an ordinary node (it takes `.flex`, `.height`, and goes in `grid`/`panels` like
an image), and `csv(path)` loads the data
([`examples/plot.figs`](examples/plot.figs)):

```rhai
let d = csv("data/growth.csv");        // #{ time_h: [..], wt: [..], … }

page("double", grid(2, panels([
    plot(#{ xlabel: "Time (h)", ylabel: "OD₆₀₀" }, [
        line_plot(d.time_h, d.wt, #{ label: "Wild type", yerr: d.wt_sd }),
        scatter(d.time_h, d.mutant, "ΔrpoS"),     // a string is the legend label
    ]),
    plot(#{ ylabel: "Doubling time (min)" }, [
        bars(["WT", "ΔrpoS", "ΔrelA", "ΔspoT"], [24, 38, 31, 45],
             #{ yerr: [1.5, 3.2, 2.1, 4.0] }),
    ]),
])))
```

| Function | Result |
|---|---|
| `plot(series)` / `plot(opts, series)` | Axes of whatever size the layout gives (default aspect `plot_aspect()` = 4:3, so plots get a height on auto pages), holding `series` (an array, or one series). |
| `line_plot(xs, ys)` / `line_plot(xs, ys, opts)` | A line through the points; `NaN`/missing values break it. |
| `scatter(xs, ys)` / `scatter(xs, ys, opts)` | Markers (`"o"` circles by default). |
| `bars(xs, ys)` / `bars(xs, ys, opts)` | Bars from 0. `xs` are numbers or **category names** (a categorical axis); several bar series are grouped side by side. Error bars on bars are black. |
| `errorbars(xs, ys, errs)` / `(…, opts)` | Error bars alone (y ± err, black by default). |
| `legend(series)` | The legend as a standalone node (e.g. beside a row of plots). |
| `csv(path)` | A map of columns: numbers when every non-empty cell parses (an empty cell is `NaN`), else strings. Comma- or tab-separated (TSV when the header has tabs and no commas), RFC 4180 quotes, `#` comment lines skipped. Relative paths resolve against the script's directory (the CLI and editor pass it), like image `src`. |

**Series options** (a map, or a string = `label`): `label` (legend entry),
`color` (default: next `palette()` colour, in order), `width` (cm), `dash`,
`marker` (`"o"`, `"s"`, `"^"`, `"d"`, `"none"`), `size` (marker diameter, cm),
`yerr` (symmetric error per point).

**Plot options**: `x` / `y` ranges `[lo, hi]` (default: the data plus a 2 %
margin — not past 0 — widened to whole ticks), `xlabel` / `ylabel`, `xscale` /
`yscale` (`"linear"` or `"log"`), `xticks` / `yticks` (explicit values),
`legend` (a corner, `"auto"`, or `""`/`false`), `frame` (`"open"`, `"box"`,
`"none"`), and `marks`: extra nodes overlaid on the data area, placed with
`.at(x, y)` in data units (log10 of the value on a log axis) — e.g.
`text("p < 0.05").at(2, 0.9).anchor(0.5, 1)`.

Defaults are journal-style and overridable hooks: open L-shaped axes, outward
ticks, labels at `base_size()`, the y label horizontal above the axis (text is
never rotated), and a legend without a box in the emptiest corner.

```rhai
fn plot_line_width() { 0.02 }   // axes + ticks, cm (default base_size() * 0.0035 ≈ 0.8 pt)
fn series_width() { 0.05 }      // data lines, cm (default 1.75 × plot_line_width())
fn palette() { ["#000000", "#d55e00"] }  // series colours (default Okabe–Ito, colour-blind safe)
fn tick_length() { 0.1 }        // cm (default base_size() * 0.015)
fn max_ticks() { 5 }            // labelled ticks per axis, steps 1/2/5 × 10^n (default 7)
fn marker_size() { 0.2 }        // marker diameter, cm (default base_size() * 0.02)
fn plot_frame() { "box" }       // "open" (default) / "box" / "none"
fn legend_corner() { "top-left" }  // "auto" (default) / a corner / "" for none
fn bar_width() { 0.8 }          // fraction of the bar spacing (default 0.7)
fn plot_aspect() { 1.0 }        // width / height of a plot panel; 0 = size it yourself (default 4:3)
fn plot_label_room() { 0 }      // band kept free at the top for a panel label, cm
```

How it maps data: the plot area is a core `stack` with
`.viewbox(xlo, yhi, xhi, ylo)`; lines and bars are `path`s with the same
viewbox, markers, ticks and error-bar caps are children placed `.at(x, y)` in
data units and centred with `.anchor`, and tick labels sit in narrow viewbox
stacks beside the area. The layout performs the mapping at the plot's final
size, so plots stretch, flex and auto-size like any other node — and every
piece is a plain node you could have written by hand. Ticks are "nice"
1/2/5 × 10ⁿ steps with at most `max_ticks()` labels and shared decimals
(`0.0, 0.5, 1.0`, real minus signs); log axes tick each decade (labelled
`0.01 … 1000`, or `10⁻⁵`-style past that) with minor ticks at 2–9.
Data outside an explicit range is clipped (lines are cut at the frame,
markers outside are dropped). Notes: the space for tick labels below the axis
and the label overhang on the right are estimated from `base_size()`; error
bars are vertical only.

In the editor (`figs-editor`), the **Script** tab is the source of truth: press
**Evaluate** (Ctrl+Enter) to lower the script into the editable **TOML** tab, the
inspector and the live preview. Authoring flows Rhai → (TOML ↔ GUI) one way; use
*Export TOML* to write out the lowered document. `figs-editor figure.figs` opens
a file directly and reloads it when it changes on disk (see
[Watch mode & live reload](#watch-mode--live-reload)).

## Architecture

```
poster.toml                        figure.figs (Rhai)
  → schema::parse   (TOML → RawDocument)   → script::eval_script (→ RawDocument)
  → schema::resolve (validate ids/cycles, convert units → typed Document)
  → layout          (two-pass measure/arrange → ComputedLayout IR)
  → renderers       (PNG via tiny-skia, PDF via krilla, SVG hand-written)
```

`figs-core` is I/O-free beyond fonts/images (and the `csv()` data files a script reads) so the Phase 2 Tauri backend can
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
- **tight constraints**: when a container's size on an axis is already known, a
  stretched cross axis or a flex share is handed to the child during measure
  too, so `aspect_ratio` children report their real height (this is what sizes
  an auto-height page).
- **images under loose constraints**: an image with neither side known (no
  `width`/`height`, no stretch or flex share — e.g. inside a `cross("center")`
  column) takes the largest box of its aspect that fits the space it is offered:
  the full available width, or the full available height when that is the
  tighter limit (like CSS `img { max-width: 100% }` / Flutter's `AspectRatio`).
  The aspect is `aspect_ratio`, else the file's natural aspect, else 4:3. An
  unbounded axis (auto page height) doesn't limit; it never yields an infinite
  size. Several such images side by side in a `row` each claim the full width
  and overflow — give them `flex` or a size.
- **text boxes**: a text block is its stacked line boxes (`font_size ×
  line_height` each, glyphs centred per CSS half-leading), grown at the top and
  bottom when the font's ascent/descent is taller than the line box (e.g. Segoe
  UI at the default 1.2), so accents and descenders never poke out and get
  clipped at a page edge. Spacing between lines is unchanged. Glyphs may still
  sit flush with the box edge — the box is the font's metrics, with no extra
  padding; add `spacing`/`pad` for breathing room.

## Output formats

The output extension picks the backend — no flags:

```sh
figs build figure.figs -o figure.png   # raster, at the page `dpi`
figs build figure.figs -o figure.pdf   # vector, page = exact page size in pt
figs build figure.figs -o figure.svg   # vector, width/height in pt
```

Any other extension is an error. The editor has matching **Export PNG / PDF /
SVG** buttons. All three backends are driven by the same `ComputedLayout` walk
and share one geometry module (arrowheads, rounded corners, ellipses, text
alignment, image fitting), so they place everything at identical coordinates;
the vector formats keep that exactness at any zoom.

| | PNG | PDF | SVG |
|---|---|---|---|
| Shapes | anti-aliased raster | vector paths (dash, cap, alpha) | `<rect>`/`<ellipse>`/`<path>` |
| Text | rasterized glyphs | **real text**: fonts embedded as subsets, ToUnicode map (selectable, searchable) | **glyph outlines** (`<use>` of per-glyph paths in `<defs>`), exact on every viewer with no font dependency; not selectable (`aria-label` carries the text) |
| Images | resampled | JPEG passed through untouched; other formats embedded losslessly | base64 `data:` URI (PNG/JPEG bytes verbatim) |
| `cover` | clipped | clip path | `clipPath` |
| Missing image | gray placeholder | same placeholder | same placeholder |

- Coordinates are points throughout. The IR's top-left origin is kept in SVG
  (`viewBox` in pt); PDF's bottom-left flip is handled by krilla, and the
  MediaBox is exactly `page.width × page.height` pt.
- `dpi` only affects PNG.
- Text in PDF/SVG uses the exact glyph positions shaped for layout, so line
  breaks and kerning match the PNG. Bitmap-only glyphs (colour emoji) have no
  outline and are skipped in SVG.
- PDF support is the `pdf` cargo feature of `figs-core` (on by default; it pulls
  in krilla). PNG and SVG are always built.

## Watch mode & live reload

Keep your text editor open and see the figure update on every save:

```sh
figs watch figure.figs -o figure.png    # any of .png / .pdf / .svg, as for build
```

`watch` builds once, then rebuilds whenever the input **or anything it reads**
changes — every image `src`, and data files a script loads. It prints one line
per build, keeps watching through errors (a script error shows its line/col), and
exits cleanly on Ctrl+C:

```text
ok  wrote figure.png (151282 bytes) at 300 dpi, auto height 373.46 pt in 25 ms
watching figure.figs and 4 dependencies (Ctrl+C to stop)
changed: figure.figs
error: parsing `figure.figs`: Syntax error: Unexpected ',' (line 4, position 23)
changed: figure.figs
ok  wrote figure.png (151282 bytes) at 300 dpi, auto height 373.46 pt in 22 ms
changed: a.png
ok  wrote figure.png (151228 bytes) at 300 dpi, auto height 373.46 pt in 37 ms
```

Directories are watched rather than files, so editors that save by writing a
temp file and renaming it over the original (VS Code, vim, …) are handled, and an
image that doesn't exist yet is picked up once it appears in its folder.

The editor does the same: `figs-editor figure.figs` opens a script in the
**Script** tab and evaluates it (a `.toml` opens in the **TOML** tab); relative
image paths resolve against the file's folder. When the file changes on disk it
reloads and re-renders automatically — unless you have unsaved edits, in which
case a banner offers **Reload** / **Keep mine** instead of overwriting them.
Changed images refresh the preview too.

The dependency list comes from `figs_core::Document::dependencies(base_dir)`
(absolute, deduplicated paths). A script built-in that reads a file registers it
with one call, `figs_core::deps::record(path)`; the watcher API is
`figs_core::watch::FileWatcher` (cargo feature `watch`).

## Build & test

```sh
cargo test          # unit + layout integration tests
cargo clippy --all-targets
```

## Milestones

M0 workspace · **M1 schema+resolve** ✓ · **M2 layout (column/row/rect, flex,
align, aspect, overflow)** ✓ · M3 text (cosmic-text) · M4 PNG backend · M5 image
+ aspect/fit · **M6 PDF + SVG backends** ✓ · **M7 CLI + file watch (`figs watch`,
editor live reload)** ✓ · M7′ egui live preview ·
M8 hardening.

## License

MIT OR Apache-2.0.
