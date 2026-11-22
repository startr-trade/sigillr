//! The install scripts verify the archive's checksum and install nothing on a mismatch (FEAT-007 R-4).

use sigillr_archtest::read;

#[cfg(unix)]
mod unix {
    use sigillr_archtest::repo_root;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    fn host_target() -> &'static str {
        let uname = |flag: &str| {
            let out = Command::new("uname").arg(flag).output().expect("uname");
            String::from_utf8(out.stdout)
                .expect("utf8")
                .trim()
                .to_string()
        };
        match (uname("-s").as_str(), uname("-m").as_str()) {
            ("Linux", "x86_64" | "amd64") => "x86_64-unknown-linux-gnu",
            ("Darwin", "x86_64") => "x86_64-apple-darwin",
            ("Darwin", "arm64") => "aarch64-apple-darwin",
            other => panic!("unsupported host for the install test: {other:?}"),
        }
    }

    pub fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("sigillr-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn run_ok(cmd: &mut Command) {
        let out = cmd.output().expect("spawn");
        assert!(
            out.status.success(),
            "{cmd:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A fixture release in `base`: the archive for the host target and its `.sha256`.
    pub fn fixture(base: &Path, version: &str) -> String {
        let name = format!("sigillr-{version}-{}", host_target());
        let src = base.join("src").join(&name);
        std::fs::create_dir_all(&src).expect("src");
        std::fs::write(src.join("sigillr"), "#!/bin/sh\necho sigillr fixture\n").expect("bin");
        let archive = format!("{name}.tar.gz");
        run_ok(
            Command::new("tar")
                .args(["-czf", &archive, "-C", "src", &name])
                .current_dir(base),
        );
        run_ok(
            Command::new("sh")
                .args([
                    "-c",
                    &format!(
                        "(sha256sum {archive} 2>/dev/null || shasum -a 256 {archive}) > {archive}.sha256"
                    ),
                ])
                .current_dir(base),
        );
        archive
    }

    pub fn install(base: &Path, dir: &Path) -> Output {
        Command::new("sh")
            .arg(repo_root().join("scripts/install.sh"))
            .env("SIGILLR_RELEASE_BASE", format!("file://{}", base.display()))
            .env("SIGILLR_VERSION", "v9.9.9")
            .env("SIGILLR_INSTALL_DIR", dir)
            .output()
            .expect("run install.sh")
    }
}

/// R-4. A matching checksum installs into SIGILLR_INSTALL_DIR; a corrupted archive exits 1 and
/// installs nothing; install.ps1 carries the same refusal.
#[test]
fn install_scripts_verify_checksum() {
    #[cfg(unix)]
    {
        let base = unix::scratch("release");
        let archive = unix::fixture(&base, "9.9.9");

        let good = base.join("bin-good");
        let out = unix::install(&base, &good);
        assert!(
            out.status.success(),
            "install failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(good.join("sigillr").is_file(), "binary not installed");

        // Corrupt the archive: the recorded checksum no longer matches.
        let mut bytes = std::fs::read(base.join(&archive)).expect("archive");
        bytes.extend_from_slice(b"corrupt");
        std::fs::write(base.join(&archive), bytes).expect("write");
        let bad = base.join("bin-bad");
        let out = unix::install(&base, &bad);
        assert_eq!(out.status.code(), Some(1), "a mismatch must exit 1");
        assert!(String::from_utf8_lossy(&out.stderr).contains("checksum mismatch"));
        assert!(
            !bad.join("sigillr").exists(),
            "nothing may be installed on a mismatch"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    let ps1 = read("scripts/install.ps1");
    assert!(
        ps1.contains("Get-FileHash -Algorithm SHA256"),
        "install.ps1 does not hash with SHA256"
    );
    let cmp = ps1
        .find("-ne $expected")
        .expect("install.ps1 has no checksum comparison");
    let tail = &ps1[cmp..];
    let refusal = &tail[..tail.find("Expand-Archive").expect("no extraction")];
    assert!(
        refusal.contains("checksum mismatch") && refusal.contains("exit 1"),
        "install.ps1 does not refuse (exit 1) before extracting"
    );
}
