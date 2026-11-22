//! The supply-chain gate: deny.toml and supply-chain.yml (FEAT-006, baseline B-023 to B-026).

use sigillr_archtest::{read, repo_root};
use std::process::Command;

fn compact(text: &str) -> String {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// R-5. deny.toml has the four sections with the agreed settings; supply-chain.yml has the
/// triggers and the job.
#[test]
fn deny_config_covers_advisories_licences_and_sources() {
    let deny = compact(&read("deny.toml"));
    for needle in [
        "[advisories]",
        "yanked = \"deny\"",
        "ignore = []",
        "[licenses]",
        "confidence-threshold = 0.9",
        "[bans]",
        "multiple-versions = \"warn\"",
        "[sources]",
        "unknown-registry = \"deny\"",
        "unknown-git = \"deny\"",
    ] {
        assert!(deny.contains(needle), "deny.toml lacks `{needle}`");
    }
    for l in [
        "MIT",
        "Apache-2.0",
        "Apache-2.0 WITH LLVM-exception",
        "BSD-2-Clause",
        "BSD-3-Clause",
        "ISC",
        "Unicode-3.0",
        "Zlib",
    ] {
        assert!(
            deny.contains(&format!("\"{l}\"")),
            "deny.toml allow list lacks {l}"
        );
    }

    let wf = compact(&read(".github/workflows/supply-chain.yml"));
    for needle in [
        "push:",
        "pull_request:",
        "schedule:",
        "- cron: \"41 3 * * *\"",
        "workflow_dispatch:",
        "tags: [\"v*\"]",
        "- deny.toml",
        "- Cargo.lock",
        "supply-chain:",
        "EmbarkStudios/cargo-deny-action@",
    ] {
        assert!(wf.contains(needle), "supply-chain.yml lacks `{needle}`");
    }
}

/// R-5. `cargo deny --locked check advisories licenses bans sources` exits 0. It fetches the
/// advisory database, so it fails offline.
#[test]
fn cargo_deny_check_passes() {
    let out = Command::new("cargo")
        .args([
            "deny",
            "--locked",
            "check",
            "advisories",
            "licenses",
            "bans",
            "sources",
        ])
        .current_dir(repo_root())
        .output()
        .expect("cannot run `cargo deny`: install cargo-deny (cargo install cargo-deny --locked)");
    assert!(
        out.status.success(),
        "cargo deny failed (offline? the advisory database is fetched):\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
