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
