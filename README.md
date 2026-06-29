# figs_rs

A fast, declarative tool for composing paper figures and posters. You describe
**structure and constraints** in TOML; the engine computes every coordinate and
size for you (no manual positioning), then renders to PNG/PDF. Think
Flutter/XAML-style relative layout, aimed at research figures.

> Status: **Phase 1 feature-complete.** TOML → layout → PNG/PDF works
> end-to-end with shaped text (incl. CJK), images, a file-watching CLI, and an
> egui live-preview window. See [Usage](#usage) and [Milestones](#milestones).

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

## Architecture

```
poster.toml
  → schema::parse   (TOML → RawDocument)
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

## Usage

```sh
# Render once (format from the output extension)
cargo run -p figs-cli -- build examples/four_panel.toml -o out.png
cargo run -p figs-cli -- build examples/poster.toml     -o out.pdf

# Re-render on every change to the document
cargo run -p figs-cli -- watch examples/poster.toml -o out.png

# Live preview window (needs a desktop; not built in headless CI)
cargo run -p figs-preview -- examples/poster.toml

# WYSIWYG editor: click to select, edit properties, Save writes back the TOML
# (comments/formatting preserved). Needs a desktop.
cargo run -p figs-editor -- examples/poster.toml
```

PDF output embeds subset fonts (selectable text, including CJK) and images as
XObjects; PNG rasterizes at `page.dpi` (default 300).

## Build & test

```sh
cargo test                 # core + cli (figs-preview is excluded by default)
cargo clippy --all-targets
cargo check -p figs-preview # type-check the GUI crate
```

## Milestones

All Phase 1 milestones are implemented:

M0 workspace ✓ · M1 schema+resolve ✓ · M2 layout (column/row/rect, flex, align,
aspect, overflow) ✓ · M3 text shaping (cosmic-text) ✓ · M4 PNG backend ✓ ·
M5 image + aspect/fit ✓ · M6 PDF backend (printpdf, embedded fonts) ✓ ·
M7 CLI + file watch ✓ · M7′ egui live preview ✓.

Next: M8 hardening (bundled default font for cross-machine determinism, richer
diagnostics) and Phase 2 (Tauri WYSIWYG editor with TOML round-tripping).

## License

MIT OR Apache-2.0.
