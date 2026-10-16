//! Insets (`padding` / `margin`): scalars, per-side tables / maps with the
//! `x` / `y` shorthands, and CSS-order arrays — the same rules in TOML and in
//! Rhai's `.pad(..)` / `.margin(..)`.

use figs_core::eval_script;
use figs_core::geom::Edges;
use figs_core::schema::parse::parse_str;

fn e(top: f32, right: f32, bottom: f32, left: f32) -> Edges {
    Edges {
        top,
        right,
        bottom,
        left,
    }
}

fn toml_padding(v: &str) -> Result<Edges, String> {
    let src = format!(
        "[page]\nwidth = 1\nheight = 1\nroot = \"r\"\n[nodes.r]\ntype = \"column\"\npadding = {v}"
    );
    parse_str(&src)
        .map(|d| d.nodes["r"].padding)
        .map_err(|e| format!("{e:?}"))
}

fn script_pad(v: &str) -> Result<(Edges, Edges), String> {
    let raw = eval_script(&format!("page(10, 10, col([]).pad({v}).margin({v}))"))
        .map_err(|e| e.message)?;
    let n = &raw.nodes["root"];
    Ok((n.padding, n.margin))
}

#[test]
fn every_form_means_the_same_in_toml_and_script() {
    let cases: [(&str, &str, Edges); 8] = [
        ("2", "2", Edges::all(2.0)),
        ("[1, 2]", "[1, 2]", e(1.0, 2.0, 1.0, 2.0)),
        ("[1, 2, 3]", "[1, 2, 3]", e(1.0, 2.0, 3.0, 2.0)),
        ("[1, 2, 3, 4]", "[1, 2, 3, 4]", e(1.0, 2.0, 3.0, 4.0)),
        ("[5]", "[5]", Edges::all(5.0)),
        ("{ x = 1 }", "#{ x: 1 }", e(0.0, 1.0, 0.0, 1.0)),
        (
            "{ y = 2, left = 3 }",
            "#{ y: 2, left: 3 }",
            e(2.0, 0.0, 2.0, 3.0),
        ),
        // A named side wins over its shorthand.
        (
            "{ x = 1, right = 4, top = 0.5 }",
            "#{ x: 1, right: 4, top: 0.5 }",
            e(0.5, 4.0, 0.0, 1.0),
        ),
    ];
    for (t, s, want) in cases {
        assert_eq!(toml_padding(t).unwrap(), want, "TOML {t}");
        assert_eq!(script_pad(s).unwrap(), (want, want), "script {s}");
    }
}

#[test]
fn bad_insets_are_rejected_with_the_reason() {
    for t in ["[1, 2, 3, 4, 5]", "[]", "{ middle = 1 }", "\"wide\""] {
        assert!(toml_padding(t).is_err(), "TOML {t}");
    }
    let e = toml_padding("{ middle = 1 }").unwrap_err();
    assert!(e.contains("middle"), "{e}");
    for (s, why) in [
        ("[1, 2, 3, 4, 5]", "[t, r, b, l]"),
        ("#{ middle: 1 }", "unknown side `middle`"),
        ("#{ top: \"a\" }", "`top` must be a number"),
        ("\"wide\"", "expects a number"),
    ] {
        let e = script_pad(s).unwrap_err();
        assert!(e.contains(why), "{s}: {e}");
    }
}

#[test]
fn insets_serialize_back_as_tables() {
    let raw = eval_script("page(10, 10, col([]).pad([1, 2]).margin(#{ y: 3 }))").unwrap();
    let toml = toml::to_string(&raw).unwrap();
    let back = parse_str(&toml).unwrap();
    assert_eq!(back.nodes["root"].padding, e(1.0, 2.0, 1.0, 2.0));
    assert_eq!(back.nodes["root"].margin, e(3.0, 0.0, 3.0, 0.0));
}
