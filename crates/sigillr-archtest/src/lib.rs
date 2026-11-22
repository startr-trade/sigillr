//! sigillr-archtest: repository checks run as tests.

use std::path::PathBuf;

/// The repository root: two parents up from this crate's manifest directory.
pub fn repo_root() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.parent()
        .and_then(|p| p.parent())
        .expect("crate sits two levels below the repository root")
        .to_path_buf()
}

/// Read a `/`-separated path under the repository root; panics naming the path.
pub fn read(rel: &str) -> String {
    let path = rel.split('/').fold(repo_root(), |acc, part| acc.join(part));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}
