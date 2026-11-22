//! The pinned toolchain (FEAT-001 R-1, FEAT-006 R-2).

use sigillr_archtest::{read, repo_root};

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
