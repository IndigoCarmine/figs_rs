//! Shadows and glows, as rasters shared by every backend.
//!
//! A [`Shadow`] of a node is computed from what the node **and its subtree**
//! actually paint: the group is rendered by the tiny-skia backend onto a
//! transparent canvas (its silhouette — the alpha of everything it draws,
//! text glyphs, image alpha, strokes and arrowheads included), then
//!
//! 1. grown (or shrunk) by `spread` — a Euclidean distance transform, so
//!    corners stay round;
//! 2. blurred with a Gaussian of σ = `blur / 2` — three box blurs (the W3C
//!    `feGaussianBlur` recipe) from 2 px up, an exact kernel below;
//! 3. tinted with the shadow colour (its alpha is the maximum opacity);
//! 4. offset by (`dx`, `dy`) on the page.
//!
//! The PNG backend does this at its output resolution and blends the result
//! under the node; PDF and SVG embed the same raster as an image (PDF has no
//! blur, and SVG filters differ between viewers), at [`vector_scale`].
//! Because the blur removes all fine detail, the image looks the same at any
//! zoom once it has a few pixels per σ.

use tiny_skia::{Pixmap, PremultipliedColorU8};

use crate::geom::{Affine, ArrowHead, Point, Rect, Shadow};
use crate::layout::{ClipRegion, ComputedNode, PaintContent};

use super::png::TinySkiaRenderer;
use super::RenderOptions;

/// Largest raster (pixels) a single shadow may use; bigger ones are computed
/// at a lower resolution and scaled up (a blur has no fine detail to lose).
const MAX_PIXELS: f32 = 24_000_000.0;

/// A tinted shadow image, premultiplied RGBA, placed on the page: its pixel
/// (0, 0) is page pixel (`x0`, `y0`) at `scale` pixels per point.
pub struct ShadowRaster {
    pub pixmap: Pixmap,
    pub x0: i32,
    pub y0: i32,
    /// Pixels per point of this raster.
    pub scale: f32,
}

impl ShadowRaster {
    /// Where the raster lands on the page, points.
    pub fn rect(&self) -> Rect {
        let s = self.scale;
        Rect::new(
            self.x0 as f32 / s,
            self.y0 as f32 / s,
            self.pixmap.width() as f32 / s,
            self.pixmap.height() as f32 / s,
        )
    }
}

/// Resolution (pixels per point) of a shadow embedded in a vector format:
/// about 4 pixels per σ of blur, between 150 dpi and `min(dpi, 300)`; a hard
/// (unblurred) shadow uses the upper bound.
pub fn vector_scale(shadow: &Shadow, dpi: f32) -> f32 {
    let hi = dpi.clamp(72.0, 300.0) / crate::units::PT_PER_INCH;
    let lo = (150.0 / crate::units::PT_PER_INCH).min(hi);
    let sigma = shadow.sigma();
    if sigma <= 0.0 {
        return hi;
    }
    (4.0 / sigma).clamp(lo, hi)
}

/// The shadow `shadow` of `group` (a node followed by its whole subtree, as
/// in the IR), rendered at `scale` pixels per point, cropped to the part
/// that can show inside `visible` (page points). `None` when nothing shows.
pub fn shadow_raster(
    group: &[ComputedNode],
    shadow: &Shadow,
    visible: Rect,
    scale: f32,
    opts: &RenderOptions,
) -> Option<ShadowRaster> {
    let ink = ink_bounds(group)?;
    let reach = shadow.reach();
    // The offset silhouette, and everything its blur can reach, seen.
    let cast = Rect::new(ink.x + shadow.dx, ink.y + shadow.dy, ink.w, ink.h);
    let out = intersect(grow(cast, reach), visible)?;
    // Blurring/spreading the edge pixels needs the silhouette `reach` beyond.
    let work = intersect(grow(out, reach), grow(cast, reach))?;
    let mut scale = scale;
    let area = work.w * work.h * scale * scale;
    if area > MAX_PIXELS {
        scale *= (MAX_PIXELS / area).sqrt();
    }
    let x0 = (work.x * scale).floor() as i32;
    let y0 = (work.y * scale).floor() as i32;
    let x1 = ((work.x + work.w) * scale).ceil() as i32;
    let y1 = ((work.y + work.h) * scale).ceil() as i32;
    let (w, h) = ((x1 - x0).max(1) as u32, (y1 - y0).max(1) as u32);

    // Page points -> raster points: offset by the shadow, minus the origin.
    let shift = Affine {
        e: shadow.dx - x0 as f32 / scale,
        f: shadow.dy - y0 as f32 / scale,
        ..Affine::IDENTITY
    };
    let silhouette = render_group(group, shift, w, h, scale, opts)?;
    let mut alpha: Vec<f32> = silhouette
        .pixels()
        .iter()
        .map(|p| p.alpha() as f32 / 255.0)
        .collect();
    spread(&mut alpha, w as usize, h as usize, shadow.spread * scale);
    gaussian_blur(&mut alpha, w as usize, h as usize, shadow.sigma() * scale);

    let c = shadow.color;
    let mut pixmap = Pixmap::new(w, h)?;
    for (px, a) in pixmap.pixels_mut().iter_mut().zip(&alpha) {
        let a = (a * c.a).clamp(0.0, 1.0);
        let u = |v: f32| (v.clamp(0.0, 1.0) * a * 255.0).round() as u8;
        let a8 = (a * 255.0).round() as u8;
        *px = PremultipliedColorU8::from_rgba(u(c.r).min(a8), u(c.g).min(a8), u(c.b).min(a8), a8)
            .unwrap_or(PremultipliedColorU8::TRANSPARENT);
    }
    Some(ShadowRaster {
        pixmap,
        x0,
        y0,
        scale,
    })
}

/// Render `group` onto a transparent `w` × `h` canvas at `scale` pixels per
/// point, every node moved by `shift` (page points -> canvas points). The
/// group root's own effects are dropped (its descendants keep theirs), and
/// so are the clips the root itself sits in (the caller's canvas applies
/// them); clips inside the group are kept.
pub(crate) fn render_group(
    group: &[ComputedNode],
    shift: Affine,
    w: u32,
    h: u32,
    scale: f32,
    opts: &RenderOptions,
) -> Option<Pixmap> {
    let base = group.first()?.clips.len();
    let moved = |t: &Option<Affine>| Some(t.unwrap_or(Affine::IDENTITY).then(shift));
    let nodes: Vec<ComputedNode> = group
        .iter()
        .enumerate()
        .map(|(i, n)| ComputedNode {
            transform: moved(&n.transform),
            clips: n.clips[base.min(n.clips.len())..]
                .iter()
                .map(|c| ClipRegion {
                    transform: moved(&c.transform),
                    ..c.clone()
                })
                .collect(),
            effects: if i == 0 { None } else { n.effects.clone() },
            ..n.clone()
        })
        .collect();
    let mut r = TinySkiaRenderer::offscreen(w, h, scale, opts)?;
    super::paint_nodes(&mut r, &nodes, &[]);
    Some(r.into_pixmap())
}

/// A generous page-space bounding box of everything `group` may paint:
/// boxes, strokes (with miters), arrowheads, text overhang. `None` when the
/// group paints nothing.
pub fn ink_bounds(group: &[ComputedNode]) -> Option<Rect> {
    let mut acc: Option<Rect> = None;
    for n in group {
        let local = match &n.content {
            PaintContent::Container => continue,
            PaintContent::Image { .. } => n.rect,
            PaintContent::Frame(f) => grow(
                n.rect,
                f.stroke
                    .as_ref()
                    .map_or(0.0, |s| f.stroke_align.outer_extent(s.width)),
            ),
            PaintContent::Rect {
                stroke_width,
                stroke_align,
                ..
            } => grow(n.rect, stroke_align.outer_extent(*stroke_width)),
            PaintContent::Ellipse {
                stroke,
                stroke_align,
                ..
            } => grow(
                n.rect,
                stroke
                    .as_ref()
                    .map_or(0.0, |s| stroke_align.outer_extent(s.width)),
            ),
            PaintContent::Text(t) => grow(n.rect, t.font_size + t.max_stroke_width()),
            PaintContent::Path(p) => {
                let Some((lo, hi)) = crate::path::bounds(&p.segments) else {
                    continue;
                };
                // Twice the width covers miters; an outside stroke's offset
                // line is mitered too.
                let w = p.stroke.as_ref().map_or(0.0, |s| s.width)
                    * if p.stroke_align == crate::geom::StrokeAlign::Outside {
                        3.0
                    } else {
                        1.0
                    };
                let heads = if p.head == ArrowHead::None {
                    0.0
                } else {
                    p.head_size + p.head_width / 2.0
                };
                grow(
                    Rect::new(lo.x, lo.y, hi.x - lo.x, hi.y - lo.y),
                    2.0 * w + heads,
                )
            }
        };
        let page = transformed_bounds(local, n.transform.as_ref());
        acc = Some(match acc {
            Some(a) => union(a, page),
            None => page,
        });
    }
    acc
}

fn transformed_bounds(r: Rect, t: Option<&Affine>) -> Rect {
    let Some(t) = t else {
        return r;
    };
    let pts = [
        Point::new(r.x, r.y),
        Point::new(r.x + r.w, r.y),
        Point::new(r.x + r.w, r.y + r.h),
        Point::new(r.x, r.y + r.h),
    ]
    .map(|p| t.apply(p));
    let (mut x0, mut y0, mut x1, mut y1) = (pts[0].x, pts[0].y, pts[0].x, pts[0].y);
    for p in &pts[1..] {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// `r` grown by `d` on every side.
pub(crate) fn grow(r: Rect, d: f32) -> Rect {
    Rect::new(r.x - d, r.y - d, r.w + 2.0 * d, r.h + 2.0 * d)
}

fn union(a: Rect, b: Rect) -> Rect {
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// The overlap of two rects, `None` when empty.
pub(crate) fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let (x0, y0) = (a.x.max(b.x), a.y.max(b.y));
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
}

// ---------------------------------------------------------------------------
// Spread: dilation / erosion by a disc (Euclidean distance transform)
// ---------------------------------------------------------------------------

/// Grow (`r > 0`) or shrink (`r < 0`) the coverage `a` (`w` × `h`, row-major,
/// `0..=1`) by `|r|` pixels with round corners. Pixels at least half covered
/// are "inside"; the new edge is anti-aliased over one pixel.
pub fn spread(a: &mut [f32], w: usize, h: usize, r: f32) {
    if r.abs() < 0.01 || w == 0 || h == 0 {
        return;
    }
    let grow = r > 0.0;
    let r = r.abs();
    // Distance from each pixel to the nearest seed: inside pixels when
    // growing, outside ones when shrinking.
    let seeds: Vec<bool> = a.iter().map(|v| (*v >= 0.5) == grow).collect();
    let d = distance_transform(&seeds, w, h);
    for (v, d2) in a.iter_mut().zip(d) {
        let d = d2.sqrt();
        if grow {
            *v = v.max((r + 1.0 - d).clamp(0.0, 1.0));
        } else {
            *v = v.min((d - r).clamp(0.0, 1.0));
        }
    }
}

/// Squared Euclidean distance (pixels) from every pixel to the nearest
/// `true` one (Felzenszwalb & Huttenlocher, separable and exact).
fn distance_transform(seeds: &[bool], w: usize, h: usize) -> Vec<f32> {
    let mut g: Vec<f64> = seeds.iter().map(|&s| if s { 0.0 } else { FAR }).collect();
    let n = w.max(h);
    let (mut f, mut d) = (vec![0.0f64; n], vec![0.0f64; n]);
    let (mut v, mut z) = (vec![0usize; n], vec![0.0f64; n + 1]);
    // Columns.
    for x in 0..w {
        for y in 0..h {
            f[y] = g[y * w + x];
        }
        edt_1d(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            g[y * w + x] = d[y];
        }
    }
    // Rows.
    for y in 0..h {
        f[..w].copy_from_slice(&g[y * w..(y + 1) * w]);
        edt_1d(&f[..w], &mut d[..w], &mut v, &mut z);
        g[y * w..(y + 1) * w].copy_from_slice(&d[..w]);
    }
    g.into_iter().map(|v| v as f32).collect()
}

/// "No seed" distance: far beyond any raster, still finite.
const FAR: f64 = 1e18;

/// 1-D squared distance transform of the sampled function `f`: the lower
/// envelope of the parabolas `(q - p)² + f(p)`.
fn edt_1d(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let qf = q as f64;
        let mut s;
        loop {
            let p = v[k] as f64;
            s = ((f[q] + qf * qf) - (f[v[k]] + p * p)) / (2.0 * (qf - p));
            if s <= z[k] && k > 0 {
                k -= 1;
            } else {
                break;
            }
        }
        if s <= z[k] {
            // k == 0: the new parabola is lower everywhere.
            v[0] = q;
            z[1] = f64::INFINITY;
            continue;
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        let qf = q as f64;
        while z[k + 1] < qf {
            k += 1;
        }
        let p = v[k] as f64;
        *out = (qf - p) * (qf - p) + f[v[k]];
    }
}

// ---------------------------------------------------------------------------
// Gaussian blur
// ---------------------------------------------------------------------------

/// Blur `a` (`w` × `h`, row-major) with a Gaussian of standard deviation
/// `sigma` pixels; outside the buffer counts as transparent. From σ = 2 px up
/// it is three box blurs per axis, sized as SVG's `feGaussianBlur` specifies
/// (within a few percent of the true Gaussian); below that an exact kernel.
pub fn gaussian_blur(a: &mut [f32], w: usize, h: usize, sigma: f32) {
    if sigma <= 0.0 || w == 0 || h == 0 {
        return;
    }
    let mut line = vec![0.0f32; w.max(h)];
    let mut tmp = vec![0.0f32; w.max(h)];
    let passes = blur_passes(sigma);
    // Rows.
    for y in 0..h {
        let row = &mut a[y * w..(y + 1) * w];
        line[..w].copy_from_slice(row);
        blur_line(&mut line[..w], &mut tmp[..w], &passes);
        row.copy_from_slice(&line[..w]);
    }
    // Columns.
    for x in 0..w {
        for y in 0..h {
            line[y] = a[y * w + x];
        }
        blur_line(&mut line[..h], &mut tmp[..h], &passes);
        for y in 0..h {
            a[y * w + x] = line[y];
        }
    }
}

/// How to blur one line.
enum Passes {
    /// Exact normalized kernel, centred.
    Kernel(Vec<f32>),
    /// Box blurs `(left, right)`: each output averages `[i - left, i + right]`.
    Boxes([(usize, usize); 3]),
}

fn blur_passes(sigma: f32) -> Passes {
    if sigma < 2.0 {
        let r = (3.0 * sigma).ceil() as i32;
        let mut k: Vec<f32> = (-r..=r)
            .map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp())
            .collect();
        let sum: f32 = k.iter().sum();
        k.iter_mut().for_each(|v| *v /= sum);
        return Passes::Kernel(k);
    }
    // d = floor(σ · 3·√(2π) / 4 + 0.5) (SVG 1.1, feGaussianBlur).
    let d = (sigma * 3.0 * (2.0 * std::f32::consts::PI).sqrt() / 4.0 + 0.5).floor() as usize;
    let d = d.max(1);
    if d % 2 == 1 {
        let r = (d - 1) / 2;
        Passes::Boxes([(r, r); 3])
    } else {
        let h = d / 2;
        Passes::Boxes([(h, h - 1), (h - 1, h), (h, h)])
    }
}

fn blur_line(line: &mut [f32], tmp: &mut [f32], passes: &Passes) {
    let n = line.len();
    match passes {
        Passes::Kernel(k) => {
            let r = (k.len() / 2) as isize;
            for (i, out) in tmp.iter_mut().enumerate() {
                let mut acc = 0.0;
                for (j, kv) in k.iter().enumerate() {
                    let src = i as isize + j as isize - r;
                    if src >= 0 && (src as usize) < n {
                        acc += kv * line[src as usize];
                    }
                }
                *out = acc;
            }
            line.copy_from_slice(tmp);
        }
        Passes::Boxes(boxes) => {
            let mut prefix = vec![0.0f64; n + 1];
            for &(left, right) in boxes {
                let size = (left + right + 1) as f64;
                for i in 0..n {
                    prefix[i + 1] = prefix[i] + line[i] as f64;
                }
                for (i, out) in tmp.iter_mut().enumerate() {
                    let a = i.saturating_sub(left);
                    let b = (i + right).min(n - 1);
                    *out = ((prefix[b + 1] - prefix[a]) / size) as f32;
                }
                line.copy_from_slice(tmp);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_preserves_mass_and_spreads() {
        for sigma in [0.7f32, 3.0, 8.0] {
            let n = 101;
            let mut a = vec![0.0f32; n * n];
            a[50 * n + 50] = 1.0;
            gaussian_blur(&mut a, n, n, sigma);
            let mass: f32 = a.iter().sum();
            assert!((mass - 1.0).abs() < 1e-3, "sigma {sigma}: mass {mass}");
            // The spread along x matches sigma (within the box approximation).
            let var: f32 = (0..n * n)
                .map(|i| a[i] * ((i % n) as f32 - 50.0).powi(2))
                .sum();
            assert!(
                (var.sqrt() - sigma).abs() / sigma < 0.15,
                "sigma {sigma}: got {}",
                var.sqrt()
            );
        }
    }

    #[test]
    fn spread_grows_and_shrinks_round() {
        let (w, h) = (41, 41);
        let mut a = vec![0.0f32; w * h];
        a[20 * w + 20] = 1.0;
        spread(&mut a, w, h, 5.0);
        // A disc of radius ~5 around the centre.
        assert_eq!(a[20 * w + 25], 1.0);
        assert_eq!(a[20 * w + 27], 0.0);
        // Diagonal: (3.5, 3.5) is ~4.95 away — inside; (4, 4) ~5.66 — mostly out.
        assert!(a[24 * w + 24] < 0.5);
        assert!(a[23 * w + 23] > 0.5);
        // Shrink a 21-wide square by 3: 15 wide.
        let mut b = vec![0.0f32; w * h];
        for y in 10..31 {
            for x in 10..31 {
                b[y * w + x] = 1.0;
            }
        }
        spread(&mut b, w, h, -3.0);
        assert_eq!(b[20 * w + 13], 1.0);
        assert!(b[20 * w + 11] < 0.01);
    }

    #[test]
    fn vector_scale_follows_blur() {
        let s = |blur: f32| Shadow {
            dx: 0.0,
            dy: 0.0,
            blur,
            spread: 0.0,
            color: crate::geom::Color::BLACK,
        };
        assert_eq!(vector_scale(&s(0.0), 300.0), 300.0 / 72.0);
        assert_eq!(vector_scale(&s(100.0), 300.0), 150.0 / 72.0);
        let mid = vector_scale(&s(4.0), 300.0); // sigma 2 pt -> 2 px/pt
        assert!((mid - 150.0 / 72.0).abs() < 1e-4 || mid > 150.0 / 72.0);
    }
}
