//! The user guide (FEAT-005): every page and link resolves, and mdBook builds it.

use sigillr_archtest::{read, repo_root};
use std::path::{Path, PathBuf};
use std::process::Command;

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read guide dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

/// Targets of `](target)` links and images in `text`.
fn link_targets(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("](") {
        rest = &rest[i + 2..];
        if let Some(j) = rest.find(')') {
            out.push(rest[..j].trim().to_string());
            rest = &rest[j..];
        }
    }
    out
}

fn is_external(t: &str) -> bool {
    t.contains("://") || t.starts_with("mailto:") || t.starts_with('#') || t.is_empty()
}

#[test]
fn mdbook_builds_without_broken_links() {
    let guide = repo_root().join("docs").join("guide");

    // Every page in SUMMARY.md exists.
    let summary = read("docs/guide/SUMMARY.md");
    for t in link_targets(&summary) {
        if !is_external(&t) {
            assert!(
                guide.join(&t).is_file(),
                "SUMMARY.md lists {t}, which does not exist"
            );
        }
    }

    // Every relative link and image in every page resolves.
    let mut pages = Vec::new();
    markdown_files(&guide, &mut pages);
    assert!(!pages.is_empty(), "no pages under docs/guide");
    for page in &pages {
        let text = std::fs::read_to_string(page).expect("read page");
        let base = page.parent().expect("page has a parent");
        for t in link_targets(&text) {
            if is_external(&t) {
                continue;
            }
            let file = t.split('#').next().unwrap_or("");
            if file.is_empty() {
                continue;
            }
            assert!(
                base.join(file).exists(),
                "{} links to {t}, which does not resolve",
                page.display()
            );
        }
    }

    // The book builds, when mdbook is available (required in CI).
    let dest = std::env::temp_dir().join(format!("sigillr-guide-{}", std::process::id()));
    match Command::new("mdbook")
        .arg("build")
        .arg(&guide)
        .arg("--dest-dir")
        .arg(&dest)
        .output()
    {
        Ok(out) => {
            let _ = std::fs::remove_dir_all(&dest);
            assert!(
                out.status.success(),
                "mdbook build failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Err(e) => assert!(
            std::env::var_os("SIGILLR_REQUIRE_MDBOOK").is_none(),
            "mdbook not found but SIGILLR_REQUIRE_MDBOOK is set: {e}"
        ),
    }
}
