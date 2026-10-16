//! Direct manipulation on the canvas: drag a `stack` child to set its `x`/`y`
//! offset (with a live preview, snapping and an x/y readout), nudge it with
//! the arrow keys, and write the result back to the source of truth — the
//! script's number literals, or the TOML document — as one undoable edit.
//!
//! Coordinates, outermost first:
//! - **screen**: egui points on the canvas ([`CanvasView`]);
//! - **page**: layout points, top-left origin (`ComputedLayout` rects);
//! - **offset**: the value of a child's `x`/`y` — page units from the parent
//!   stack's content-box origin, or the parent's `viewbox` user units
//!   ([`OffsetFrame`]). The child's offset point is its border box's
//!   `anchor` point, before its leading margin.
//!
//! Only children of a `stack` move: elsewhere the layout computes positions.
//! An axis on which the child *fills* the stack (no size and no offset) is
//! locked too, since giving it an offset would change its size.

use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Pos2, Stroke};
use figs_core::geom::{Rect as PRect, ViewBox};
use figs_core::schema::{NodeKind, RawNode};
use figs_core::script::OffsetSite;

use crate::app::{FigsApp, Selection};
use crate::history::Snapshot;
use crate::script_edit::{self, LiteralEdit};
use crate::theme::Palette;

/// Snap distance, in screen pixels.
pub const SNAP_PX: f32 = 5.0;
/// How long a canvas notice stays up.
const NOTICE_FOR: Duration = Duration::from_secs(5);
/// Arrow presses this close together on one node undo as one edit.
const NUDGE_GROUP: Duration = Duration::from_millis(1200);

// ---------------------------------------------------------------------------
// Coordinate conversion
// ---------------------------------------------------------------------------

/// Where the page is drawn: page point `(0, 0)` at `origin`, `scale` screen
/// points per page point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasView {
    pub origin: Pos2,
    pub scale: f32,
}

impl CanvasView {
    pub fn page_pos(&self, p: Pos2) -> [f32; 2] {
        [
            (p.x - self.origin.x) / self.scale,
            (p.y - self.origin.y) / self.scale,
        ]
    }

    pub fn screen_pos(&self, [x, y]: [f32; 2]) -> Pos2 {
        egui::pos2(
            self.origin.x + x * self.scale,
            self.origin.y + y * self.scale,
        )
    }

    /// Page points per screen pixel.
    pub fn px(&self) -> f32 {
        1.0 / self.scale
    }
}

/// How a stack child's offset values map to page points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OffsetFrame {
    /// The parent stack's content box (page points).
    pub content: PRect,
    pub viewbox: Option<ViewBox>,
    /// Points per page unit.
    pub unit_pt: f32,
}

fn lo(r: PRect, axis: usize) -> f32 {
    if axis == 0 {
        r.x
    } else {
        r.y
    }
}

fn len(r: PRect, axis: usize) -> f32 {
    if axis == 0 {
        r.w
    } else {
        r.h
    }
}

impl OffsetFrame {
    /// The frame of a stack laid out at `rect` with `padding` (page units).
    pub fn new(
        rect: PRect,
        padding: figs_core::geom::Edges,
        viewbox: Option<ViewBox>,
        unit_pt: f32,
    ) -> Self {
        OffsetFrame {
            content: rect.inset(padding.scaled(unit_pt)),
            viewbox,
            unit_pt,
        }
    }

    /// Offset value on `axis` (0 = x, 1 = y) -> absolute page point.
    pub fn pt_of(&self, axis: usize, v: f32) -> f32 {
        match (self.viewbox, axis) {
            (Some(vb), 0) => vb.map_x(v, self.content),
            (Some(vb), _) => vb.map_y(v, self.content),
            (None, _) => lo(self.content, axis) + v * self.unit_pt,
        }
    }

    /// Absolute page point on `axis` -> offset value.
    pub fn value_of(&self, axis: usize, pt: f32) -> f32 {
        let rel = pt - lo(self.content, axis);
        match self.viewbox {
            Some(vb) => {
                let (v0, v1) = if axis == 0 {
                    (vb.x0, vb.x1)
                } else {
                    (vb.y0, vb.y1)
                };
                let size = len(self.content, axis);
                if size <= 0.0 {
                    v0
                } else {
                    v0 + rel / size * (v1 - v0)
                }
            }
            None => rel / self.unit_pt.max(1e-6),
        }
    }

    /// The value an unset offset stands for: the content-box origin.
    pub fn origin_value(&self, axis: usize) -> f32 {
        self.value_of(axis, lo(self.content, axis))
    }

    /// Offset units per page point on `axis` (signed: negative when a
    /// viewbox axis is reversed, e.g. a y axis pointing up).
    pub fn units_per_pt(&self, axis: usize) -> f32 {
        let o = lo(self.content, axis);
        self.value_of(axis, o + 1.0) - self.value_of(axis, o)
    }
}

/// Per axis: does a stack child fill the stack (no explicit size, and no
/// offset)? Mirrors the layout's rule (`stack_child_fills`).
pub fn axis_fills(n: &RawNode) -> [bool; 2] {
    let ar = n.aspect_ratio.filter(|a| *a > 0.0);
    let pinned_w = n.width.is_some() || (ar.is_some() && n.height.is_some());
    let pinned_h = n.height.is_some() || (ar.is_some() && n.width.is_some());
    [!pinned_w && n.x.is_none(), !pinned_h && n.y.is_none()]
}

// ---------------------------------------------------------------------------
// Snapping and rounding
// ---------------------------------------------------------------------------

/// A snap: move by `delta` so an edge/center lands on the guide at `at`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    pub delta: f32,
    pub at: f32,
}

/// The closest snap of the span `[lo, hi]` — its start, center or end — onto
/// any of `targets` within `threshold`.
pub fn snap_axis(lo: f32, hi: f32, targets: &[f32], threshold: f32) -> Option<Snap> {
    let mut best: Option<Snap> = None;
    for edge in [lo, (lo + hi) / 2.0, hi] {
        for &t in targets {
            let d = t - edge;
            if d.abs() <= threshold && best.is_none_or(|b| d.abs() < b.delta.abs()) {
                best = Some(Snap { delta: d, at: t });
            }
        }
    }
    best
}

/// Snap targets on `axis`: the parent content box's edges and center, and
/// the siblings' edges.
pub fn snap_targets(content: PRect, siblings: &[PRect], axis: usize) -> Vec<f32> {
    let (a, l) = (lo(content, axis), len(content, axis));
    let mut t = vec![a, a + l / 2.0, a + l];
    for s in siblings {
        t.push(lo(*s, axis));
        t.push(lo(*s, axis) + len(*s, axis));
    }
    t
}

/// Decimals that resolve `step` (a length in offset units): `0.01` -> 2.
pub fn decimals_for(step: f32) -> usize {
    let step = step.abs();
    if !(step > 0.0 && step.is_finite()) {
        return 2;
    }
    ((-step.log10() - 1e-4).ceil().max(0.0) as usize).min(4)
}

fn decimals_of(v: f32) -> usize {
    script_edit::format_num(v as f64)
        .split_once('.')
        .map_or(0, |(_, f)| f.len())
}

pub fn round_dec(v: f32, decimals: usize) -> f32 {
    let m = 10f64.powi(decimals as i32);
    ((v as f64 * m).round() / m) as f32
}

/// The nudge step for `unit_per_px` offset units per screen pixel: the
/// largest 1-2-5 value not above one pixel, so a press moves at most a pixel
/// and offsets stay short decimals (0.01, 0.02, 0.05, 0.1, …).
pub fn nice_step(unit_per_px: f32) -> f32 {
    let u = unit_per_px.abs();
    if !(u > 0.0 && u.is_finite()) {
        return 1.0;
    }
    // An exact power of ten (`1 / 100`, not `10^-2.0`), so steps are clean.
    let k = u.log10().floor() as i32;
    let base = if k < 0 {
        1.0 / 10f32.powi(-k)
    } else {
        10f32.powi(k)
    };
    [5.0, 2.0, 1.0]
        .into_iter()
        .map(|m| m * base)
        .find(|s| *s <= u * (1.0 + 1e-4))
        .unwrap_or(base)
}

// ---------------------------------------------------------------------------
// Planning a move
// ---------------------------------------------------------------------------

/// Everything a drag or nudge of one node needs, captured when it starts.
#[derive(Clone, Debug)]
pub struct MovePlan {
    pub id: String,
    pub frame: OffsetFrame,
    /// The node's border box at the start (page points).
    pub start_rect: PRect,
    /// The offset point (anchor point before margin) at the start.
    pub start_point: [f32; 2],
    /// Which axes may move.
    pub axes: [bool; 2],
    /// The offset values at the start (`None`: unset).
    pub start: [Option<f32>; 2],
    /// Script mode: the call that sets each axis.
    pub sites: [Option<OffsetSite>; 2],
    /// Script mode: other nodes made by the same call (they move too).
    pub mirrors: [Vec<String>; 2],
    pub siblings: Vec<PRect>,
    /// Why an axis is locked, if one is.
    pub note: Option<String>,
}

/// Check that `id` can be moved on the canvas and capture its geometry, or
/// say why not.
pub fn plan_move(app: &FigsApp, id: &str) -> Result<MovePlan, String> {
    let prev = app.preview.as_ref().ok_or("no preview")?;
    let doc = &app.model;
    let node = doc.nodes.get(id).ok_or("no such node")?;
    let rect_of = |id: &str| {
        prev.computed
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.rect)
    };
    let parent_id =
        crate::model::parent_of(doc, id).ok_or("the root fills the page: it has no offset")?;
    let parent = &doc.nodes[&parent_id];
    if parent.kind != NodeKind::Stack {
        return Err(format!(
            "position is computed by layout (parent `{parent_id}` is a {}); only stack children move",
            crate::app::kind_label(parent.kind)
        ));
    }
    let (Some(rect), Some(prect)) = (rect_of(id), rect_of(&parent_id)) else {
        return Err("not laid out".into());
    };
    let unit_pt = doc.page.unit.to_pt();
    let viewbox = parent.viewbox.and_then(|v| ViewBox::from_array(v).ok());
    let frame = OffsetFrame::new(prect, parent.padding, viewbox, unit_pt);

    let fills = axis_fills(node);
    let mut axes = [!fills[0], !fills[1]];
    if !axes[0] && !axes[1] {
        return Err(format!(
            "`{id}` fills its stack (no size, no offset): give it a width/height or an offset to move it"
        ));
    }
    let mut notes: Vec<String> = Vec::new();
    for (a, name) in ["x", "y"].into_iter().enumerate() {
        if fills[a] {
            notes.push(format!("{name} locked: fills the stack"));
        }
    }

    let start = [node.x, node.y];
    let mut sites = [None, None];
    let mut mirrors = [Vec::new(), Vec::new()];
    if app.script_is_truth() {
        if app.evaluated_script.as_deref() != Some(app.script_buffer.as_str()) {
            return Err("the script has unevaluated edits: Evaluate (Ctrl+Enter) first".into());
        }
        let found = app.script_sites.get(id).copied().unwrap_or_default();
        for (a, name) in ["x", "y"].into_iter().enumerate() {
            if !axes[a] {
                continue;
            }
            let site = if a == 0 { found.x } else { found.y };
            let checked = match (site, start[a]) {
                (Some(site), Some(cur)) => {
                    script_edit::offset_literal(&app.script_buffer, &site, a, cur).map(|_| site)
                }
                _ => Err(script_edit::Blocked::NoSite),
            };
            match checked {
                Ok(site) => {
                    sites[a] = Some(site);
                    mirrors[a] = app
                        .script_sites
                        .iter()
                        .filter(|(other, s)| {
                            other.as_str() != id && (if a == 0 { s.x } else { s.y }) == Some(site)
                        })
                        .map(|(other, _)| other.clone())
                        .collect();
                    mirrors[a].sort();
                }
                Err(why) => {
                    axes[a] = false;
                    notes.push(format!("{name}: {why}"));
                }
            }
        }
        if !axes[0] && !axes[1] {
            return Err(notes.join("; "));
        }
    } else if let Some(e) = &app.toml_error {
        return Err(format!("fix the TOML error first ({e})"));
    }

    let [ax, ay] = node.anchor.unwrap_or([0.0, 0.0]);
    let start_point = [
        rect.x + ax * rect.w - node.margin.left * unit_pt,
        rect.y + ay * rect.h - node.margin.top * unit_pt,
    ];
    let siblings = parent
        .children
        .iter()
        .filter(|c| c.as_str() != id)
        .filter_map(|c| rect_of(c))
        .collect();
    Ok(MovePlan {
        id: id.to_string(),
        frame,
        start_rect: rect,
        start_point,
        axes,
        start,
        sites,
        mirrors,
        siblings,
        note: (!notes.is_empty()).then(|| notes.join("; ")),
    })
}

/// A snap guide: a line at `at` across `axis` (0: vertical line at x = at),
/// spanning `from..to` on the other axis (page points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    pub axis: usize,
    pub at: f32,
    pub from: f32,
    pub to: f32,
}

/// The offsets for a drag of `delta` page points, snapped unless `snap` is
/// false; `px` is page points per screen pixel (rounding and snap distance).
pub fn drag_values(
    plan: &MovePlan,
    delta: [f32; 2],
    snap: bool,
    px: f32,
) -> ([Option<f32>; 2], Vec<Guide>) {
    let mut values = plan.start;
    let mut d = [0.0f32; 2];
    let mut snaps: Vec<(usize, f32)> = Vec::new();
    for a in 0..2 {
        if !plan.axes[a] {
            continue;
        }
        d[a] = delta[a];
        let mut snapped = false;
        if snap {
            let lo0 = lo(plan.start_rect, a) + d[a];
            let targets = snap_targets(plan.frame.content, &plan.siblings, a);
            if let Some(s) = snap_axis(lo0, lo0 + len(plan.start_rect, a), &targets, SNAP_PX * px) {
                d[a] += s.delta;
                snapped = true;
                snaps.push((a, s.at));
            }
        }
        if d[a] == 0.0 {
            continue; // unmoved: keep the value exactly as written
        }
        let v = plan.frame.value_of(a, plan.start_point[a] + d[a]);
        // A free drag resolves one screen pixel; a snapped one keeps the
        // alignment exact (up to 4 decimals).
        let decimals = if snapped {
            4
        } else {
            decimals_for(plan.frame.units_per_pt(a) * px)
        };
        values[a] = Some(round_dec(v, decimals));
    }
    let moved = PRect::new(
        plan.start_rect.x + d[0],
        plan.start_rect.y + d[1],
        plan.start_rect.w,
        plan.start_rect.h,
    );
    let guides = snaps
        .into_iter()
        .map(|(a, at)| {
            let o = 1 - a;
            let c = plan.frame.content;
            Guide {
                axis: a,
                at,
                from: lo(c, o).min(lo(moved, o)),
                to: (lo(c, o) + len(c, o)).max(lo(moved, o) + len(moved, o)),
            }
        })
        .collect();
    (values, guides)
}

/// The offsets after `steps` arrow presses per axis (`mult`: 10 with Shift),
/// in screen-pixel steps (see [`nice_step`]); `px` as in [`drag_values`].
pub fn nudge_values(plan: &MovePlan, steps: [i32; 2], mult: f32, px: f32) -> [Option<f32>; 2] {
    let mut values = plan.start;
    for a in 0..2 {
        if steps[a] == 0 || !plan.axes[a] {
            continue;
        }
        let per_pt = plan.frame.units_per_pt(a);
        let step = nice_step(per_pt * px) * mult;
        let base = plan.start[a].unwrap_or_else(|| plan.frame.origin_value(a));
        // Screen-down/right is +pt; a reversed viewbox axis counts down.
        let v = base + steps[a] as f32 * step * per_pt.signum();
        let decimals = decimals_for(step).max(decimals_of(base)).min(4);
        values[a] = Some(round_dec(v, decimals));
    }
    values
}

// ---------------------------------------------------------------------------
// Applying and committing
// ---------------------------------------------------------------------------

/// A drag in progress.
pub enum Drag {
    Moving(Box<Moving>),
    /// The press landed on something that can't move (cursor shows it).
    Blocked,
}

pub struct Moving {
    pub plan: MovePlan,
    /// State before the drag (undo, cancel).
    pub before: Snapshot,
    pub press: Pos2,
    pub values: [Option<f32>; 2],
    pub guides: Vec<Guide>,
}

/// Show the offsets live: set them on the node (and nodes sharing its call).
fn apply_preview(app: &mut FigsApp, plan: &MovePlan, values: [Option<f32>; 2]) {
    for a in 0..2 {
        let ids = std::iter::once(&plan.id).chain(&plan.mirrors[a]);
        for id in ids {
            if let Some(n) = app.model.nodes.get_mut(id) {
                if a == 0 {
                    n.x = values[0];
                } else {
                    n.y = values[1];
                }
            }
        }
    }
    app.needs_rebuild = true;
}

/// Write `values` into the source of truth as one undoable edit (`group`:
/// fold it into the previous edit, for runs of arrow presses). Returns a
/// notice to show.
fn write_back(
    app: &mut FigsApp,
    plan: &MovePlan,
    values: [Option<f32>; 2],
    before: Snapshot,
    group: bool,
) -> Result<String, String> {
    let changed = [values[0] != plan.start[0], values[1] != plan.start[1]];
    let fmt = |v: Option<f32>| v.map_or("-".to_string(), |v| script_edit::format_num(v as f64));
    let summary = format!("x {}, y {}", fmt(values[0]), fmt(values[1]));
    if app.script_is_truth() {
        let mut edits = Vec::new();
        let mut lines = Vec::new();
        for a in 0..2 {
            if !changed[a] {
                continue;
            }
            let (Some(site), Some(cur), Some(v)) = (plan.sites[a], plan.start[a], values[a]) else {
                return Err("internal: axis without a script site".into());
            };
            let (range, _) = script_edit::offset_literal(&app.script_buffer, &site, a, cur)
                .map_err(|e| e.to_string())?;
            edits.push(LiteralEdit {
                range,
                text: script_edit::format_num(v as f64),
            });
            lines.push(site.line);
        }
        let new_src = script_edit::apply_edits(&app.script_buffer, edits);
        app.script_buffer = new_src;
        app.evaluate_script();
        if let Some(e) = app.script_error.clone() {
            // Never leave a broken script behind: restore it.
            app.script_buffer = before.script;
            app.evaluate_script();
            return Err(format!("rewrite failed: {e}"));
        }
        if !group {
            app.history.push(before);
        }
        lines.dedup();
        let shared = plan.mirrors[0].len().max(plan.mirrors[1].len());
        let mut msg = format!(
            "{summary} \u{2192} script line {}",
            lines
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
        if shared > 0 {
            msg.push_str(&format!(
                " (the same call also places {shared} other node{})",
                if shared == 1 { "" } else { "s" }
            ));
        }
        Ok(msg)
    } else {
        let in_sync = app.toml_in_sync();
        apply_preview(app, plan, values);
        let patched = in_sync
            .then(|| {
                crate::model::patch_toml_offset(
                    &app.toml_buffer,
                    &plan.id,
                    values[0].filter(|_| changed[0]),
                    values[1].filter(|_| changed[1]),
                )
            })
            .flatten()
            .and_then(|t| figs_core::schema::parse::parse_str(&t).ok().map(|d| (t, d)));
        match patched {
            // Format-preserving: the TOML pane keeps its comments.
            Some((text, doc)) => {
                app.toml_buffer = text;
                app.model = doc;
            }
            None => app.regen_toml = true,
        }
        app.dirty = true;
        app.needs_rebuild = true;
        if !group {
            app.history.push(before);
        }
        Ok(format!("{summary} ({})", offset_units(app, plan)))
    }
}

fn offset_units(app: &FigsApp, plan: &MovePlan) -> &'static str {
    if plan.frame.viewbox.is_some() {
        "viewbox units"
    } else {
        crate::app::unit_label(app.model.page.unit)
    }
}

fn notice(app: &mut FigsApp, msg: impl Into<String>) {
    app.canvas_notice = Some((msg.into(), Instant::now()));
}

/// Nudge the selected node by arrow-key steps.
fn nudge(app: &mut FigsApp, id: &str, steps: [i32; 2], mult: f32, px: f32) {
    let plan = match plan_move(app, id) {
        Ok(p) => p,
        Err(why) => return notice(app, why),
    };
    let blocked: Vec<&str> = (0..2)
        .filter(|&a| steps[a] != 0 && !plan.axes[a])
        .map(|a| ["x", "y"][a])
        .collect();
    let values = nudge_values(&plan, steps, mult, px);
    if values == plan.start {
        if !blocked.is_empty() {
            notice(app, plan.note.clone().unwrap_or_default());
        }
        return;
    }
    let now = Instant::now();
    let group = app
        .nudge_group
        .as_ref()
        .is_some_and(|(g, t)| g == id && now.duration_since(*t) < NUDGE_GROUP);
    let before = app.snapshot();
    match write_back(app, &plan, values, before, group) {
        Ok(msg) => {
            app.nudge_group = Some((id.to_string(), now));
            notice(app, msg);
        }
        Err(e) => notice(app, e),
    }
}

// ---------------------------------------------------------------------------
// The canvas interaction (called by `panels::canvas` every frame)
// ---------------------------------------------------------------------------

/// The node a press at page point `pt` grabs: the topmost node there — or
/// the selected node when that topmost node is inside it, so a selected
/// container can be dragged by its content.
fn grab_target(app: &FigsApp, pt: [f32; 2]) -> Option<String> {
    let prev = app.preview.as_ref()?;
    let top = crate::panels::node_at_point(&prev.computed, pt[0], pt[1])?;
    if let Some(sel) = app.selected.node_id() {
        let mut cur = Some(top.clone());
        while let Some(id) = cur {
            if id == sel {
                return Some(sel.to_string());
            }
            cur = crate::model::parent_of(&app.model, &id);
        }
    }
    Some(top)
}

pub fn interact(
    app: &mut FigsApp,
    ctx: &egui::Context,
    resp: &egui::Response,
    painter: &egui::Painter,
    view: CanvasView,
    p: &Palette,
) {
    // The canvas takes keyboard focus when used, and keeps the arrow keys.
    if resp.clicked() || resp.drag_started() {
        resp.request_focus();
    }
    if resp.has_focus() {
        ctx.memory_mut(|m| {
            m.set_focus_lock_filter(
                resp.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            )
        });
    }
    let keys_free = resp.has_focus() || ctx.memory(|m| m.focused().is_none());

    if keys_free {
        use egui::{Key, Modifiers};
        let redo = ctx.input_mut(|i| {
            i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                || i.consume_key(Modifiers::COMMAND, Key::Y)
        });
        if redo {
            app.redo();
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z)) {
            app.undo();
        }
    }

    // ---- drag ----
    if resp.drag_started_by(egui::PointerButton::Primary) {
        let press = ctx.input(|i| i.pointer.press_origin()).unwrap_or_default();
        let pt = view.page_pos(press);
        app.drag = match grab_target(app, pt) {
            None => None,
            Some(id) => {
                app.selected = Selection::Node(id.clone());
                match plan_move(app, &id) {
                    Ok(plan) => {
                        if let Some(n) = &plan.note {
                            notice(app, n.clone());
                        }
                        Some(Drag::Moving(Box::new(Moving {
                            values: plan.start,
                            plan,
                            before: app.snapshot(),
                            press,
                            guides: Vec::new(),
                        })))
                    }
                    Err(why) => {
                        notice(app, why);
                        Some(Drag::Blocked)
                    }
                }
            }
        };
    }

    let cancel = app.drag.is_some()
        && ctx.input_mut(|i| i.consume_key(Default::default(), egui::Key::Escape));
    if cancel {
        if let Some(Drag::Moving(m)) = app.drag.take() {
            app.model = m.before.model;
            app.needs_rebuild = true;
        }
    }

    let pointer = ctx.input(|i| i.pointer.interact_pos());
    let alt = ctx.input(|i| i.modifiers.alt);
    let mut preview: Option<(MovePlan, [Option<f32>; 2])> = None;
    if let (Some(Drag::Moving(m)), Some(pos)) = (app.drag.as_mut(), pointer) {
        if resp.dragged() || resp.drag_stopped() {
            let delta = [
                (pos.x - m.press.x) / view.scale,
                (pos.y - m.press.y) / view.scale,
            ];
            let (values, guides) = drag_values(&m.plan, delta, !alt, view.px());
            m.guides = guides;
            if values != m.values {
                m.values = values;
                preview = Some((m.plan.clone(), values));
            }
        }
    }
    if let Some((plan, values)) = preview {
        apply_preview(app, &plan, values);
    }

    if resp.drag_stopped() {
        if let Some(Drag::Moving(m)) = app.drag.take() {
            let m = *m;
            if m.values == m.plan.start {
                app.model = m.before.model;
                app.needs_rebuild = true;
            } else {
                app.nudge_group = None;
                match write_back(app, &m.plan, m.values, m.before, false) {
                    Ok(msg) => notice(app, msg),
                    Err(e) => notice(app, e),
                }
            }
        }
        app.drag = None;
    }

    // ---- arrow-key nudge ----
    if keys_free && app.drag.is_none() {
        if let Some(id) = app.selected.node_id().map(str::to_string) {
            use egui::{Key, Modifiers};
            let mut steps = [0i32; 2];
            let mut mult = 1.0;
            for (key, axis, dir) in [
                (Key::ArrowLeft, 0, -1),
                (Key::ArrowRight, 0, 1),
                (Key::ArrowUp, 1, -1),
                (Key::ArrowDown, 1, 1),
            ] {
                // Shift first: a plain pattern also matches Shift+key.
                let n = ctx.input_mut(|i| i.count_and_consume_key(Modifiers::SHIFT, key));
                if n > 0 {
                    mult = 10.0;
                    steps[axis] += dir * n as i32;
                }
                let n = ctx.input_mut(|i| i.count_and_consume_key(Modifiers::NONE, key));
                steps[axis] += dir * n as i32;
            }
            if steps != [0, 0] {
                nudge(app, &id, steps, mult, view.px());
            }
        }
    }

    // ---- cursor ----
    match &app.drag {
        Some(Drag::Moving(_)) => ctx.set_cursor_icon(egui::CursorIcon::Grabbing),
        Some(Drag::Blocked) => ctx.set_cursor_icon(egui::CursorIcon::NotAllowed),
        None => {
            // A grab hand over the selected node when a press would move it.
            if let (Some(pos), Some(sel)) = (resp.hover_pos(), app.selected.node_id()) {
                let movable = crate::model::parent_of(&app.model, sel)
                    .and_then(|pid| app.model.nodes.get(&pid))
                    .is_some_and(|pn| pn.kind == NodeKind::Stack);
                if movable && grab_target(app, view.page_pos(pos)).as_deref() == Some(sel) {
                    ctx.set_cursor_icon(egui::CursorIcon::Grab);
                }
            }
        }
    }

    // ---- overlays ----
    if let Some(Drag::Moving(m)) = &app.drag {
        let guide = Stroke::new(1.0_f32, Color32::from_rgb(0xff, 0x3d, 0x9a));
        for g in &m.guides {
            let (a, b) = if g.axis == 0 {
                ([g.at, g.from], [g.at, g.to])
            } else {
                ([g.from, g.at], [g.to, g.at])
            };
            painter.line_segment([view.screen_pos(a), view.screen_pos(b)], guide);
        }
        let fmt =
            |v: Option<f32>| v.map_or("auto".to_string(), |v| script_edit::format_num(v as f64));
        let label = format!(
            "x {}  y {}  {}",
            fmt(m.values[0]),
            fmt(m.values[1]),
            offset_units(app, &m.plan)
        );
        let anchor = view.screen_pos([
            m.plan.start_rect.x + m.values_delta(0),
            m.plan.start_rect.y + m.values_delta(1),
        ]) - egui::vec2(0.0, 6.0);
        let font = egui::FontId::monospace(11.5);
        let galley = painter.layout_no_wrap(label, font, Color32::WHITE);
        let r = egui::Rect::from_min_size(
            anchor - egui::vec2(0.0, galley.size().y + 6.0),
            galley.size() + egui::vec2(10.0, 6.0),
        );
        painter.rect_filled(r, 4.0, Color32::from_black_alpha(200));
        painter.galley(r.min + egui::vec2(5.0, 3.0), galley, Color32::WHITE);
    }

    if let Some((msg, at)) = &app.canvas_notice {
        let age = at.elapsed();
        if age < NOTICE_FOR {
            let area = resp.rect;
            let font = egui::FontId::proportional(13.0);
            let galley = painter.layout(msg.clone(), font, p.tx, (area.width() - 48.0).max(100.0));
            let size = galley.size() + egui::vec2(16.0, 10.0);
            let r = egui::Rect::from_min_size(
                egui::pos2(area.center().x - size.x / 2.0, area.max.y - size.y - 12.0),
                size,
            );
            painter.rect_filled(r, 6.0, p.panel2);
            painter.rect_stroke(r, 6.0, Stroke::new(1.0_f32, p.line));
            painter.galley(r.min + egui::vec2(8.0, 5.0), galley, p.tx);
            ctx.request_repaint_after(NOTICE_FOR - age);
        } else {
            app.canvas_notice = None;
        }
    }
}

impl Moving {
    /// How far (page points) the node's offset point has moved on `axis`.
    fn values_delta(&self, axis: usize) -> f32 {
        match self.values[axis] {
            Some(v) if self.values[axis] != self.plan.start[axis] => {
                self.plan.frame.pt_of(axis, v) - self.plan.start_point[axis]
            }
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use figs_core::geom::Edges;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn screen_page_round_trip() {
        let v = CanvasView {
            origin: egui::pos2(100.0, 50.0),
            scale: 2.0,
        };
        assert_eq!(v.page_pos(egui::pos2(120.0, 70.0)), [10.0, 10.0]);
        assert_eq!(v.screen_pos([10.0, 10.0]), egui::pos2(120.0, 70.0));
        assert_eq!(v.px(), 0.5);
    }

    #[test]
    fn content_box_offsets_in_page_units() {
        // A stack at (10, 20) pt, 1 cm padding, cm page unit.
        let cm = 72.0 / 2.54;
        let f = OffsetFrame::new(
            PRect::new(10.0, 20.0, 200.0, 100.0),
            Edges::all(1.0),
            None,
            cm,
        );
        assert!(approx(f.content.x, 10.0 + cm));
        assert!(approx(f.pt_of(0, 2.0), 10.0 + 3.0 * cm));
        assert!(approx(f.value_of(1, 20.0 + 1.5 * cm), 0.5));
        assert!(approx(f.origin_value(0), 0.0));
        assert!(approx(f.units_per_pt(0), 1.0 / cm));
        for v in [-1.0, 0.0, 3.25] {
            assert!(approx(f.value_of(0, f.pt_of(0, v)), v));
        }
    }

    #[test]
    fn viewbox_offsets_in_user_units() {
        // A data-plot viewbox: x 0..10, y 100 (top) .. 0 (bottom).
        let vb = ViewBox::from_array([0.0, 100.0, 10.0, 0.0]).unwrap();
        let f = OffsetFrame::new(
            PRect::new(0.0, 0.0, 200.0, 100.0),
            Edges::default(),
            Some(vb),
            1.0,
        );
        assert!(approx(f.pt_of(0, 5.0), 100.0));
        assert!(approx(f.pt_of(1, 100.0), 0.0));
        assert!(approx(f.value_of(1, 25.0), 75.0));
        assert!(approx(f.origin_value(1), 100.0)); // unset = content top
        assert!(f.units_per_pt(1) < 0.0); // reversed: down on screen = smaller y
        assert!(approx(f.units_per_pt(0), 0.05));
    }

    #[test]
    fn fills_follow_the_layout_rule() {
        let mut n = crate::model::new_node(NodeKind::Image);
        assert_eq!(axis_fills(&n), [true, true]);
        n.width = Some(2.0);
        assert_eq!(axis_fills(&n), [false, true]);
        n.aspect_ratio = Some(1.5);
        assert_eq!(axis_fills(&n), [false, false]);
        let mut t = crate::model::new_node(NodeKind::Text);
        t.y = Some(0.0);
        assert_eq!(axis_fills(&t), [true, false]);
    }

    #[test]
    fn snaps_to_the_closest_edge_or_center() {
        let targets = [0.0, 50.0, 100.0];
        // Left edge 2 away from 0.
        assert_eq!(
            snap_axis(2.0, 12.0, &targets, 3.0),
            Some(Snap {
                delta: -2.0,
                at: 0.0
            })
        );
        // Center (49) is closer to 50 than any edge.
        assert_eq!(snap_axis(44.0, 54.0, &targets, 3.0).unwrap().at, 50.0);
        // Right edge onto 100.
        assert_eq!(snap_axis(89.0, 99.0, &targets, 3.0).unwrap().delta, 1.0);
        assert_eq!(snap_axis(20.0, 30.0, &targets, 3.0), None);
        let t = snap_targets(
            PRect::new(0.0, 0.0, 100.0, 40.0),
            &[PRect::new(60.0, 5.0, 10.0, 10.0)],
            0,
        );
        assert_eq!(t, vec![0.0, 50.0, 100.0, 60.0, 70.0]);
    }

    #[test]
    fn steps_and_rounding() {
        assert_eq!(nice_step(0.0123), 0.01);
        assert_eq!(nice_step(0.03), 0.02);
        assert_eq!(nice_step(0.7), 0.5);
        assert_eq!(nice_step(1.0), 1.0);
        assert_eq!(decimals_for(0.01), 2);
        assert_eq!(decimals_for(0.0123), 2);
        assert_eq!(decimals_for(0.5), 1);
        assert_eq!(decimals_for(3.0), 0);
        assert_eq!(round_dec(1.23456, 2), 1.23);
    }

    fn plan() -> MovePlan {
        // A 20x10 pt child at offset (10, 10) pt in a 100x100 pt stack, pt unit.
        MovePlan {
            id: "n".into(),
            frame: OffsetFrame::new(
                PRect::new(0.0, 0.0, 100.0, 100.0),
                Edges::default(),
                None,
                1.0,
            ),
            start_rect: PRect::new(10.0, 10.0, 20.0, 10.0),
            start_point: [10.0, 10.0],
            axes: [true, true],
            start: [Some(10.0), Some(10.0)],
            sites: [None, None],
            mirrors: [Vec::new(), Vec::new()],
            siblings: vec![PRect::new(63.0, 60.0, 10.0, 10.0)],
            note: None,
        }
    }

    #[test]
    fn drag_moves_snaps_and_locks() {
        let p = plan();
        // 1 screen px = 0.5 pt here; free drag rounds to 1 decimal.
        let (v, g) = drag_values(&p, [20.13, 0.0], false, 0.5);
        assert_eq!(v, [Some(30.1), Some(10.0)]);
        assert!(g.is_empty());
        // Right edge (10+20+32=62) is 1 pt from the sibling's left (63): the
        // closest snap (the center, 52, is 2 pt from the parent's 50).
        let (v, g) = drag_values(&p, [32.0, 0.0], true, 0.5);
        assert_eq!(v[0], Some(43.0));
        assert_eq!(g.len(), 1);
        assert_eq!((g[0].axis, g[0].at), (0, 63.0));
        assert_eq!((g[0].from, g[0].to), (0.0, 100.0));
        // The same drag with snapping off (Alt).
        assert_eq!(drag_values(&p, [32.0, 0.0], false, 0.5).0[0], Some(42.0));
        // A locked axis keeps its value.
        let mut locked = plan();
        locked.axes = [false, true];
        locked.start = [None, Some(10.0)];
        let (v, _) = drag_values(&locked, [33.0, 7.0], false, 0.5);
        assert_eq!(v, [None, Some(17.0)]);
    }

    #[test]
    fn nudges_by_pixel_steps() {
        let p = plan();
        // 1 px = 0.5 pt -> step 0.5; Shift = 5.
        assert_eq!(nudge_values(&p, [1, -2], 1.0, 0.5), [Some(10.5), Some(9.0)]);
        assert_eq!(
            nudge_values(&p, [0, 1], 10.0, 0.5),
            [Some(10.0), Some(15.0)]
        );
        // Unset offsets start from the content origin; reversed viewbox axes
        // count down when moving down.
        let mut q = plan();
        q.frame.viewbox = Some(ViewBox::from_array([0.0, 100.0, 100.0, 0.0]).unwrap());
        q.start = [None, Some(50.0)];
        assert_eq!(nudge_values(&q, [1, 1], 1.0, 0.5), [Some(0.5), Some(49.5)]);
    }
}
