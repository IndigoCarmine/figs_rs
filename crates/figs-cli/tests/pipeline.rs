//! Tests for the shared render pipeline used by `build` and `watch`.

use std::path::PathBuf;

use figs_cli::pipeline;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("figs_pipeline_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

const DOC: &str = r##"
[page]
width = 120
height = 60
unit = "pt"
background = "#ffffff"
root = "root"
[nodes.root]
type = "column"
padding = 4
cross_axis_alignment = "stretch"
children = ["bar", "label"]
[nodes.bar]
type = "rect"
height = 16
fill = "#3366cc"
[nodes.label]
type = "text"
content = "Pipeline"
font_size = 14
"##;

#[test]
fn renders_png() {
    let input = scratch("in_png.toml");
    std::fs::write(&input, DOC).unwrap();
    let output = scratch("out.png");
    let assets = pipeline::assets_for(&input);

    let n = pipeline::render_once(&input, &output, &assets).unwrap();
    assert!(n > 1000, "tiny PNG: {n} bytes");
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(&bytes[1..4], b"PNG", "not a PNG");
}

#[test]
fn renders_pdf() {
    let input = scratch("in_pdf.toml");
    std::fs::write(&input, DOC).unwrap();
    let output = scratch("out.pdf");
    let assets = pipeline::assets_for(&input);

    pipeline::render_once(&input, &output, &assets).unwrap();
    let bytes = std::fs::read(&output).unwrap();
    assert!(bytes.starts_with(b"%PDF-"), "not a PDF");
}

#[test]
fn unsupported_extension_errors() {
    let input = scratch("in_err.toml");
    std::fs::write(&input, DOC).unwrap();
    let output = scratch("out.gif");
    let assets = pipeline::assets_for(&input);

    let err = pipeline::render_once(&input, &output, &assets).unwrap_err();
    assert!(
        err.to_string().contains("unsupported output extension"),
        "{err}"
    );
}

#[test]
fn parse_error_is_reported() {
    let input = scratch("in_bad.toml");
    std::fs::write(&input, "this is not valid = = toml").unwrap();
    let output = scratch("out2.png");
    let assets = pipeline::assets_for(&input);

    assert!(pipeline::render_once(&input, &output, &assets).is_err());
}
