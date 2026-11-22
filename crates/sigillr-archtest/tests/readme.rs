//! README checks (FEAT-004, R-1): the light/dark lockup and the links a reader needs next.

use std::path::Path;

use sigillr_archtest::{read, repo_root};

/// Targets of `](target)` markdown links and of `src=`/`srcset=`/`href=` attributes.
fn link_targets(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("](") {
        rest = &rest[i + 2..];
        if let Some(end) = rest.find(')') {
            out.push(rest[..end].to_string());
        }
    }
    for attr in ["src=\"", "srcset=\"", "href=\""] {
        let mut rest = text;
        while let Some(i) = rest.find(attr) {
            rest = &rest[i + attr.len()..];
            if let Some(end) = rest.find('"') {
                out.push(rest[..end].to_string());
            }
        }
    }
    out
}

#[test]
fn readme_has_picture_lockup_and_links() {
    let readme = read("README.md");

    // The lockup: a <picture> whose dark source is the dark lockup and whose fallback <img> is the light one.
    let picture = readme
        .find("<picture>")
        .and_then(|s| readme[s..].find("</picture>").map(|e| &readme[s..s + e]))
        .expect("README has a <picture> element");
    assert!(
        picture.contains("<source")
            && picture.contains("prefers-color-scheme: dark")
            && picture.contains("sigillr-lockup-dark.svg"),
        "the <picture> needs a dark <source> for sigillr-lockup-dark.svg"
    );
    let img = picture
        .find("<img")
        .map(|s| &picture[s..])
        .expect("the <picture> has an <img> fallback");
    assert!(
        img.contains("sigillr-lockup-light.svg"),
        "the <img> fallback is sigillr-lockup-light.svg"
    );

    // The links a reader is pointed to.
    for needle in [
        "CONTRIBUTING.md",
        "SECURITY.md",
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "docs/guide/",
    ] {
        assert!(readme.contains(needle), "README links {needle}");
    }

    // Every relative target resolves to a file or directory in the repository.
    let root = repo_root();
    for target in link_targets(&readme) {
        if target.contains("://") || target.starts_with('#') || target.starts_with("mailto:") {
            continue;
        }
        let path = target.split('#').next().unwrap_or("");
        if path.is_empty() {
            continue;
        }
        assert!(
            Path::new(&root.join(path)).exists(),
            "README link target {path} does not exist"
        );
    }
}

/// DEF-004 R-1. crates.io cannot resolve repository-relative images, so every image in the README
/// is an absolute https URL; a raw.githubusercontent.com URL on main names a file that exists.
#[test]
fn readme_images_are_absolute() {
    let readme = read("README.md");
    let root = repo_root();
    for attr in ["src=\"", "srcset=\""] {
        let mut rest = readme.as_str();
        while let Some(i) = rest.find(attr) {
            rest = &rest[i + attr.len()..];
            let url = &rest[..rest.find('"').unwrap_or(rest.len())];
            assert!(
                url.starts_with("https://"),
                "README image {url} is not an absolute https URL"
            );
            if let Some(path) =
                url.strip_prefix("https://raw.githubusercontent.com/startr-trade/sigillr/main/")
            {
                assert!(
                    root.join(path).exists(),
                    "README image {url} names {path}, which does not exist"
                );
            }
        }
    }
}

/// DEF-004 R-2. The README describes the released crate, with no pre-release note.
#[test]
fn readme_has_no_pre_release_note() {
    let readme = read("README.md").to_lowercase();
    assert!(
        !readme.contains("no public release"),
        "README still says there is no public release"
    );
}
