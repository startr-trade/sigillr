//! The pinned toolchain (FEAT-001 R-1, R-2; FEAT-006 R-2).

use sigillr_archtest::{read, repo_root};

fn is_exact_version(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// R-1: the toolchain is pinned exactly, rustc matches it, and every crate dir is a workspace member.
#[test]
fn workspace_builds_and_tests_on_pinned_toolchain() {
    let toml: toml::Table = read("rust-toolchain.toml").parse().expect("valid TOML");
    let channel = toml["toolchain"]["channel"]
        .as_str()
        .expect("toolchain.channel is a string");
    assert!(
        is_exact_version(channel),
        "channel must be an exact x.y.z, got {channel}"
    );

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let out = std::process::Command::new(rustc)
        .arg("--version")
        .output()
        .expect("run rustc --version");
    let version = String::from_utf8(out.stdout).expect("utf-8 version");
    assert!(
        version.starts_with(&format!("rustc {channel}")),
        "rustc is `{}`, expected rustc {channel}",
        version.trim()
    );

    let crates = repo_root().join("crates");
    for entry in std::fs::read_dir(&crates).expect("read crates/") {
        let dir = entry.expect("dir entry").path();
        if !dir.is_dir() {
            continue;
        }
        let manifest = std::fs::read_to_string(dir.join("Cargo.toml"))
            .unwrap_or_else(|e| panic!("{} has no Cargo.toml: {e}", dir.display()));
        let t: toml::Table = manifest.parse().expect("valid crate manifest");
        // Inheriting from the workspace is the membership proof. A crate with its own version
        // line (sigillr-client, FTR-004 Decisions) proves it through rust-version instead.
        let inherits = |key: &str| {
            t.get("package")
                .and_then(|p| p.get(key))
                .and_then(|v| v.get("workspace"))
                .and_then(|w| w.as_bool())
                == Some(true)
        };
        assert!(
            inherits("version") || inherits("rust-version"),
            "{} must use version.workspace = true (or rust-version.workspace = true with its own version line)",
            dir.display()
        );
    }
}

/// R-2: line endings are forced to LF.
#[test]
fn gitattributes_forces_lf() {
    assert!(
        read(".gitattributes")
            .lines()
            .any(|l| l == "* text=auto eol=lf"),
        ".gitattributes must contain `* text=auto eol=lf`"
    );
}

fn pinned_channel() -> String {
    read("rust-toolchain.toml")
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == "channel").then(|| v.trim().trim_matches('"').to_string())
        })
        .expect("rust-toolchain.toml has a channel")
}

/// FEAT-006 R-2. Every toolchain a workflow names is the pinned channel; none says stable/nightly.
#[test]
fn archtest_toolchain_pinned_everywhere() {
    let channel = pinned_channel();
    let dir = repo_root().join(".github/workflows");
    for e in std::fs::read_dir(dir).expect("workflows dir") {
        let p = e.expect("entry").path();
        if p.extension().is_none_or(|x| x != "yml") {
            continue;
        }
        let name = p.file_name().expect("name").to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&p).expect("workflow");
        for l in text.lines() {
            let t = l.trim().trim_start_matches("- ");
            if let Some(v) = t.strip_prefix("toolchain:") {
                let v = v.trim().trim_matches('"');
                assert_eq!(v, channel, "{name}: toolchain `{v}` is not the pinned one");
            }
            if let Some((_, rest)) = t.split_once("rust-toolchain@") {
                let v: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                if v.contains('.') {
                    assert_eq!(
                        v, channel,
                        "{name}: rust-toolchain@{v} is not the pinned one"
                    );
                }
            }
            for word in t.split_whitespace() {
                if let Some(v) = word.strip_prefix('+') {
                    if v.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                        assert_eq!(v, channel, "{name}: +{v} is not the pinned one");
                    }
                    assert!(v != "stable" && v != "nightly", "{name}: names +{v}");
                }
            }
        }
    }
}
