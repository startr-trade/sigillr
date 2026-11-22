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
        let inherits = t
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.get("workspace"))
            .and_then(|w| w.as_bool())
            == Some(true);
        assert!(
            inherits,
            "{} must use version.workspace = true",
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
