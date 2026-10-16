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
  `padding`, `x`, `y`, `anchor`, `rotate` (see [Rotation](#rotation)), and the
  visual effects `shadow`, `glow`, `opacity` (see
  [Shadows, glows & opacity](#shadows-glows--opacity)). `margin`/`padding` accept a scalar (`1.5`) or a table
  (`{ top = 1, left = 2 }`). `x`/`y` are only read by a `stack` parent (see
  below); under `column`/`row` and on the root they are ignored.
- **Container** (`column`/`row`): `children`, `spacing`, `main_axis_alignment`
  (`start`/`center`/`end`/`space_between`/`space_around`/`space_evenly`),
  `cross_axis_alignment` (`start`/`center`/`end`/`stretch`).
- **stack**: `children`, `viewbox` (plus the shared `padding`).
- **Decoration** (`column`/`row`/`stack`): `fill` (colour or gradient),
  `stroke`, `stroke_width`, `corner_radius` (alias `radius`), `dash`, `clip` —
  a background and border painted behind the children (see
  [Gradients & frames](#gradients--frames)).
- **text**: `content`, `font_size` (pt), `font_family` (an ordered fallback
  list: `"Arial, Yu Gothic"` or `["Arial", "Yu Gothic"]`), `font_weight`,
  `font_style` (`normal`/`italic`), `color`, `align`, `line_height` (multiple
  of font size), `letter_spacing` (em), `underline`, `strike`, inline `spans`
  (see [Rich text](#rich-text-spans)), and an outline: `stroke`,
  `stroke_width`, `join` (see [Text outline](#text-outline)).
- **image**: `src`, `fit` (`contain`/`cover`/`fill`), `smooth` (`false`:
  nearest-neighbour scaling, see [Pixelated images](#pixelated-images)).
- **rect**: `fill`, `stroke`, `stroke_width`, `corner_radius`.
  Every `fill` (rect, ellipse, path, containers) and `page.background` takes a
  colour *or a gradient* (see [Gradients & frames](#gradients--frames)).
- **line** / **arrow** / **path**: `points`, `segments` (curves), `viewbox`, `stroke`,
  `stroke_width`, `dash`, `cap`, `join`, `head`, `head_size`, `head_width`,
  `head_style`; **path** adds `fill`, `closed`.
- **ellipse**: `fill`, `stroke`, `stroke_width`, `dash`, `cap`, `join`.

Lengths are in the page `unit`; `font_size` is always points. Internally
everything is converted to points (1 pt = 1/72 in).

### Text outline

A `text` node takes the same `stroke` (colour) and `stroke_width` (page units,
like every other stroke width; default 1 pt once `stroke` is set) as the
shapes, and draws a real outline along the glyph contours:

```toml
[nodes.label]
type = "text"
content = "(a)"
font_weight = 700
stroke = "#ffffff"     # a white halo, legible on dark images
stroke_width = 0.07    # cm here: the rim reaches 0.035 cm past the glyphs
```

- **Paint order is stroke, then fill** (SVG `paint-order="stroke"`) for the
  whole string: the outlines of all glyphs are stroked first, then the glyphs are
  filled on top, so the fill covers the inner half of the stroke and the glyph
  shapes stay exactly as without it — only `stroke_width / 2` shows outside.
  A neighbouring glyph's outline never covers a glyph, and a translucent outline
  (`"#000000a0"`) stays uniform where outlines overlap.
- **Layout ignores it**: the box is the same as without a stroke; the outline
  may overhang it by `stroke_width / 2` (it is not clipped).
- `join` (`round`, the default for text; `miter`, `bevel`) shapes the corners.
  On shapes it defaults to `miter`. (`dash`/`cap` also apply to a text outline.)
- Same geometry in every backend: the unhinted glyph outlines (`ttf-parser`)
  are stroked as one path. PNG strokes them with tiny-skia and blends the glyph
  bitmaps on top; PDF draws the outline as a vector path (not as stroked text),
  so the text layer still contains the string exactly once; SVG writes one
  stroked `<path>` before the glyph group.

In Rhai: `text("(a)").bold().stroke("#ffffff").stroke_width(0.07)` (`.join(..)`
to change the join). The framework's `halo(t, color, r)` is exactly this with
`stroke_width = 2r`.

### Gradients & frames

Anywhere a fill is accepted — `rect` / `ellipse` / `path` `fill`, container
`fill`, `page.background` — you can write a colour or a **gradient**:

```toml
fill = "#ff0000"                                                  # solid, as before
fill = { linear = 90, stops = [[0, "#0b1a3a"], [1, "#1d3b6e"]] }  # top → bottom
fill = { linear = [[0, 0], [1, 1]], stops = ["#ffffff", "#000000"] }  # corner to corner
fill = { radial = [0.5, 0.5], radius = 0.5, focal = [0.35, 0.3],
         stops = [[0, "#ffffffcc"], [1, "#ffffff00"]] }
```

- **Coordinates are relative to the node's border box** (`0..1` fractions of
  its width/height), so a gradient stretches with the layout. Add
  `absolute = true` to give points and `radius` in the page unit from the box's
  top-left corner instead (an absolute radial gradient needs a `radius`).
- `linear = <degrees>` runs **clockwise from left → right**: `0` = left →
  right, `90` = top → bottom (PowerPoint's convention, matching `rotate`). The
  line passes through the box centre and spans it corner to corner, as in CSS.
  `linear = [[x1, y1], [x2, y2]]` runs from the first point to the second;
  colour bands stay perpendicular to that line on the page.
- `radial = [cx, cy]` is the centre; `radius` defaults to `0.5`, `focal`
  (where offset 0 sits) to the centre. Box-relative, the circle scales with the
  box — an ellipse touching all four edges at `radius = 0.5` in a non-square
  box (SVG's `objectBoundingBox`); `absolute = true` makes it a true circle.
- `stops` are `[offset, colour]` pairs (offsets `0..1`, non-decreasing; colours
  may carry alpha, `#rrggbbaa`) or bare colours, which are spaced evenly (the
  first defaults to 0, the last to 1, as in CSS). Beyond the ends the end
  colours continue (pad).
- Gradients are drawn in the node's own frame, so they **turn with `rotate`**.
  Strokes stay solid colours (a thick gradient arrow is a closed `path` with a
  gradient `fill` — see [`examples/poster_frame.figs`](examples/poster_frame.figs)).

**Container decoration.** `column` / `row` / `stack` take `fill`, `stroke`,
`stroke_width` (default 1 pt once `stroke` is set), `corner_radius` (`radius`),
and `dash` — a background over the whole rounded box and a border drawn
**inside** it (its outer edge is the box edge, outer corner radius =
`corner_radius`), both painted behind the children. That replaces the
`stack([rect()…, content])` wrapper:

```toml
[nodes.header]
type = "row"
children = ["title"]
padding = 0.4
fill = { linear = 90, stops = ["#3d6fb6", "#16305e"] }
stroke = "#bcd4ff"
stroke_width = 0.06
radius = 0.6
```

`clip = true` keeps the children inside the box — inside the border, with the
corner radius reduced by the stroke width — e.g. an image or plot in a rounded
frame. Clips nest (a child clipped twice shows only where both regions
overlap), and follow rotation. The clip is exact in every backend: PDF clip
path, SVG `clipPath`, PNG via a canvas snapshot restored outside an
anti-aliased mask (one canvas copy per clipped container).

Shadows go on any node, frames included — see
[Shadows, glows & opacity](#shadows-glows--opacity).

In Rhai: `.fill("#…")` or `.fill(linear(90, ["#0b1a3a", "#1d3b6e"]))`,
`linear([x1, y1], [x2, y2], stops)`, `radial(stops)`,
`radial([cx, cy], r, stops)`, refined with `.focal(fx, fy)` / `.absolute()`;
on containers `.fill(…) .stroke(hex) .stroke_width(n) .radius(n) .dash([…])
.clip()`; `page(…).background(linear(…))`. The editor's inspector edits solid
fills and a gradient's stop colours (direction and offsets in TOML), and shows
a **Decoration** section for containers.

See [`examples/poster_frame.figs`](examples/poster_frame.figs): a conference
poster's framing — dark gradient page, gradient section header bars with a
light rounded border, solid and dashed framed blocks, a dashed separator, a
gradient block arrow, and a clipped rounded "photo" frame.
### Rich text (spans)

A text node can mix styles inside one paragraph: `spans` is a list of runs,
each with its own text and optional overrides. Anything a span leaves unset
inherits the node's style; the node's `content` (if any) comes first, unstyled.

```toml
[nodes.dg]
type = "text"
font_size = 11
font_family = "Arial, Yu Gothic"        # Latin in Arial, Japanese in Yu Gothic
spans = [
  { text = "ΔG" },
  { text = "300K", script = "sub" },
  { text = " = " },
  { text = "−27.5", color = "#e0635a", font_weight = 700 },
  { text = " kJ mol" },
  { text = "−1", script = "super" },
]
```

| Span field | Meaning |
|---|---|
| `text` | the run's text (required) |
| `font_family` | family list for this run; **replaces** the node's list |
| `font_size` | pt; with `script`, the *base* size the script is derived from |
| `font_weight` | 100–900 (nearest installed face, CSS matching order) |
| `font_style` (alias `style`) | `"italic"` / `"normal"` |
| `color` | hex |
| `underline`, `strike` | `true` / `false` (can switch off a node-level one) |
| `script` | `"super"` (`"sup"`) / `"sub"`: size × **0.65**, baseline **+0.35 em** / **−0.15 em** (em of the base size) |
| `baseline_shift` | exact shift in em of the base size, positive = up (overrides `script`'s) |
| `letter_spacing` | tracking in em of the run's font size |

- **Line height.** A line box is `size × line_height` of the largest run on
  the line, with the largest ascent + descent box centred in it (CSS
  half-leading), so a 2× span makes its line taller. **Super/subscripts count
  with their base size and unshifted**, so `mol⁻¹` or an affiliation `¹`
  never pushes lines apart; only at the block's very top/bottom does a raised
  or lowered glyph that would poke out of the box grow it (nothing clips).
- **Wrapping** treats the paragraph as one text: breaks can fall inside or
  between spans exactly as in unstyled text, and alignment, `line_height`,
  `stroke` and `rotate` work unchanged.
- **Underline / line-through** are bars from the face's own metrics
  (`post`/`OS/2` underline and strikeout position and thickness; size-based
  fallbacks when a face has none). An underline sits at the unshifted
  baseline at the run's base size, so a subscript's underline lines up with
  its neighbours'; a line-through follows the (shifted) glyphs. Bars are
  filled in the run's colour, underlines under the glyphs, line-throughs over
  them; they are not part of a text `stroke`.
- **`letter_spacing`** (node or span, **em** of the font size, so it scales
  with the text) is added after every character — after every *cluster*, so a
  ligature or a base + combining mark gets it once — before line breaking, so
  wrapping sees it. Kerning and ligatures are kept. The trailing spacing of a
  line's last character is not counted in its width, so centred and
  right-aligned tracked titles stay centred on their ink.

#### Fonts: fallback lists, weights, italic

- **The author's list is authoritative, per character.** `font_family =
  "Arial, Yu Gothic"` sets each character in the *first* listed family that
  has a glyph for it: Latin, digits, Greek (`Δ μ`), `−` and spaces in Arial;
  kana, kanji and full-width punctuation (`、。（）`) in Yu Gothic. Combining
  marks, variation selectors and joiners stay with the preceding character.
  Names match case-insensitively, localized names too (`游ゴシック`); the
  generic `sans-serif` / `serif` / `monospace` map to the system defaults.
  Families that aren't installed are skipped; with none installed the
  generic `sans-serif` face is used. `[page] font_family` (Rhai
  `page(..).font("…")`) is the default list for nodes that name none.
- **What's left to the system.** Only characters no listed family covers fall
  through to cosmic-text's fallback: the requested face, then a per-script
  platform list, then a few common fonts, then any face that has the glyph.
  Its script lists are **Windows**: Yu Gothic for kana (and for kanji only
  when the locale is exactly `ja`; otherwise Microsoft YaHei UI, i.e. Chinese
  glyph shapes), then Segoe UI / Segoe UI Emoji / Segoe UI Symbol;
  **macOS**: Hiragino Sans (kana; kanji on `ja`, else PingFang SC), then
  `.SF NS`, Menlo, Apple Color Emoji, Geneva, Arial Unicode MS; **Linux**:
  Noto Sans CJK JA (kana; kanji on `ja`, else Noto Sans CJK SC), then Noto
  Sans, DejaVu Sans, FreeSans, Noto symbols/emoji. Naming the Japanese face in
  the list avoids all of this.
- **Weights** pick the nearest installed face in CSS order (for 400: 400,
  500, lighter, bolder; above 500: bolder first). There is no synthetic bold:
  a family with only a regular face (MS Mincho) stays regular.
- **Italic** uses the family's real italic face when it has one (Arial
  Italic, Times New Roman Italic), else an oblique face, else the upright face
  **slanted by 14°** (a synthetic oblique — Yu Gothic, MS Mincho, BIZ UD
  Gothic have no italics). The slant is a shear about each glyph's baseline
  origin, identical in PNG (swash's fake italic), PDF (a `cm` shear around the
  run; still real text) and SVG / rotated text / outlines (sheared outlines).

#### Japanese line breaking

Breaks follow UAX #14 (cosmic-text + `unicode-linebreak`): no break before
closing punctuation `、。，．）」』】` etc., none after opening brackets
`（「『【`, breaks allowed between ideographs and kana, Latin words kept whole.
Small kana and `ー` are treated as non-starters (the strict rule: `ッ`, `ャ`,
`ー` don't start a line; `チェッ` and `マー` stay together). One case UAX #14
can't help with is a "word" wider than the line (a long Latin word, a long run
of small kana, a box only a couple of characters wide), which is broken
between arbitrary glyphs. There figs fixes up the result: an opening bracket
left at a line end moves to the next line, and closing punctuation left at a
line start hangs at the end of the previous line (*burasage*; it may overhang
the box by that one glyph). A small kana can still start a line in such a
box. Tests sweep a mixed paragraph across widths.

In scripts the core builders mirror the model 1:1:

```rhai
text([span("ΔG"), span("300K").sub(), " = ", span("−27.5").color("#e0635a").bold()])
```

`text([...])` takes spans and plain strings (nested arrays are flattened).
Span setters: `.bold()`, `.weight(n)`, `.italic()`, `.upright()`, `.color(hex)`,
`.size(pt)`, `.sup()`, `.sub()`, `.shift(em)`, `.underline()`, `.strike()`,
`.font(name | "A, B" | ["A", "B"])`, `.tracking(em)`. The same `.italic()`,
`.underline()`, `.strike()`, `.tracking(em)`, `.font([...])` on a text node set
the style every span inherits. The framework's [markup](#rich-text-markup)
writes the same spans concisely.

### Shadows, glows & opacity

Three effects go on **any node** — shape, text, image, or a container with
everything in it. They are purely visual: layout is exactly as without them.

```toml
[nodes.bar]                    # a section header bar with a soft drop shadow
type = "row"
fill = { linear = 90, stops = ["#3d4a73", "#080a10"] }
shadow = { dx = 0.06, dy = 0.07, blur = 0.2, color = "#00000066" }

[nodes.label]                  # a white label kept legible on a bright image
type = "text"
content = "(a)"
color = "#ffffff"
glow = { blur = 0.12, color = "#000000c0" }

[nodes.overlay]                # a translucent box, contents included
type = "column"
fill = "#0b1020"
opacity = 0.75
```

- **`shadow = { dx, dy, blur, spread, color }`** (all optional; lengths in
  the page unit): the silhouette of everything the node *and its subtree*
  paint — the rounded frame, the glyphs (with their outline), an image's
  alpha, a path's fill, strokes and arrowheads — grown by `spread`, blurred,
  tinted with `color` (its alpha is the shadow's opacity; default
  `#00000080`) and moved by (`dx`, `dy`). `blur` is the blur radius as in CSS
  (`box-shadow`): a Gaussian of σ = `blur / 2`, so the edge ramps from ~84 %
  to ~16 % over `blur`. The offset is on the page: a rotated node's shadow
  still falls the same way as every other one.
- **`glow = { blur, color, spread }`**: an outer glow — a shadow with no
  offset whose silhouette is first grown by `spread` (default `blur / 2`), so
  the halo is nearly opaque at the edges and fades out about `blur` past them
  (default colour `#000000`). The dark halo behind white labels on images.
  Unlike a text `stroke`, it is soft and works on any node.
- **`opacity`** (`0..1`): the node and its subtree are composited as **one
  layer** — overlapping children don't show through each other, as with a
  PowerPoint group or CSS `opacity`. Its shadows fade with it; `0` hides all
  of it.
- Paint order: the shadow, then the glow, then the node. Both are clipped by
  a clipping ancestor (like everything else the node draws); clips *inside*
  the node shape the silhouette.

Every backend draws the same pixels. A shadow is a **raster**: the group is
rendered by the PNG renderer onto a transparent canvas, its alpha grown by an
exact Euclidean distance transform (round corners), blurred (three box blurs,
the SVG `feGaussianBlur` recipe, exact kernel below σ = 2 px), and tinted.
PNG does this at the output resolution and blends it under the node; PDF —
which has no blur — and SVG embed the same raster as an image, at about four
pixels per σ (150–300 dpi), which looks identical at any zoom because a blur
has no fine detail. SVG deliberately doesn't use `<filter>`: filter regions,
resolution and `feMorphology` (square, not round) differ between viewers.
Opacity is native: a PNG layer composited back at that alpha, a PDF
transparency group, an SVG `<g opacity>`.

In Rhai: `.shadow(dx, dy, blur)`, `.shadow(dx, dy, blur, color)`,
`.shadow(dx, dy, blur, spread, color)`, `.glow(blur)`, `.glow(blur, color)`,
`.opacity(a)`. The editor's inspector has an **Effects** section for every
node. See [`examples/effects.figs`](examples/effects.figs).

### Pixelated images

`smooth = false` on an `image` (Rhai `.smooth(false)`) scales it
nearest-neighbour, so a low-resolution scan — an 83 × 52 px AFM image blown
up to a panel — keeps its blocky pixels instead of turning to mush. PNG
samples it nearest-neighbour; PDF and SVG embed it upscaled by a whole factor
(each pixel a block, to about 300 dpi at its drawn size) so every viewer shows
the blocks whatever its own interpolation (poppler, for one, ignores a missing
`/Interpolate` below 4× magnification), and SVG adds
`image-rendering: pixelated`. Smooth images (the default) are bilinear in PNG
and carry `/Interpolate true` in PDF, so a small image looks the same in both.

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
  inside still use flex for widths. A column whose own height is known — an
  explicit `height`, or an `aspect_ratio` (its width is always decided) — is
  not content-sized: its flex children share that height as on a fixed page.
  That is what lets a `plot(…)` (an aspect column around a flex data area) be
  the page root: `page("double", plot(…))`.
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

`contain = true` (script: `.contain()`) on a stack keeps everything its
children draw inside its box: when a child — or anything nested in one — would
hang past an edge, the stack shrinks its **content box** by exactly that
overhang, and offsets, filling children and the viewbox map onto the smaller
box. The stack's own size doesn't change; its padding box is the limit (so
`padding` is the clear space kept at the edges). It is how a plot keeps its tick
labels: the labels hang off the axes in data units, and the axes shrink until
the widest y label, the tallest x label and the half labels centred on the axis
ends all fit — measured, never estimated. Overhangs that don't depend on the
box size (a label centred on an edge) settle at once; others converge in a few
passes. On a plain stack it also makes room for negative offsets (and counts
them in its intrinsic size).

```toml
[nodes.axes]                   # x 0..10; "end" is centred on the right edge
type = "stack"
viewbox = [0, 0, 10, 1]
contain = true                 # -> the content box gives up half its width
children = ["curve", "end"]
```

Overhangs are measured on what children actually cover, so a rotated child
counts with its turned box.

### Rotation

`rotate` (degrees) turns any node — and its whole subtree — **clockwise on the
page about the centre of its box** (`-90` is a quarter turn counter-clockwise,
so text reads bottom-to-top). There is no separate pivot: to turn about a
point, centre the node on it in a stack with `anchor = [0.5, 0.5]`.

How layout treats it depends on the angle:

- **Quarter turns are layout-aware.** An odd quarter turn (`90`, `-90`, `270`)
  swaps the node's footprint, like Flutter's `RotatedBox`: the parent lays out
  the turned box. A rotated y-axis label in a row is exactly one text line wide
  and as tall as its text is long; in a stretched row it is as long as the row
  is tall, so centred text sits at the middle of the axis. `180` keeps the
  footprint.
- **Any other angle is paint-only**, like CSS `transform`: layout uses the
  unrotated box and the drawing may overhang its neighbours (a tilted note or
  stamp). Angles within 0.001° of a multiple of 90° count as quarter turns.

The node's own properties stay in its own frame: `width`, `height`,
`aspect_ratio` and `padding` describe the unrotated box (`width = 4,
rotate = 90` is 4 tall on the page), and its children are laid out inside it as
usual, then turned with it. What the parent reads — `margin`, `flex`, `x`/`y`,
`anchor` — acts on the footprint in the parent's frame (`anchor = [0.5, 1]` is
the bottom-centre of the turned box).

```toml
[nodes.axis]
type = "row"
cross_axis_alignment = "stretch"
children = ["ylabel", "img"]

[nodes.ylabel]           # a vertical axis label, centred along the image
type = "text"
content = "Intensity (a.u.)"
align = "center"
rotate = -90

[nodes.img]
type = "image"
src = "a.png"
flex = 1
```

In scripts: `.rotate(deg)` — `row([text("Intensity").center().rotate(-90),
image("a.png").flex(1)]).stretch()`, or a tilted label
`text("note").at(3, 1).anchor(0.5, 0.5).rotate(-20)`. See
[`examples/rotation.figs`](examples/rotation.figs).

In the IR each node carries an absolute `transform` (`Option<Affine>`, the
composition of its own and its ancestors' rotations; `None` when unrotated);
its `rect` is the unrotated box in that local frame and `bounds()` the
page-space box it covers. Every backend draws the node unrotated under that
transform (see [Output formats](#output-formats)), and the editor's canvas
hit-test (`ComputedLayout::hit_test`) and selection outline follow it; the
inspector has a **Rotate** field.

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
  repeated, as in SVG); `cap` is `butt` (default), `round` or `square`; `join`
  is `miter` (default), `round` or `bevel`.
- **Arrowheads.** `head` is `none` / `start` / `end` / `both` (default `end` for
  `arrow`, `none` for `line`/`path`, so `path` + `head` gives a bent arrow).
  `head_size` is the head's length in page units (default 5 × the stroke width);
  `head_width` its base width — a length, or a multiple of the stroke width as a
  string, `head_width = "3x"` (default 0.8 × `head_size`; PowerPoint's medium
  head is `head_size = 3w`, `head_width = "3x"`). Heads are in the stroke color
  and stay solid on dashed strokes; closed paths have no heads.
- **Head styles.** `head_style` is `triangle` (default, filled), `stealth`
  (filled, swept back with a notch 0.6 × its length from the tip), `open` (a
  chevron stroked in the shaft's width, its mitered apex on the tip),
  `barb_left` / `barb_right` (a filled half head on one side of the shaft —
  the harpoons of an equilibrium `⇌`; left/right as seen travelling to the
  tip). The shaft is cut back into the head so no line cap shows past it.

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

#### Arrow shapes

PowerPoint's block arrows are prelude shapes too. Each is **one closed
`path`** filled with `accent()`, so `.fill(linear(..))`, `.stroke(..)`,
`.shadow(..)` and `.opacity(..)` chain on it. Angles are degrees from +x,
clockwise on the page (as `rotate`); lengths in page units.

| Function | Result |
|---|---|
| `block_arrow(x1, y1, x2, y2, shaft_w, head_w, head_len)` / `(…, #{ both: true })` | A straight block arrow to its tip (x2, y2): 7 corners, or 10 with a head at both ends (⇔). |
| `equilibrium(x1, y1, x2, y2)` / `(…, gap)` / `(…, opts)` | ⇌: two arrows `gap` apart (default 4 × the width) in `fg()`, `annotation_width()` wide. Options `gap`, `width`, `barb` — `"straight"` (core half-barb harpoons; `head_size`, `head_width` as on any arrow) or `"hook"` (plain shafts, each with a curved barb leaving the tip along the shaft and bending outwards, 1.5 × the width with round caps — the PowerPoint ⇌; `head_size` = its length back along the shaft, default 0.4 × the chord up to 20 × the width; `head_width` = how far out, a length or `"3x"`, default 0.4 × `head_size`). |
| `arc_arrow(cx, cy, r, a0, a1, w)` / `(…, opts)` | A circular block arrow: a band `w` wide on the circle of radius `r`, from angle `a0` to its tip at `a1` (`a1 < a0` runs anticlockwise; less than a full turn). Outer and inner edges are true arcs. Options `head_width` (default 2 × w), `head_len` (along the circle, default w), `both`. |
| `block_arrow_along(points, w)` / `(…, opts)` | A bent block arrow following a centre line: `[x, y]` entries are straight runs, `[cx, cy, x, y]` quadratic and `[c1x, c1y, c2x, c2y, x, y]` cubic curves (the arguments of `.quad` / `.cubic`). The outline is the centre line (curves flattened) offset by w / 2 each side, with mitred joins bevelled past a miter of 4 (keep curves wider than the band). Options `radius` (round every corner between straight runs), `head_width` (default 2 × w), `head_len` (default w), `both`. |

```rhai
block_arrow(1, 3.5, 7, 3.5, 0.6, 1.4, 1.0, #{ both: true })
equilibrium(9, 3.5, 13, 3.5, #{ barb: "hook", width: 0.06 })
arc_arrow(4, 9, 2.5, 200, 340, 0.6)                          // over the top
block_arrow_along([[14, 6], [14, 10], [19, 10]], 0.8, #{ radius: 1.2 })
```

See [`examples/arrows.figs`](examples/arrows.figs).

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
  `.margin(n)`, `.pad(n)`, `.rotate(deg)` (clockwise; see [Rotation](#rotation)).
- **effects** (any node): `.shadow(dx, dy, blur[, [spread,] color])`,
  `.glow(blur[, color])`, `.opacity(a)` — see
  [Shadows, glows & opacity](#shadows-glows--opacity).
- **user coordinates**: `.viewbox(x0, y0, x1, y1)` (stack/line/arrow/path) and
  `.anchor(ax, ay)` (read by a `stack` parent) — see
  [User coordinates](#user-coordinates-viewbox--anchors).
- **offset** (read by a `stack` parent): `.at(x, y)`, `.x(n)`, `.y(n)` — e.g.
  `stack([image("a.png"), text("(a)").at(0.2, 0.2)])`.
- **text**: `.size(n)`, `.weight(n)` / `.bold()`, `.italic()`, `.color(hex)`,
  `.font(name | "A, B" | ["A", "B"])`, `.tracking(em)`, `.underline()`, `.strike()`,
  `.align("left"|"center"|"right"|"justify")` / `.center()`, `.line_height(n)`,
  and an outline painted behind the glyphs: `.stroke(hex)`, `.stroke_width(n)`,
  `.join("round"|"miter"|"bevel")` (see [Text outline](#text-outline)).
  Rich text: `text([span("ΔG"), span("300K").sub(), …])` (see
  [Rich text](#rich-text-spans)).
- **container** (`col`/`row`): `.spacing(n)`, `.main("space_between"|…)`,
  `.cross("start"|"center"|"end"|"stretch")` / `.stretch()`.
- **decoration** (`col`/`row`/`stack`): `.fill(hex | gradient)`, `.stroke(hex)`,
  `.stroke_width(n)`, `.radius(n)`, `.dash([on, off, …])`, `.clip()` — see
  [Gradients & frames](#gradients--frames).
- **gradients** (for any `.fill` / `.background`): `linear(deg, stops)`,
  `linear([x1, y1], [x2, y2], stops)`, `radial(stops)`,
  `radial([cx, cy], r, stops)`, `.focal(fx, fy)`, `.absolute()`; `stops` are
  `[[0, "#…"], [1, "#…"]]` or `["#…", "#…"]`.
- **rect**: `.fill(hex | gradient)`, `.stroke(hex)`, `.stroke_width(n)`, `.radius(n)`.
- **shapes** (`line`/`arrow`/`ellipse`/`path`): `.stroke(hex)`, `.stroke_width(n)`,
  `.fill(hex)` (path/ellipse), `.dash([on, off, …])`, `.cap("butt"|"round"|"square")`,
  `.join("miter"|"round"|"bevel")`,
  `.head("none"|"start"|"end"|"both")`, `.head_size(n)`, `.head_width(n | "3x")`,
  `.head_style("triangle"|"stealth"|"open"|"barb_left"|"barb_right")`, `.closed()` (path) — e.g.
  `stack([image("a.png"), arrow(1, 4, 3, 2).stroke("#e0635a")])`.
- **curves** (continue a path from its last point): `.to(x, y)`,
  `.quad(cx, cy, x, y)`, `.cubic(c1x, c1y, c2x, c2y, x, y)`, `.arc(r, x, y)` /
  `.arc(r, x, y, #{ sweep, large, rotation })` (`r` a number or `[rx, ry]`) —
  see [Curves](#curves).
- **image**: `.fit("contain"|"cover"|"fill")`, `.smooth(false)`.
- **page**: `.unit("cm"|…)`, `.background(hex | gradient)`, `.dpi(n)`, `.font(name | list)`.
- **script-only**: `.meta(key, value)` / `.meta(key)` (a note on the node for
  other script code, `()` when unset; never part of the document — how a plot
  asks `labeled` for label room), `.prepend(child)` (insert a first child into
  a `col`/`row`/`stack`).

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
| `style(preset)` / `style(map)` / `style(preset, map)` | Switch a bundle of defaults in one line: `style("nature");` at the top of the script. See [Style presets](#style-presets). |
| `page(preset, root)` | Page of a named size, unit `cm`, in the style's `font_family()` when one is set and on its `page_bg()` background when one is set. Journal presets `"single"` (`single_width()`, default 8.5 cm column) and `"double"` (`double_width()`, default 17.5 cm) fix only the width and get an [auto height](#auto-page-height) that fits the content. Paper presets `"a5"`, `"a4"`, `"a3"`, `"a2"`, `"a1"`, `"a0"` (posters), `"letter"` are fixed; append `-landscape` to swap their sides (journal presets have no landscape form). The core `page(w, h, root)` is unchanged. |
| `paper_size(preset)` | `[w, h]` of a preset, to drop down to core: `let s = paper_size("double"); page(s[0], 5, root).unit("cm")`. For journal presets `h` is the string `"auto"`, so `page(s[0], s[1], root)` stays auto. |
| `title(s)` / `caption(s)` / `label(s)` / `body(s)` | Semantic text: `title_size()` (1.5× base) bold centered / base centered / `label_size()` (1.25× base) bold / base. `s` may also be a span array: `body(markup("H_2O"))`. |
| `rich(s)` / `markup(s)` | Rich text from [inline markup](#rich-text-markup): `rich(s)` is `text(markup(s))`, `markup(s)` the span array itself. |
| `halo(t, color, r)` | Text node `t` with a halo: the core [text outline](#text-outline) in `color`, reaching `r` cm past the glyphs (`.stroke(color).stroke_width(2 * r)`, round joins). Purely visual — lays out exactly like `t` alone, so it drops in anywhere; what panel labels and scale-bar labels use by default. |
| `vbox(items)` / `hbox(items)` | `col`/`row` with the default gap, stretched cross axis, and bare strings allowed. |
| `grid(ncols, items)` / `grid(ncols, gap, items)` | Rows of `ncols` equal-flex cells (rows share height equally); the last row is padded with empty cells so widths stay equal. |
| `panels(items)` / `panels(style, items)` | Each item overlaid with an auto label, `label_inset()` in from its `label_corner()` (via core `stack`). `style` is a template whose last `a`/`A`/`1` is the counter: `"(a)"` (default), `"a"`, `"A"`, `"a."`, `"Fig. 1"`. |
| `panels(items, captions)` / `panels(style, items, captions)` | The same labeled panels, each with its caption below (`captioned`). `captions` must match `items` in length. |
| `labeled(item, s)` / `panel_label(style, i)` | One labeled panel — `stack([item, pin(label_tag(s), label_corner(), label_inset())])` — / the `i`-th (0-based) label string. |
| `circle(r)` / `circle(cx, cy, r)` | A square `ellipse` of radius `r`; the 3-argument form is centered on (`cx`, `cy`) of the enclosing `stack` (via `.at`). |
| `curve` / `curved_arrow` / `brace` / `bracket` `(x1, y1, x2, y2, bend_or_depth)` | Curved annotations built on the core curve builders — see [Curves](#curves). |
| `block_arrow(x1, y1, x2, y2, shaft_w, head_w, head_len)` | A thick block arrow with its tip at (`x2`, `y2`): a shaft `shaft_w` wide and a triangular head `head_w` across, `head_len` long — one closed 7-corner `path` filled with `accent()`, so `.fill(linear(0, [...]))` gives a gradient and `.shadow(..)` depth. |
| `equilibrium(x1, y1, x2, y2)` / `(…, gap)` | The equilibrium arrows ⇌: two `arrow`s with `head_style("barb_left")` harpoon heads, `gap` apart (default 4 × `annotation_width()`) either side of the chord — one towards (`x2`, `y2`), one back — barbs outside; in `fg()`, `annotation_width()` wide, in a `stack`. |
| `annotate(item, marks)` | `stack([item, ..marks])`: overlays annotation nodes (an array, or one node) on a panel; coordinates inside are cm from the panel's top-left. Composes with labels: `panels([annotate(..), ..])`, `labeled(annotate(..), "(a)")`. |
| `scalebar(len, s)` | A `len` cm bar (white, `scalebar_thickness()` thick) with label `s` centered above it (`""` for none), `scalebar_inset()` in from `scalebar_corner()` (default bottom-right). |
| `scalebar(len, field, s)` | The same bar in image/physical units: `len` out of a field `field` wide (same unit), so exactly `len / field` of the panel width whatever the layout — `scalebar(5, 50, "5 μm")` for a 50 μm field, `scalebar(80, 1024, "1 μm")` for 80 px of a 1024 px image. |
| `callout(tx, ty, x, y, s)` | The core `arrow(tx, ty, x, y)` in `accent()` with label `s` at its tail, beside it on the side away from the target and vertically centered on `ty`. |
| `highlight(x, y, w, h)` / `spot(cx, cy, r)` | Dashed box with its top-left at (`x`, `y`) / dashed circle centered on (`cx`, `cy`), in `accent()` (a closed `path` / an `ellipse`; `.dash([])` makes them solid). |
| `pin(node, corner)` / `pin(node, corner, inset)` | Any node pinned `inset` (default `label_inset()`) in from a corner of the enclosing stack — what `labeled` and `scalebar` use. |
| `captioned(item, c)` | `item` above caption `c` (a string becomes `caption(c)`, a node is used as is), `caption_gap()` apart; the item takes the spare height and the caption spans the item's width, so its centered text sits under the middle of the panel whatever the parent's cross alignment. |
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
fn font_family() { "Arial" }  // page font of page(preset, root) (default "": renderer default)
fn title_size() { 14 }      // pt (default base_size() * 1.5)
fn label_size() { 9 }       // pt (default base_size() * 1.25)
fn gap() { 0.3 }            // cm between siblings in framework containers (default 0.2)
fn label_style() { "A" }    // panel labels A, B, C … (default "(a)")
fn label_inset() { 0.3 }    // cm from the panel corner (default base_size() * 0.02 = 0.16)
fn label_corner() { "top-right" }   // or "bottom-left" / "bottom-right" (default "top-left")
fn label_halo() { "" }      // rim around the label (default "#ffffff"; "" for none)
fn label_halo_width() { 0.05 }      // cm past the glyphs (default label_size() * 0.0035 ≈ 1 pt)
fn label_background() { "#ffffff" } // box behind the label instead of the halo (default "": none)
fn label_pad() { 0.1 }      // cm between label text and its box (default base_size() * 0.01)
fn caption_gap() { 0.15 }   // cm between a panel and its caption (default gap() / 2)
fn single_width() { 8.9 }   // cm, the "single" page preset (default 8.5)
fn double_width() { 18.3 }  // cm, the "double" page preset (default 17.5)
fn accent() { "#ffd400" }   // callout / highlight / spot colour (default "#ff5a3c")
fn annotation_width() { 0.05 }      // their stroke, cm (default base_size() * 0.005)
fn annotation_dash() { [0.2, 0.1] } // highlight/spot dash, cm (default 3x / 2x the width)
fn scalebar_color() { "#000000" }   // bar + label (default "#ffffff")
fn scalebar_thickness() { 0.1 }     // cm (default base_size() * 0.01 = 0.08)
fn scalebar_corner() { "bottom-left" } // (default "bottom-right")
fn scalebar_inset() { 0.3 }         // cm from the corner (default label_inset())
fn scalebar_halo() { "" }           // bar + label outline colour (default "#000000a0")
fn scalebar_frac_inset() { 0.05 }   // side inset of scalebar(len, field, s), fraction of width (default 0.025)
fn fg() { "#ffffff" }       // colour of semantic text and (via plot_fg()) plots (default "": renderer black, no colour set)
fn page_bg() { "#0f1a2e" }  // background of page(preset, root) (default "": none)
```

The label and caption defaults derive from `base_size()` / `gap()`, so
overriding those rescales them too. The inset keeps labels off the panel edge;
the caption gap is half the grid gap so a caption reads as belonging to the panel
above it rather than the one below.

#### Rich-text markup

`rich("…")` (or `markup("…")` for the bare span array) writes
[spans](#rich-text-spans) in one string. The parser is Rust
([`script/markup.rs`](crates/figs-core/src/script/markup.rs)), framework-only:
it just produces the core spans, which layout and the backends handle as if
written out with `span(..)`.

```rhai
rich("○__佐藤 太郎__^1・A. Tanaka^{1,*}")                // underline, superscripts
rich("A. Tanaka *et al.*, *Nature* **2021**, *12*, 345–352")
rich("Δ*G*_{300K} = {#e0635a bold|−27.5} kJ mol^{−1}")   // subscript, colour group
rich("H_2O, OD_{600}, *K*_n/*K*_e, 4.2 Debye^†, ~~0.38~~")
rich("{#ffd700 bold|Sodium chloride} = 結晶成長部位").font("Arial, Yu Gothic")
```

| Markup | Span |
|---|---|
| `*text*` / `**text**` | italic / bold |
| `__text__` / `~~text~~` | underline / line-through |
| `_x`, `_{text}` | subscript (one character, or a braced group) |
| `^x`, `^{text}` | superscript |
| `{spec\|text}` | style group; `spec` = space-separated items: `#rrggbb` colour, `bold`/`b`, `italic`/`i`, `upright`, `underline`/`u`, `strike`/`s`, `sup`, `sub`, `size=12` (pt), `weight=600`, `font=Arial` / `font='Times New Roman'`, `tracking=0.1`, `shift=0.3` (em), `color=#hex` |
| `\c` | `c` literally: `\*`, `\_`, `\^`, `\{`, `\}`, `\~`, `\\` |

Groups nest (`x^{*n*}` is an italic superscript; `{u|… {#f00|…} …}`).
`*`, `**`, `__`, `~~` open only when followed by a non-space **and** closed
later in the same group (by a delimiter preceded by a non-space); otherwise
they are literal — `5 * 3`, `Tanaka^{1,*}`, `*corresponding author` need no
escaping. `_`/`^` before a space or the end are literal. Errors (an unmatched
`}`, a `{` without `|`, an unknown style item, a bad colour, a dangling `\`,
an unclosed `_{`) name the 1-based character column:
`rich: markup: in style group: \`#zz\`: invalid colour \`#zz\` (column 4)`.

See [`examples/rich_text.figs`](examples/rich_text.figs) for a poster body
block (title band with tracking, authors with affiliations, a wrapped
Japanese/English paragraph, thermodynamics with subscripts and coloured
values, a serif reference list).

#### Style presets

Hooks give one-knob-at-a-time control; `style(..)` switches a coherent bundle
of them in one line at the top of the script
([`examples/style_presets.figs`](examples/style_presets.figs)):

```rhai
style("nature");                               // a preset
style(#{ base_size: 9, label_style: "A" });    // individual keys, merged over it
style("science", #{ label_corner: "top-right" }); // both at once

page("double", grid(2, panels([image("a.png"), image("b.png")])))
```

| Preset | Font | Text (`base_size`) | Labels | `single` / `double` width |
|---|---|---|---|---|
| `"default"` | renderer default sans | 8 pt | `(a)`, 10 pt bold | 8.5 / 17.5 cm |
| `"nature"` | Arial | 7 pt | `a`, 8 pt bold lowercase | 8.9 / 18.3 cm (89 / 183 mm) |
| `"science"` | Arial | 7 pt | `A`, 8 pt bold uppercase | 5.7 / 18.3 cm |
| `"acs"` | Arial | 7 pt | `(a)`, 8 pt bold | 8.25 / 17.78 cm (3.25 / 7 in) |
| `"rsc"` | Arial | 7 pt | `(a)`, 8 pt bold | 8.3 / 17.1 cm |
| `"poster"` | renderer default sans | 24 pt (60 pt titles, 1 cm gaps) | `(a)`, 30 pt bold | 8.5 / 17.5 cm |
| `"dark"` | renderer default sans | 8 pt, white (`fg`) | `(a)`, white with a black halo | 8.5 / 17.5 cm |
| `"poster-dark"` | renderer default sans | `"poster"` sizes + the `"dark"` theme | `(a)`, 30 pt, white | 8.5 / 17.5 cm |

The two dark presets are **themes**: `fg` white (semantic text, and through
`plot_fg()` every axis, tick, label and legend of a plot), series colours from
`dark_palette()`, a dark navy `page_bg` (`#0f1a2e`, applied by
`page(preset, root)`; `.background(page_bg())` on a core page), black label
halos and marker rings in the page colour. See [Plots](#plots) for the plot
keys.

The journal presets are **approximations** of each publisher's author
guidelines (which change, and differ between titles) — sensible starting points,
not certified styles; check the current guide before submitting. They use Arial
for the Helvetica/Arial-class sans the guides ask for, since it is the most
widely installed. Line widths need no key of their own: annotation strokes,
scale bars, insets and halos are derived from the text size, so 7 pt text gives
~1 pt strokes (inside the usual 0.25–1.5 pt range) and `"poster"` gets
proportionally heavier ones.

- **Mechanism.** Hooks are functions, and Rhai functions can't see script
  variables, so each evaluation gets a small style registry on the Rust side
  (`style_set` / `style_get` / `style_reset`, fresh per script). Every hook
  reads its own key first — `fn base_size() { style_get("base_size", 8) }` —
  so the keys of `style(map)` are exactly the hook names above, and a
  misspelled key or unknown preset is an error that lists the valid ones.
  `style(name)` resets earlier `style` calls; `style(map)` merges.
  `style_push(map)` / `style_pop()` scope a map (save the registry, merge,
  restore it exactly) — what a plot's `style:` option uses.
- **Your hooks still win.** A `fn base_size() { … }` in your script replaces
  the whole hook, registry lookup included, so it overrides any preset.
- **Fonts.** The preset's `font_family()` is set on the page by
  `page(preset, root)` (chain `.font("…")` to override, or set it on single
  nodes with the core `.font`). A family that isn't installed falls back to the
  system's default font — the figure still renders, in another face — so pin a
  concrete installed family for reproducible output.

**Labeled & captioned panels.** A labeled panel is a core `stack`: the item
fills it and the label tag is overlaid with `pin(tag, label_corner(),
label_inset())`. Stack offsets measure from the top-left, so a top-left tag is
simply `.at(inset, inset)`; for the other corners it sits in a column that fills
the stack (no offset, no size), padded by the inset and aligned with
`.main("end")` / `.cross("end")` as needed.

By default the label is black with a thin white **halo** (`label_halo()`): on
white plots and light images the rim is invisible, so the label looks exactly
as before, while on dark micrographs it keeps the letter crisp instead of
vanishing — the same contrast trick as `scalebar_halo()`, without covering the
image with a box. `halo(t, color, r)` is just the core
[text outline](#text-outline) — one text node with `stroke` set, painted behind
the glyphs — so the label lands exactly where a bare one would and a PDF's text
layer holds it once. With
`label_background()` set, the label instead becomes a small stack of a filled
`rect` under the text (padded via the text's margin). All of it is ordinary
nodes, so you can restyle or replace any piece. Captions go below with
`panels(items, captions)`
([`examples/captioned_panels.figs`](examples/captioned_panels.figs)); a caption
spans its panel's width, so its centered text sits under the panel's middle:

```rhai
fn label_background() { "#ffffff" }   // a white box instead of the halo

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
fn label_background() { "#ffffff" }   // a white box instead of the halo

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
images are mostly dark), with a thin translucent-black outline around the bar
and its label (`scalebar_halo()`) so it stays legible over bright regions
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
| `bars(xs, ys)` / `bars(xs, ys, opts)` | Bars from 0. `xs` are numbers or **category names** (a categorical axis); several bar series are grouped side by side. Error bars on bars are in `plot_fg()` (black). |
| `errorbars(xs, ys, errs)` / `(…, opts)` | Error bars alone (y ± err, in `plot_fg()` — black — by default). |
| `plot_text(x, y, s)` / `(…, opts)` | Text at the **data point** (x, y), centred on it; goes among the series or in `marks`. Options: `color` (hex or palette index; default `plot_fg()`), `size` (pt, default `base_size()`), `font` (a family or fallback list; default `plot_font()`, the page font), `bold`, `italic`, `markup: true` (`s` is [inline markup](#rich-text-markup) — `"*k* = 0.3 min^{−1}"` becomes rich spans), `anchor: [ax, ay]` (which point of the text sits there, default `[0.5, 0.5]`). |
| `plot_arrow(x1, y1, x2, y2)` / `(…, opts)` | An arrow between two data points, in `plot_fg()`, `series_width()` wide. Options: `color`, `width` (cm), `head` (`"end"`, `"start"`, `"both"`), `dash`. |
| `palette_color(i)` | Colour `i` (0-based, wrapping) of `palette()` — to give markers and their fit line one colour; `#{ color: i }` does the same in series options. |
| `legend(series)` / `legend(style, series)` | The legend as a standalone node (e.g. beside a row of plots), in `legend_style()` or `"swatch"` / `"text"`. |
| `csv(path)` | A map of columns: numbers when every non-empty cell parses (an empty cell is `NaN`), else strings. Comma- or tab-separated (TSV when the header has tabs and no commas), RFC 4180 quotes, `#` comment lines skipped. Relative paths resolve against the script's directory (the CLI and editor pass it), like image `src`. |

**Series options** (a map, or a string = `label`): `label` (legend entry),
`color` (`"#rrggbb"`, or a palette index `0`, `1`, …; default: next
`palette()` colour, in order), `width` (cm), `dash`, `marker` (`"o"`, `"s"`,
`"^"`, `"d"`, `"none"`), `open` (`true`: hollow markers, outlined in the
series colour `marker_stroke_width()` wide; default `marker_open()`), `size`
(marker diameter, cm), `yerr` (symmetric error per point).

**Plot options**: `x` / `y` ranges `[lo, hi]` (default: the data plus a 2 %
margin — not past 0 — widened to whole ticks), `xlabel` / `ylabel`, `title`
(centred above the data area, `plot_title_size()`), `xscale` / `yscale`
(`"linear"` or `"log"`), `xticks` / `yticks` (explicit values), `legend` (a
corner, `"auto"`, `"text"` / `"swatch"` for that style at `legend_corner()`,
`""`/`false` for none, or a map `#{ style, corner }` / `#{ style, at: [x, y],
anchor: [ax, ay] }` placing the legend's top-left — or its `anchor` point — at
a data point), `frame` (`"open"`, `"box"`, `"none"`), and `marks`: extra nodes
overlaid on the data area, placed with `.at(x, y)` in data units (log10 of the
value on a log axis) — e.g. `text("p < 0.05").at(2, 0.9).anchor(0.5, 1)` — and
`plot_text` / `plot_arrow` annotations, which take plain data values (the plot
applies log axes) and may equally go in the series array. Per axis:
`xminor` / `yminor` (minor intervals per labelled step, overriding
`minor_ticks()`), `xtick_format` / `ytick_format` (tick-label decimals —
`2` gives `0.00, 0.25, …` with a real minus — or a function of the value
returning the label, `|v| `${v.to_int()}%``; log axes pass the value itself),
`xtick_direction` / `ytick_direction` (`"out"` / `"in"`, overriding
`tick_direction()`). `frame_box: true` makes the node's box the data frame
(below), and `style: #{ … }` applies style keys to **this plot only** — pushed
over the current style while the plot is built, then the registry is restored
exactly, so nothing leaks into later plots (defaults the series builders left
open, such as `marker_open()`, are read inside that scope too). `xlabel`,
`ylabel` and `title` also take markup spans: `xlabel: markup("*T* [K]")`.

**Placing a plot by its axes.** By default a plot's box holds everything — the
data area shrinks until tick labels, axis labels and title fit — which suits
grids. To match a drawn layout instead, `frame_box: true` (or
`style(#{ plot_frame_box: true })` for all plots) makes the node *be* the data
frame: `.at(x, y).width(w).height(h)` puts the axes exactly there, and tick
labels, axis labels and title hang outside the box (no `contain`, no
`plot_pad()`, and no panel-label band; the layout doesn't clip, so they still
render). `plot_bg()` then fills the frame only, and `plot_aspect()` applies to
the frame ([`examples/plot_frame.figs`](examples/plot_frame.figs)):

```rhai
style(#{ plot_frame: "box", plot_aspect: 0, tick_label_size: 11 });
let p = plot(#{ frame_box: true, x: [0, 10], y: [0, 1], ytick_format: 2, yminor: 5,
                xlabel: markup("*t* [min]"), ylabel: "fraction" }, series);
page(30, 13, stack([p.at(3, 2.5).width(11).height(7.5)])).unit("cm")  // frame at (3, 2.5)
```

**Panel-label room.** A plot reserves `plot_label_room()` (the label inset
plus one label line) at its top only when it is **labelled**: it notes the room
on its node (`.meta("label_room", …)`, script-only data that is never lowered),
and `labeled` / `panels` prepend that band inside the plot — under its aspect
and any size you chained, so a labelled plot keeps its box — before overlaying
the label. A plot on its own, in a `row`, or with `frame_box` reserves nothing.

**Fits over data, text legends, annotations, dark themes**
([`examples/plot_dark.figs`](examples/plot_dark.figs), the heating-curve /
spectrum style of a dark conference poster):

```rhai
style("poster-dark", #{ base_size: 16, plot_frame: "box", tick_direction: "in", minor_ticks: 2 });
let h = csv("data/heating.csv");
let f = csv("data/heating_fit.csv");

plot(#{ title: "加熱曲線", xlabel: "Temperature [°C]", ylabel: "会合度", legend: "text" }, [
    scatter(h.T, h.a10, #{ color: 0, open: true, label: "10 μM" }),   // data: hollow markers
    line_plot(f.T, f.f10, #{ color: 0 }),                            // its fit, same colour
    scatter(h.T, h.a100, #{ color: 1, open: true, label: "100 μM" }),
    line_plot(f.T, f.f100, #{ color: 1 }),
    plot_text(30, 0.2, "MCH", #{ bold: true }),                       // at a data point
    plot_arrow(60, 0.9, 70, 0.6),
])
```

- **Theme.** No plot colour is hard-wired: axes, frame, ticks, tick and axis
  labels, titles, swatch-legend text, uncoloured error bars and annotations
  use `plot_fg()` (which follows `fg()`, black by default), series
  `palette()`, marker rings `marker_ring()`, the panel fill `plot_bg()`. The
  `"dark"` / `"poster-dark"` presets switch them together; `dark_palette()`
  lifts the Okabe–Ito hues to at least 6:1 WCAG contrast on `#0f1a2e`
  (legible as legend *text*) while keeping them apart under deuteranopia /
  protanopia. The light default renders pixel-identically to before.
- **Text legend.** `legend: "text"` (or `legend_style()` `"text"`) stacks the
  labels alone, each in its series colour — the slide/poster convention — in a
  corner or at `at: [x, y]`. The swatch legend stays the default.
- **CJK.** Plot text sets no font of its own, so labels and titles use the
  page font (`font_family()`) and whatever fallback the renderer applies:
  `"会合度"` turned for the y label reads upwards like the Latin labels.

Defaults are journal-style and overridable hooks: open L-shaped axes, outward
ticks, labels at `base_size()`, the y label turned 90° (`.rotate(-90)`, reading
upwards) left of the tick labels and centred on the axis — or horizontal above
it with `ylabel_rotated()` false — category names that wrap within their slot,
and a legend without a box in the emptiest corner.

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
fn plot_label_room() { 0 }      // band a labelled plot keeps free for its panel label, cm (standalone plots: none)
fn plot_pad() { 0 }             // clear space between the outermost labels and the plot's box, cm (default base_size() * 0.01)
fn ylabel_rotated() { false }   // y label horizontal above the axis (default true: turned 90° left of the tick labels)
fn tick_direction() { "in" }    // ticks into the data area (default "out")
fn minor_ticks() { 2 }          // minor intervals per labelled step on linear axes (default 0: none)
fn marker_open() { true }       // hollow markers by default (default false)
fn marker_stroke_width() { 0.03 }  // hollow-marker outline, cm (default 1.25 × plot_line_width())
fn marker_ring() { "" }         // ring around filled markers (default "#ffffff"; "" for none)
fn legend_style() { "text" }    // "swatch" (default) / "text": coloured labels only
fn plot_title_size() { 10 }     // pt, the `title` option (default base_size())
fn plot_fg() { "#ffffff" }      // axes, ticks, labels, titles, legend text, annotations (default fg(), i.e. black)
fn plot_bg() { "#1b2a44" }      // fill behind the whole plot panel (default "": transparent)
fn plot_frame_box() { true }    // the plot's box is its data frame, labels hang outside (default false)
fn plot_font() { "Arial" }      // all plot text (default "": the page font); the fonts below follow it
fn tick_label_size() { 7 }      // pt (default base_size())
fn tick_label_font() { "Arial" }  // default plot_font()
fn tick_label_pad() { 0.05 }    // tick end (or axis, ticks "in") -> tick label, cm (default base_size() * 0.012)
fn axis_label_size() { 9 }      // xlabel / ylabel, pt (default base_size())
fn axis_label_font() { "Arial" }  // default plot_font()
fn axis_label_pad() { 0.1 }     // tick labels -> axis label, cm; the turned y label sits twice that (default base_size() * 0.012)
fn legend_size() { 7 }          // legend labels, pt (default base_size())
fn legend_font() { "Arial" }    // default plot_font()
fn plot_title_font() { "Arial" }  // the `title` option (default plot_font())
```

Every plot hook reads its style key first, so all of them can also be set with
`style(#{ … })` or a preset (`palette: dark_palette()`, `plot_frame: "box"`, …).

The rotated y label can still wrap mid-word when it is longer than the axis
(the core has no "don't wrap" switch yet); keep it shorter than the axis or use
`ylabel_rotated()` false.

How it maps data: the plot area is a core `stack` with
`.viewbox(xlo, yhi, xhi, ylo)`; lines and bars are `path`s with the same
viewbox, markers, ticks and error-bar caps are children placed `.at(x, y)` in
data units and centred with `.anchor`. Around it, in the same data units,
hang a band of y tick labels (a viewbox stack spanning the axis, as wide as its
widest label) right-aligned to `x0` with the turned y label before it, and a
band of x tick labels and the x label below `y0` (category names as equal
cells, one per slot). All of it sits in one [`contain`](#user-coordinates-viewbox--anchors)
stack, so the area shrinks until every label fits the plot's box: each margin
*is* the measured size of its labels — including the half labels overhanging
the axis ends — and nothing spills past the panel. The layout performs the
mapping at the plot's final size, so plots stretch, flex and auto-size like any
other node — and every piece is a plain node you could have written by hand. Ticks are "nice"
1/2/5 × 10ⁿ steps with at most `max_ticks()` labels and shared decimals
(`0.0, 0.5, 1.0`, real minus signs); log axes tick each decade (labelled
`0.01 … 1000`, or `10⁻⁵`-style past that) with minor ticks at 2–9.
Data outside an explicit range is clipped (lines are cut at the frame,
markers outside are dropped); a marker or `marks` node reaching past the plot's
box makes room for itself like a label does. Note: error bars are vertical
only.

### Posters

A conference poster — header, numbered section bars, a column grid with
dashed separators, frames, tables, bullets — is part of the framework too
([`examples/poster_demo.figs`](examples/poster_demo.figs): a full A0 poster
in the dark PowerPoint look in 98 lines (87 without comments and blank lines), nearly all of them content):

```rhai
style("poster-dark");

poster(#{
    logos: ["logo.png"],
    title: "Curvature control in supramolecular gels",
    authors: [#{ name: "A. Author", aff: 1, presenter: true }, #{ name: "B. Author", aff: [1, 2] }],
    affiliations: ["Univ. of Somewhere", "Institute of Elsewhere"],
    number: "P1-001",
}, rows([
    columns([
        section("Background", [image("a.png").aspect(1.6), body("…"), ref("C. Author et al., J. Imag. 2020, 1, 1.")]),
        section("Methods", [frame(image("setup.png").aspect(3)), subheading("Samples"),
                            table([["Sample", "Light dose"], ["Gel A", "20 mW/cm²"]])]),
    ], [3, 2]),
    section("Conclusion", bullets(["First finding.", "Second finding."])),
], [5, 1]))
```

| Function | Result |
|---|---|
| `poster(header, body)` / `poster(preset, header, body)` | A `poster_size()` page (default `"a0"`; any paper preset, `-landscape` too) on `poster_bg()` (a colour or gradient; default `page_bg()`), the header above the body, `poster_margin()` from the edges. `header` is a node, a map for `poster_header`, or `()`; `body` an array (→ `rows`) or a node. |
| `poster_header(#{ title, authors, affiliations, logos, number })` | Logos (`poster_logo_height()` tall) on the left; the title (`poster_title_size()`, bold) and the author line centred right of them; the poster number bold in the top-right corner; a `poster_header_rule()` under it all. |
| `author_line(authors, affiliations)` | Authors are names (strings or rich text: `markup("**A. Author**")`) or `#{ name, aff: [1, 2], presenter: true }`; affiliations are strings or rich text too. With `affiliation_position()` `"before"` (default, the Japanese convention): `（Univ. A¹・Univ. B²）○A. Author¹・B. Author¹,²` on one line; `"below"`: the authors, then the numbered affiliations. The line is one text node of rich-text spans — affiliation numbers are superscript spans — so it wraps like any text; a single affiliation is not numbered. |
| `section(title, content)` / `section(number, title, content)` | A bar (`section_bar_height()`, `section_bar_fill()`, `section_bar_border()`, square unless `section_bar_radius()`) with the title centred on it, over `content` (an array of items `section_gap()` apart, or one node) padded `section_pad()`. Numbered `section_label_style()` (`"1."` → "1. Title") in the order sections are built; an integer `number` restarts the count from it, a string is used as is (`""` for none). The title is a string, rich text, or a text node used as is (the number goes before it in the bar's title style; `section_title(s)` gives that style to chain setters on). Effects: `section_title_glow()`, `section_title_shadow()`, `section_bar_shadow()`. |
| `columns(items)` / `columns(items, widths)` / `columns(items, widths, opts)` | Side by side, `poster_gutter()` apart, with a dashed `vsep` in each gutter (`separators()`), starting `separator_inset()` down (the middle of the bars). Widths: flex weights, `"12cm"` lengths, `"auto"`. Options (also as the second argument): `gutter` — one length or one per gutter (so each row of a poster can have its own); `separators` — `true` / `false`, or one per gutter: `true`, `false`, a number (a separator ending that many cm below the top of the row) or `#{ start, end }` (cm from the top of the row; `start` defaults to the bar middle, `end` to the bottom). |
| `rows(items)` / `rows(items, heights)` / `rows(items, heights, opts)` | Down the page, `poster_row_gap()` apart (dashed `hsep`s with `row_separators()`); same sizes as `columns`. Options: `gaps` (one or one per gap), `separators` (`true` / `false`, or one per gap). |
| `vsep()` / `vsep(w)`, `hsep()` / `hsep(h)` | The dashed separators (`separator_color()`, `separator_width()`, `separator_dash()`) on their own: vertical in a gutter `w` wide / horizontal across the parent in a band `h` tall. |
| `frame(content)` / `dashed_frame(content)` | A thin rounded border box (`frame_color()`, `frame_width()`, `frame_radius()`, `frame_pad()`; `frame_dash()`). Square grey boxes are core setters: `frame(..).radius(0).stroke("#7f7f7f")`. |
| `note(s)` / `ref(s)` / `subheading(s)` | The highlighted take-home sentence (centred, `note_color()`) / a reference line (`ref_size()`, centred) / `"■ " + s` (`subheading_mark()`, `subheading_size()`; the mark bold on its own with `subheading_mark_bold()`, sized with `subheading_mark_size()`). |
| `bullets(items)` | A list with a hanging indent: each item in a cell `bullet_indent()` right of its `bullet_mark()`, so wrapped lines align; an array item is a nested list. The mark has its own size, `bullet_mark_size()`, kept on the first line's baseline. |
| `table(rows)` / `table(rows, opts)` | Rows of cells (strings, numbers, spans, nodes; `colspan(n, x)` spans n columns); the first row a bold header above a `table_header_rule_width()` rule. Options `header` (false: none), `widths`, `align` (one or per column), `rules` (`"grid"`, `"rows"`, `"booktabs"`, `"none"`), `pad` / `pad_x` / `pad_y`. Rules are filled rects between the cells, so the grid is exact (a spanning cell covers the rules between its columns); cells are padded `table_pad_x()` left and right, `table_pad_y()` top and bottom (both default `table_pad()`), filled `table_fill()` / `table_header_fill()`. |

All sizes derive from `base_size()`, so `style("poster")` (24 pt body) on A0
gives the proportions of a typical PowerPoint poster: 53.5 pt titles on 2.3 cm
bars, 80 pt poster title, 40 pt author line, 0.54 cm gutters, 1.5 pt dashed
separators (12 : 4.5 pt dashes), 1 pt table rules with a 3 pt header rule.
`style("poster")` is the light look (navy bars on white, grey separators and
frames); `style("poster-dark")` the dark one — a navy page gradient lit from
the top, top-lit bars (`#3d4a73` → `#181d2d` → `#080a10`) with a hairline
pale-cyan border and a soft drop shadow, titles in a subtle dark glow and a
tight soft shadow (the PowerPoint text effects), white text, separators and
frames, amber notes (`#ffc000`), table cells a shade off the page — plus the
dark plot theme. The effect hooks take `false`, `true` (that look, scaled
with `base_size()`) or a map (`#{ blur, color }` for the glow,
`#{ dx, dy, blur, spread, color }` for shadows). Every hook is a `style` key
as well:

```rhai
fn section_bar_radius() { 0.3 }        // rounded bars (default 0: square)
fn section_label_style() { "" }        // no numbers (default "1.")
fn separators() { false }              // plain gutters (default true)
style(#{ note_color: "#ff6b5a", poster_gutter: 1.0, table_rules: "booktabs" });
style(#{ section_title_glow: #{ blur: 0.15, color: "#000000c0" }, section_bar_shadow: false });
```

Hooks: `poster_size`, `poster_bg`, `poster_margin`, `poster_gutter`,
`poster_row_gap`, `poster_title_size`, `poster_title_font`, `author_size`,
`affiliation_position`, `author_sep`, `presenter_mark`,
`poster_number_size`, `poster_logo_height`, `poster_header_rule`,
`poster_header_rule_width`, `section_label_style`, `section_title_size`,
`section_title_color`, `section_title_font`, `section_bar_height`,
`section_bar_fill`, `section_bar_border`, `section_bar_border_width`,
`section_bar_radius`, `section_pad`, `section_gap`, `separators`,
`row_separators`, `separator_color`, `separator_width`, `separator_dash`,
`separator_inset`, `frame_color`, `frame_width`, `frame_radius`, `frame_pad`,
`frame_dash`, `note_color`, `note_size`, `note_font`, `ref_size`, `ref_color`,
`bullet_mark`, `bullet_size`, `bullet_indent`, `bullet_gap`,
`subheading_mark`, `subheading_size`, `table_size`, `table_rule`,
`table_rule_width`, `table_header_rule_width`, `table_rules`, `table_pad`,
`table_fill`, `table_header_fill`, `section_title_glow`,
`section_title_shadow`, `section_bar_shadow`, `table_pad_x`, `table_pad_y`,
`subheading_mark_bold`, `subheading_mark_size`, `bullet_mark_size`. Lengths are cm; the section counter is
fresh for every evaluation. A section title is anchored on the bar's centre,
so it stays centred even when a CJK font's line box is taller than the bar.
For Japanese posters set a font list once — `style("poster-dark", #{
font_family: "Arial, Yu Gothic" })` puts Latin in Arial and Japanese in Yu
Gothic, as PowerPoint does. Every text argument (titles, section titles,
author names and affiliations, items, cells, notes, refs) also takes rich text:
`ref(markup("A. Author *et al.*, *J. Imag.* **2021**, *4*, 101."))`,
`section(markup("H_2O *in situ*"), …)`.

In the editor (`figs-editor`), the **Script** tab is the source of truth: press
**Evaluate** (Ctrl+Enter) to lower the script into the editable **TOML** tab, the
inspector and the live preview. Authoring flows Rhai → (TOML ↔ GUI) one way; use
*Export TOML* to write out the lowered document. `figs-editor figure.figs` opens
a file directly and reloads it when it changes on disk (see
[Watch mode & live reload](#watch-mode--live-reload)).

### Canvas editing (drag to position)

Children of a `stack` can be placed directly on the editor's canvas; the text
source stays the truth, and every move is written back into it.

- **Drag** a stack child to set its `x`/`y`. The preview updates live, a small
  readout shows the offset (page units, or the parent's `viewbox` units), and
  the edit is committed on release. Esc cancels a drag. Pressing on something
  inside the selected node drags the selected node, so a selected container
  can be moved by its content.
- **Snapping**: edges and centre snap to the parent's content-box edges/centre
  and to sibling edges within 5 screen pixels (a magenta guide shows the
  match); hold **Alt** to place freely. A free drag is rounded to the precision
  of one screen pixel (e.g. `0.01` cm); a snapped axis keeps up to 4 decimals,
  so the alignment stays exact.
- **Arrow keys** nudge the selected stack child by one *screen pixel* at the
  current zoom, rounded down to a 1-2-5 step in page units (e.g. `0.02` cm),
  so values stay short decimals; **Shift** nudges 10×. (A pixel of the output
  DPI was rejected: at 300 dpi it gives steps like `0.00847` cm.) Nudges work
  when the canvas has focus (click it) or no text field does.
- **Undo / Redo** (Ctrl+Z, Ctrl+Shift+Z / Ctrl+Y, or the menu buttons) undo
  canvas moves; a run of arrow presses on one node undoes as one step. Typing
  in the source panes keeps its own undo; inspector edits are not recorded.
- **Not draggable**: children of a `column`/`row` (their position is computed
  by layout), the root, and a stack child on an axis where it *fills* the
  stack (no size and no offset — giving it an offset would change its size).
  The canvas shows why, and the cursor says no.

Where the move is written:

- **Script** (`.figs`): the offset is rewritten in the `.at(x, y)` / `.x(n)` /
  `.y(n)` call that set it, e.g. `.at(6, 4)` → `.at(8.04, 5.21)`. The editor
  records each call's source position during evaluation and only rewrites
  plain number literals in your script. An axis whose value is an expression
  or variable (`.at(k(), 1)`), or a node placed by a framework component
  (`circle(cx, cy, r)`, `panels`, `scalebar`, …), is locked with a notice
  such as "position is computed in script (line 9)" — the script is never
  guessed at. A call inside your own `fn` places every node it makes, so
  dragging one label of `labelled(...)` moves them all (the notice says so).
  Evaluate pending script edits before dragging.
- **TOML**: `x`/`y` are set in the node's table in place (via `toml_edit`), so
  comments and formatting survive, and saving writes that text.

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
- **rotation**: an odd quarter turn measures the node with the constraints'
  axes swapped and reports its size swapped back, so the parent sees the turned
  footprint; arrange gives the node its slot with width and height exchanged
  about the same centre. Other angles don't touch layout. See
  [Rotation](#rotation).
- **text boxes**: a text block is its stacked line boxes (`font_size ×
  line_height` each, glyphs centred per CSS half-leading), grown at the top and
  bottom when the font's ascent/descent is taller than the line box (e.g. Segoe
  UI at the default 1.2), so accents and descenders never poke out and get
  clipped at a page edge. Spacing between lines is unchanged. Glyphs may still
  sit flush with the box edge — the box is the font's metrics, with no extra
  padding; add `spacing`/`pad` for breathing room. With [spans](#rich-text-spans)
  each line box follows its largest run; super/subscripts count at their base
  size, unshifted.

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
| Text `stroke` | glyph outlines stroked (tiny-skia), glyphs blended on top | vector path under the text (text layer unchanged) | one stroked `<path>` before the glyph group |
| Rich text | per-glyph colour/size/face; scripts on shifted baselines; synthetic italic via swash's skew | one glyph run per (face, size, span) with its fill; the string still appears **once** (all runs share the text and ToUnicode ranges, so `pdftotext` gives `ΔG300K = −27.5`); synthetic italic = `cm` shear | one `aria-label` group (full text) with a `fill` group per colour run; synthetic italic in each `<use>` matrix |
| Underline / strike | filled bars | filled vector rects | `<rect>`s |
| Images | resampled (bilinear) | JPEG passed through untouched; other formats embedded losslessly; `/Interpolate true` | base64 `data:` URI (PNG/JPEG bytes verbatim) |
| `smooth = false` | nearest-neighbour | upscaled nearest-neighbour to ~300 dpi, `/Interpolate` off | `image-rendering: pixelated` + the same upscale |
| `shadow` / `glow` | blurred silhouette at the output dpi, blended under the node | the same raster as an image (soft mask, interpolated) | the same raster as `<image class="figs-shadow">` |
| `opacity` | layer composited at that alpha | transparency group (`/ca`) | `<g opacity>` |
| `cover` | clipped | clip path | `clipPath` |
| Gradients | tiny-skia linear / two-point radial shaders | axial / radial shadings (soft mask for translucent stops) | `<linearGradient>` / `<radialGradient>` in `<defs>`, `userSpaceOnUse` |
| `clip = true` | canvas snapshot restored outside an AA mask | `W n` clip path | `<g clip-path>` |
| Missing image | gray placeholder | same placeholder | same placeholder |

- Coordinates are points throughout. The IR's top-left origin is kept in SVG
  (`viewBox` in pt); PDF's bottom-left flip is handled by krilla, and the
  MediaBox is exactly `page.width × page.height` pt.
- `dpi` only affects PNG (and caps the resolution of shadow rasters embedded in
  PDF/SVG, at most 300 dpi).
- **Rotation** is applied per node from the IR transform: PNG draws through a
  tiny-skia `Transform` (rotated text is filled from glyph outlines, since glyph
  bitmaps only blit upright), PDF wraps the node in `q … cm … Q` (rotated text
  stays real, selectable text), SVG in `<g transform="matrix(…)">`.
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
