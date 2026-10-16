//! Rich text in scripts.
//!
//! **Core** (explicit, 1:1 with the TOML `spans` model):
//!
//! ```text
//! text([span("ΔG"), span("300K").sub(), span(" = "), span("−27.5").color("#e0635a").bold()])
//! ```
//!
//! `text(array)` takes spans and/or plain strings (a string is an unstyled
//! span; nested arrays are flattened). Span setters: `.bold()`, `.weight(n)`,
//! `.italic()`, `.upright()`, `.color(hex)`, `.size(pt)`, `.sup()`, `.sub()`,
//! `.shift(em)`, `.underline()`, `.strike()`, `.font(name | list | array)`,
//! `.tracking(em)`. The text-node setters `.italic()`, `.underline()`,
//! `.strike()` and `.tracking(em)` set the node style every span inherits.
//!
//! **Framework** sugar: `markup(s)` parses the inline markup of
//! [`super::markup`] into the same span array (`text(markup("H_2O"))`); the
//! prelude's `rich(s)` is `text(markup(s))`.

use rhai::{Array, Dynamic, Engine, EvalAltResult};

use crate::geom::Color;
use crate::schema::span::{FontStyle, RawSpan, Script};
use crate::schema::NodeKind;

use super::{dyn_to_f32, err, leaf, set_node, ScriptNode};

type SpanResult = Result<RawSpan, Box<EvalAltResult>>;

/// Flatten `items` (spans, strings, nested arrays) into spans.
fn collect_spans(items: Array, out: &mut Vec<RawSpan>) -> Result<(), Box<EvalAltResult>> {
    for item in items {
        if item.is_string() {
            out.push(RawSpan::new(item.into_string().expect("checked")));
        } else if item.is::<RawSpan>() {
            out.push(item.cast::<RawSpan>());
        } else if item.is_array() {
            collect_spans(item.cast::<Array>(), out)?;
        } else {
            return Err(err(format!(
                "`text([...])` takes spans and strings, got {}",
                item.type_name()
            )));
        }
    }
    Ok(())
}

/// A family argument: a name, a comma-separated list, or an array of names.
fn family_arg(name: &str, v: Dynamic) -> Result<String, Box<EvalAltResult>> {
    if v.is_string() {
        return Ok(v.into_string().expect("checked"));
    }
    if let Some(a) = v.clone().try_cast::<Array>() {
        let names = a
            .into_iter()
            .map(|x| {
                x.into_string()
                    .map_err(|_| err(format!("`{name}`: family names must be strings")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(names.join(", "));
    }
    Err(err(format!(
        "`{name}` expects a family name, list or array"
    )))
}

fn span_num(name: &'static str, v: &Dynamic) -> Result<f32, Box<EvalAltResult>> {
    dyn_to_f32(v)
        .filter(|x| x.is_finite())
        .ok_or_else(|| err(format!("`{name}` expects a number")))
}

pub(super) fn register(engine: &mut Engine) {
    engine.register_type_with_name::<RawSpan>("Span");

    engine.register_fn("span", |s: &str| RawSpan::new(s));
    engine.register_fn(
        "text",
        |items: Array| -> Result<ScriptNode, Box<EvalAltResult>> {
            let mut spans = Vec::new();
            collect_spans(items, &mut spans)?;
            let mut sn = leaf(NodeKind::Text);
            if spans.is_empty() {
                sn.node.content = Some(String::new());
            }
            sn.node.spans = spans;
            Ok(sn)
        },
    );

    // --- span setters ---
    engine.register_fn("bold", |mut s: RawSpan| {
        s.font_weight = Some(700);
        s
    });
    engine.register_fn("weight", |mut s: RawSpan, v: Dynamic| -> SpanResult {
        let w = span_num("weight", &v)?;
        if !(1.0..=1000.0).contains(&w) {
            return Err(err(format!("`weight` out of range: {w}")));
        }
        s.font_weight = Some(w.round() as u16);
        Ok(s)
    });
    engine.register_fn("italic", |mut s: RawSpan| {
        s.font_style = Some(FontStyle::Italic);
        s
    });
    engine.register_fn("upright", |mut s: RawSpan| {
        s.font_style = Some(FontStyle::Normal);
        s
    });
    engine.register_fn("color", |mut s: RawSpan, hex: &str| -> SpanResult {
        s.color = Some(
            Color::parse_hex(hex).map_err(|_| err(format!("`color`: invalid color `{hex}`")))?,
        );
        Ok(s)
    });
    engine.register_fn("size", |mut s: RawSpan, v: Dynamic| -> SpanResult {
        let x = span_num("size", &v)?;
        if x <= 0.0 {
            return Err(err(format!("`size` must be positive, got {x}")));
        }
        s.font_size = Some(x);
        Ok(s)
    });
    engine.register_fn("sup", |mut s: RawSpan| {
        s.script = Some(Script::Super);
        s
    });
    engine.register_fn("sub", |mut s: RawSpan| {
        s.script = Some(Script::Sub);
        s
    });
    engine.register_fn("shift", |mut s: RawSpan, v: Dynamic| -> SpanResult {
        s.baseline_shift = Some(span_num("shift", &v)?);
        Ok(s)
    });
    engine.register_fn("underline", |mut s: RawSpan| {
        s.underline = Some(true);
        s
    });
    engine.register_fn("strike", |mut s: RawSpan| {
        s.strike = Some(true);
        s
    });
    engine.register_fn("tracking", |mut s: RawSpan, v: Dynamic| -> SpanResult {
        s.letter_spacing = Some(span_num("tracking", &v)?);
        Ok(s)
    });
    engine.register_fn("font", |mut s: RawSpan, v: Dynamic| -> SpanResult {
        s.font_family = Some(family_arg("font", v)?);
        Ok(s)
    });

    // --- text-node style every span inherits ---
    engine.register_fn("italic", |n: ScriptNode| {
        set_node(n, |r| r.font_style = Some(FontStyle::Italic))
    });
    engine.register_fn("underline", |n: ScriptNode| {
        set_node(n, |r| r.underline = true)
    });
    engine.register_fn("strike", |n: ScriptNode| set_node(n, |r| r.strike = true));
    engine.register_fn(
        "tracking",
        |n: ScriptNode, v: Dynamic| -> Result<ScriptNode, Box<EvalAltResult>> {
            let x = span_num("tracking", &v)?;
            Ok(set_node(n, |r| r.letter_spacing = Some(x)))
        },
    );
    // `.font([...])`: an ordered fallback list as an array.
    engine.register_fn(
        "font",
        |n: ScriptNode, list: Array| -> Result<ScriptNode, Box<EvalAltResult>> {
            let f = family_arg("font", Dynamic::from_array(list))?;
            Ok(set_node(n, |r| r.font_family = Some(f)))
        },
    );
    engine.register_fn(
        "font",
        |d: super::ScriptDoc, list: Array| -> Result<super::ScriptDoc, Box<EvalAltResult>> {
            let f = family_arg("font", Dynamic::from_array(list))?;
            Ok(super::set_doc(d, |pg| pg.font_family = Some(f)))
        },
    );

    // --- framework sugar: inline markup -> spans ---
    engine.register_fn("markup", |s: &str| -> Result<Array, Box<EvalAltResult>> {
        let spans = super::markup::parse(s).map_err(|e| err(e.to_string()))?;
        Ok(spans.into_iter().map(Dynamic::from).collect())
    });
}
