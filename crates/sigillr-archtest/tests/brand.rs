//! FEAT-003 R-1: the brand assets exist with the sizes the design lists.

use std::fs;
use std::path::PathBuf;

fn brand_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/brand")
}

const SVGS: &[(&str, u32, u32)] = &[
    ("sigillr-mark.svg", 160, 160),
    ("sigillr-lockup-light.svg", 720, 200),
    ("sigillr-lockup-dark.svg", 720, 200),
    ("sigillr-favicon.svg", 64, 64),
    ("sigillr-app-icon.svg", 512, 512),
    ("sigillr-social-preview.svg", 1280, 640),
];

const PNGS: &[(&str, u32, u32)] = &[
    ("apple-touch-icon.png", 180, 180),
    ("sigillr-social-preview.png", 1280, 640),
];

/// Returns the text of the root `<svg ...>` start tag, skipping the XML prolog and comments.
fn root_tag(text: &str) -> Option<&str> {
    let mut rest = text;
    loop {
        rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = &r[r.find("-->")? + 3..];
        } else if rest.starts_with("<?") {
            rest = &rest[rest.find("?>")? + 2..];
        } else {
            break;
        }
    }
    if !rest.starts_with("<svg") {
        return None;
    }
    Some(&rest[..rest.find('>')?])
}

#[test]
fn brand_assets_present() {
    let dir = brand_dir();
    for (name, w, h) in SVGS {
        let text = fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let tag = root_tag(&text).unwrap_or_else(|| panic!("{name}: root is not <svg"));
        let want = format!("viewBox=\"0 0 {w} {h}\"");
        assert!(tag.contains(&want), "{name}: expected {want} in {tag}");
        assert!(text.trim_end().ends_with("</svg>"), "{name}: not closed");
        assert!(!text.contains("<image"), "{name}: embedded raster");
    }
    for (name, w, h) in PNGS {
        let b = fs::read(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(b.len() > 33, "{name}: too short");
        assert_eq!(&b[..8], b"\x89PNG\r\n\x1a\n", "{name}: not a PNG");
        assert_eq!(&b[12..16], b"IHDR", "{name}: no IHDR");
        let bw = u32::from_be_bytes(b[16..20].try_into().unwrap());
        let bh = u32::from_be_bytes(b[20..24].try_into().unwrap());
        assert_eq!((bw, bh), (*w, *h), "{name}: size");
    }
}
