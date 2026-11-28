//! The release workflow checks CI first and publishes nothing in a dry run (FEAT-007).

use sigillr_archtest::read;
use std::collections::BTreeMap;

struct Job {
    needs: Vec<String>,
    body: String,
}

/// The jobs of release.yml: name -> (`needs`, raw text), read by indentation (no YAML dependency).
fn jobs(text: &str) -> BTreeMap<String, Job> {
    let mut out: BTreeMap<String, Job> = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut in_jobs = false;
    for line in text.lines() {
        if line.starts_with("jobs:") {
            in_jobs = true;
            continue;
        }
        if !in_jobs {
            continue;
        }
        if line.starts_with("  ") && !line.starts_with("   ") && line.trim_end().ends_with(':') {
            let name = line.trim().trim_end_matches(':').to_string();
            out.insert(
                name.clone(),
                Job {
                    needs: vec![],
                    body: String::new(),
                },
            );
            current = Some(name);
            continue;
        }
        if let Some(job) = current.as_ref().and_then(|n| out.get_mut(n)) {
            job.body.push_str(line);
            job.body.push('\n');
            if let Some(v) = line.strip_prefix("    needs:") {
                job.needs = v
                    .trim()
                    .trim_matches(['[', ']'])
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
    }
    out
}

fn reaches(jobs: &BTreeMap<String, Job>, from: &str, target: &str) -> bool {
    let mut stack = vec![from.to_string()];
    let mut seen = vec![];
    while let Some(j) = stack.pop() {
        if seen.contains(&j) {
            continue;
        }
        seen.push(j.clone());
        for n in jobs.get(&j).map(|j| j.needs.clone()).unwrap_or_default() {
            if n == target {
                return true;
            }
            stack.push(n);
        }
    }
    false
}

/// Every job other than `ci-green` waits (directly or through another job) for `ci-green`, and the
/// steps of `publish` that publish are skipped when `dry_run` is set.
#[test]
fn release_workflow_checks_ci_first() {
    let text = read(".github/workflows/release.yml");
    let jobs = jobs(&text);
    assert!(jobs.contains_key("ci-green"), "no ci-green job");
    for name in jobs.keys().filter(|n| *n != "ci-green") {
        assert!(
            reaches(&jobs, name, "ci-green"),
            "job `{name}` does not need ci-green"
        );
    }
    let publish = jobs.get("publish").expect("no publish job");
    let mut publishing_steps = 0;
    for step in publish.body.split("\n      - ").skip(1) {
        let publishes = step.contains("cargo publish -p sigillr --locked\n")
            || step.contains("gh release create");
        if publishes {
            publishing_steps += 1;
            assert!(
                step.contains("if:") && step.contains("inputs.dry_run"),
                "a publishing step is not skipped on dry_run:\n{step}"
            );
        }
    }
    assert_eq!(
        publishing_steps, 2,
        "expected cargo publish and gh release create"
    );
    assert!(
        publish
            .body
            .contains("cargo publish -p sigillr --locked --dry-run"),
        "publish does not run cargo publish --dry-run first"
    );
}

/// R-5. `artifact-scan` runs Trivy over the archives and the SBOM (HIGH,CRITICAL, fixable only,
/// failing the job), runs in a dry run too, and `publish` needs it.
#[test]
fn release_workflow_scans_artifacts_before_publish() {
    let text = read(".github/workflows/release.yml");
    let jobs = jobs(&text);
    let scan = jobs.get("artifact-scan").expect("no artifact-scan job");
    assert!(scan.body.contains("aquasecurity/trivy-action@"));
    assert!(scan.body.contains("scan-type: fs"), "no fs scan");
    assert!(scan.body.contains("scan-type: sbom"), "no sbom scan");
    assert!(scan.body.contains("severity: HIGH,CRITICAL"));
    assert!(scan.body.contains("ignore-unfixed: true"));
    assert!(scan.body.contains("exit-code: \"1\""));
    assert!(
        !scan.body.contains("dry_run") && !scan.body.contains("    if:"),
        "artifact-scan must run in a dry run too"
    );
    assert!(
        jobs.get("publish")
            .expect("no publish job")
            .needs
            .iter()
            .any(|n| n == "artifact-scan"),
        "publish does not need artifact-scan"
    );
}

/// R-6. `publish` attests build provenance for the archives, and only `publish` may mint tokens.
#[test]
fn release_attests_build_provenance() {
    let text = read(".github/workflows/release.yml");
    let jobs = jobs(&text);
    let publish = jobs.get("publish").expect("no publish job");
    assert!(
        publish.body.contains("actions/attest-build-provenance@"),
        "publish has no attest-build-provenance step"
    );
    assert!(
        publish.body.contains("artifacts/*.tar.gz") && publish.body.contains("artifacts/*.zip")
    );
    assert!(
        publish.body.contains("id-token: write") && publish.body.contains("attestations: write")
    );
    for (name, job) in &jobs {
        if name != "publish" {
            assert!(
                !job.body.contains("id-token: write") && !job.body.contains("attestations: write"),
                "job `{name}` must not have id-token or attestations write"
            );
        }
    }
    let top = text.split("\njobs:").next().expect("header");
    assert!(
        !top.contains("id-token: write"),
        "top-level permissions grant id-token"
    );
}

/// DEF-002 R-1. cargo-cyclonedx picks the package by `--manifest-path`; it has no `-p`.
#[test]
fn sbom_step_selects_the_crate_by_manifest_path() {
    let text = read(".github/workflows/release.yml");
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("cargo cyclonedx"))
        .expect("release.yml runs cargo cyclonedx");
    let args: Vec<&str> = line.split_whitespace().collect();
    assert!(
        args.windows(2)
            .any(|w| w == ["--manifest-path", "crates/sigillr/Cargo.toml"]),
        "the sbom step must select the crate with --manifest-path crates/sigillr/Cargo.toml: {line}"
    );
    assert!(
        !args
            .iter()
            .any(|a| *a == "-p" || a.starts_with("--package")),
        "cargo cyclonedx has no -p/--package option: {line}"
    );
}

/// DEF-003 R-1. kanbanr writes the notes to `docs/guide/releases/v<version>.md`: the release step
/// reads that path from the tag name as is, and a step that also runs on dry runs checks the file exists.
#[test]
fn release_notes_path_matches_kanbanr() {
    let text = read(".github/workflows/release.yml");
    assert!(
        text.contains("--notes-file \"docs/guide/releases/${GITHUB_REF_NAME}.md\""),
        "gh release create must read docs/guide/releases/${{GITHUB_REF_NAME}}.md"
    );
    assert!(
        text.contains("test -f \"docs/guide/releases/v${version}.md\""),
        "a dry-run step must check docs/guide/releases/v${{version}}.md exists"
    );
    for entry in
        std::fs::read_dir(sigillr_archtest::repo_root().join("docs/guide/releases")).unwrap()
    {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if name != "README.md" {
            assert!(
                name.starts_with('v') && name.ends_with(".md"),
                "release notes {name} are not named v<version>.md"
            );
        }
    }
}

const CLIENT_TAG: &str = "\"sigillr-client-v[0-9]+.[0-9]+.[0-9]+\"";
const SERVER_TAG: &str = "\"v[0-9]+.[0-9]+.[0-9]+\"";
const ON_CLIENT_TAG: &str = "startsWith(github.ref, 'refs/tags/sigillr-client-v')";

/// The `- "…"` entries under `on.push.tags`.
fn tag_patterns(text: &str) -> Vec<String> {
    let head = text.split("\njobs:").next().expect("header");
    let tags = head.split("    tags:\n").nth(1).expect("on.push.tags");
    tags.lines()
        .map_while(|l| l.strip_prefix("      - "))
        .map(|p| p.trim().to_string())
        .collect()
}

/// The step of `job` whose `run:` line is exactly `cmd`, by position.
fn step_index(job: &Job, cmd: &str) -> Option<usize> {
    job.body
        .split("\n      - ")
        .position(|s| s.lines().any(|l| l.trim() == format!("run: {cmd}")))
}

/// FTR-004 T12. The workflow triggers on a server tag and on a client tag, and no tag matches
/// both: a tag filter matches the whole name, and the two patterns start with different literals.
#[test]
fn release_workflow_triggers_on_a_client_tag() {
    let text = read(".github/workflows/release.yml");
    let tags = tag_patterns(&text);
    assert_eq!(
        tags,
        [SERVER_TAG, CLIENT_TAG],
        "on.push.tags must be the server tag and the client tag"
    );
    let literal = |p: &str| -> String {
        p.trim_matches('"')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect()
    };
    let (s, c) = (literal(SERVER_TAG), literal(CLIENT_TAG));
    assert!(
        !s.starts_with(&c) && !c.starts_with(&s),
        "the tag patterns' literal prefixes `{s}` and `{c}` overlap"
    );
}

/// FTR-004 T12. On a client tag, `publish-client` runs after ci-green, refuses a tag that is not
/// the crate's version, runs the publish dry run, then publishes (skipped on dry_run) with the
/// credential the server publish uses.
#[test]
fn client_tag_checks_the_version_and_dry_runs_before_publishing() {
    let text = read(".github/workflows/release.yml");
    let jobs = jobs(&text);
    let client = jobs.get("publish-client").expect("no publish-client job");
    assert_eq!(
        client.needs,
        ["ci-green"],
        "publish-client must need ci-green"
    );
    assert!(
        client
            .body
            .contains(&format!("    if: ${{{{ {ON_CLIENT_TAG} }}}}")),
        "publish-client must run only on a client tag"
    );
    assert!(
        client
            .body
            .contains("select(.name == \"sigillr-client\") | .version")
            && client
                .body
                .contains("if [ \"${GITHUB_REF_NAME}\" != \"sigillr-client-v${version}\" ]; then")
            && client.body.contains("exit 1"),
        "publish-client must refuse a tag that is not sigillr-client-v<crate version>"
    );
    let check = client
        .body
        .split("\n      - ")
        .position(|s| s.contains("GITHUB_REF_NAME"))
        .expect("version check step");
    let dry = step_index(client, "cargo publish -p sigillr-client --dry-run --locked")
        .expect("client dry run step");
    let real = step_index(client, "cargo publish -p sigillr-client --locked")
        .expect("client publish step");
    assert!(
        check < dry && dry < real,
        "order must be version check, dry run, publish"
    );
    let step = client.body.split("\n      - ").nth(real).unwrap();
    assert!(
        step.contains("if:") && step.contains("inputs.dry_run"),
        "the client publish is not skipped on dry_run:\n{step}"
    );
    assert!(
        step.contains("CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}"),
        "the client publish must use the workflow's crates.io credential"
    );
    for forbidden in ["gh release", "upload-artifact", "attest", "docker"] {
        assert!(
            !client.body.contains(forbidden),
            "publish-client must do nothing but publish the crate: found `{forbidden}`"
        );
    }
}

/// FTR-004 T12. A server tag does not publish the client (its place in a server release arrives
/// with v0.2.0), and a client tag runs none of the server's jobs.
#[test]
fn server_tag_does_not_publish_the_client() {
    let text = read(".github/workflows/release.yml");
    let jobs = jobs(&text);
    for (name, job) in &jobs {
        if name != "publish-client" {
            assert!(
                !job.body.contains("-p sigillr-client") && !job.body.contains("\"sigillr-client\""),
                "job `{name}` must not publish or select sigillr-client"
            );
        }
    }
    let skip = format!("    if: ${{{{ !{ON_CLIENT_TAG} }}}}");
    for name in ["no-c", "build", "sbom"] {
        let job = jobs.get(name).unwrap_or_else(|| panic!("no {name} job"));
        assert!(
            job.body.contains(&skip),
            "job `{name}` must skip on a client tag"
        );
    }
    for name in ["artifact-scan", "publish"] {
        assert!(
            ["build", "sbom"].iter().any(|n| reaches(&jobs, name, n)),
            "job `{name}` must follow a job that skips on a client tag"
        );
    }
}
