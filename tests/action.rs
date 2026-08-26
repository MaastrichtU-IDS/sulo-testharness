//! The composite action and the release workflow, checked against each other.
//!
//! Neither file runs under `cargo test`, so nothing else in this repository
//! reads them at all. Two failure modes are worth catching here rather than
//! on a tag push or, worse, at a consumer's first run:
//!
//! 1. A file that does not parse as YAML. A release workflow only ever runs
//!    on a pushed tag, and a tag is not cheap to retract.
//! 2. A rename in one file and not the other. The asset names are a contract
//!    between `release.yml` (which uploads them) and `action.yml` (which
//!    downloads them), and a mismatch is invisible until a consumer's job
//!    404s. These tests assert the two agree name for name AND pair for
//!    pair, so `Linux/X64` pointing at the macOS asset fails too.
//!
//! A test that only asserted "this parses" would pass against an empty
//! document. Every assertion below is about content.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_yaml::Value;

const REPO: &str = "MaastrichtU-IDS/sulo-testharness";
const SUITE_BUNDLE: &str = "sulo-suite.tar.gz";

/// The inputs spec section 11's consumer snippet and the plan's task 4 name.
const DECLARED_INPUTS: [&str; 5] = [
    "allow-indeterminate",
    "format",
    "ontology",
    "suite",
    "version",
];

fn repo_file(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn parse(relative: &str) -> Value {
    let path = repo_file(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is not readable: {e}", path.display()));
    serde_yaml::from_str(&text)
        .unwrap_or_else(|e| panic!("{} does not parse as YAML: {e}", path.display()))
}

fn action() -> Value {
    parse("action.yml")
}

fn release() -> Value {
    parse(".github/workflows/release.yml")
}

fn differential_workflow() -> Value {
    parse(".github/workflows/differential.yml")
}

/// The named step of the `differential` job.
fn differential_step(name: &str) -> Value {
    let wf = differential_workflow();
    let job = need(need(&wf, "jobs"), "differential").clone();
    steps(&job)
        .iter()
        .find(|s| key(s, "name").and_then(Value::as_str) == Some(name))
        .unwrap_or_else(|| panic!("differential.yml has no step named `{name}`"))
        .clone()
}

/// Mapping lookup by string key, without relying on any `Index` impl.
fn key<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    v.as_mapping()?
        .iter()
        .find(|(kk, _)| kk.as_str() == Some(k))
        .map(|(_, vv)| vv)
}

fn need<'a>(v: &'a Value, k: &str) -> &'a Value {
    key(v, k).unwrap_or_else(|| panic!("missing key `{k}`"))
}

fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    need(v, k)
        .as_str()
        .unwrap_or_else(|| panic!("key `{k}` is not a string"))
}

/// YAML 1.1 resolves a bare `on` key to the boolean true, and parsers differ
/// on whether they do. Accept either rather than quoting the key in the
/// workflow and hoping GitHub agrees.
fn trigger(workflow: &Value) -> &Value {
    key(workflow, "on")
        .or_else(|| {
            workflow.as_mapping()?.iter().find_map(|(k, v)| {
                if k.as_bool() == Some(true) {
                    Some(v)
                } else {
                    None
                }
            })
        })
        .expect("release.yml declares no trigger")
}

fn steps(job_or_action: &Value) -> &Vec<Value> {
    need(job_or_action, "steps")
        .as_sequence()
        .expect("steps is not a sequence")
}

/// Every `run:` script in a job or composite action, joined.
fn scripts(job_or_action: &Value) -> String {
    steps(job_or_action)
        .iter()
        .filter_map(|s| key(s, "run"))
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The step whose `id` is `wanted`.
fn step_with_id<'a>(job_or_action: &'a Value, wanted: &str) -> &'a Value {
    steps(job_or_action)
        .iter()
        .find(|s| key(s, "id").and_then(Value::as_str) == Some(wanted))
        .unwrap_or_else(|| panic!("no step with id `{wanted}`"))
}

/// Release asset names mentioned anywhere in `text`, recognised by the
/// `sulo-testharness-<platform>` shape. The bare binary name, and the
/// `sulo-testharness:` log prefix, deliberately do not match: the trailing
/// hyphen is required.
fn asset_names(text: &str) -> BTreeSet<String> {
    const PREFIX: &str = "sulo-testharness-";
    let bytes = text.as_bytes();
    let mut found = BTreeSet::new();
    let mut cursor = 0usize;
    while let Some(offset) = text[cursor..].find(PREFIX) {
        let start = cursor + offset;
        let mut end = start + PREFIX.len();
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'-' | b'_' | b'.'))
        {
            end += 1;
        }
        found.insert(text[start..end].to_string());
        cursor = end;
    }
    found
}

/// The `include:` entries of the build matrix.
fn build_matrix(release: &Value) -> Vec<Value> {
    let build = need(need(release, "jobs"), "build");
    need(need(need(build, "strategy"), "matrix"), "include")
        .as_sequence()
        .expect("matrix include is not a sequence")
        .clone()
}

// ---------------------------------------------------------------
// action.yml: the surface spec section 11 promises consumers.
// ---------------------------------------------------------------

#[test]
fn the_action_declares_exactly_the_documented_inputs() {
    let action = action();

    assert_eq!(text(need(&action, "runs"), "using"), "composite");

    let inputs = need(&action, "inputs")
        .as_mapping()
        .expect("inputs is not a mapping");
    let declared: BTreeSet<&str> = inputs.iter().filter_map(|(k, _)| k.as_str()).collect();
    let expected: BTreeSet<&str> = DECLARED_INPUTS.into_iter().collect();
    assert_eq!(
        declared, expected,
        "action.yml inputs drifted from the set the plan and spec section 11 name"
    );

    for (name, spec) in inputs {
        let name = name.as_str().unwrap();
        assert!(
            !text(spec, "description").trim().is_empty(),
            "input `{name}` has no description"
        );
        assert!(
            key(spec, "default").is_some(),
            "input `{name}` has no default; the consumer snippet in spec section 11 \
             passes `ontology` only, so every other input must default"
        );
    }
}

#[test]
fn the_action_defaults_let_the_spec_section_11_snippet_work() {
    let action = action();
    let inputs = need(&action, "inputs");

    // `- uses: MaastrichtU-IDS/sulo-testharness@v0.1.0
    //    with: { ontology: sulo.ttl }`
    assert_eq!(text(need(inputs, "ontology"), "default"), "sulo.ttl");
    assert_eq!(text(need(inputs, "format"), "default"), "text");
    // Empty means "use the suite bundled with the pinned release", which is
    // the only thing that can work when the consumer passes `ontology` alone:
    // the action downloads a binary, not this repository.
    assert_eq!(text(need(inputs, "suite"), "default"), "");

    let version = text(need(inputs, "version"), "default");
    assert!(
        version.starts_with('v') && version.len() > 1,
        "the default version `{version}` is not a release tag"
    );
}

#[test]
fn every_declared_input_reaches_bash_through_env_and_not_through_interpolation() {
    let action = action();
    let runs = need(&action, "runs");

    // A `with:` value is consumer-controlled text. Splicing it into a script
    // body with ${{ }} is a command-injection hole, so inputs must arrive as
    // environment variables.
    for step in steps(runs) {
        if let Some(script) = key(step, "run").and_then(Value::as_str) {
            assert!(
                !script.contains("${{ inputs."),
                "an input is interpolated directly into a run script; pass it via env instead"
            );
        }
    }

    let env_values: Vec<String> = steps(runs)
        .iter()
        .filter_map(|s| key(s, "env"))
        .filter_map(Value::as_mapping)
        .flat_map(|m| {
            m.iter()
                .filter_map(|(_, v)| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .collect();

    for input in DECLARED_INPUTS {
        let wanted = format!("inputs.{input}");
        assert!(
            env_values.iter().any(|v| v.contains(&wanted)),
            "input `{input}` is declared but never used; a declared-and-ignored input is a \
             knob that silently does nothing"
        );
    }
}

#[test]
fn the_action_invokes_the_run_subcommand_with_the_flags_it_documents() {
    let script = scripts(need(&action(), "runs"));
    for fragment in ["run --suite", "--format", "--ontology"] {
        assert!(
            script.contains(fragment),
            "the action no longer passes `{fragment}`; the CLI surface it targets is \
             `run --suite <dir> [--ontology <ttl>] [--filter <substr>] [--format text|json|junit]`"
        );
    }
}

#[test]
fn the_action_propagates_the_harness_exit_code() {
    let action = action();
    let runs = need(&action, "runs");
    let script = scripts(runs);

    // The recurring defect shape of this project, at the CI level: a
    // composite action that always succeeds silently disables the harness
    // for every consumer.
    assert!(
        script.contains("|| status=$?"),
        "the action does not capture the harness exit status"
    );
    assert!(
        script.contains("exit \"${status}\""),
        "the action does not re-raise the captured exit status as the step's own"
    );
    assert!(
        !script.contains("|| true"),
        "`|| true` in the action swallows a failure"
    );
    assert!(
        !script.contains("exit 0"),
        "an explicit `exit 0` can only override the harness's own verdict"
    );

    for step in steps(runs) {
        assert_ne!(
            key(step, "continue-on-error").and_then(Value::as_bool),
            Some(true),
            "continue-on-error would make this step incapable of failing"
        );
    }

    // Spec section 5.4: every documented code is named, including 3.
    // Indeterminate propagates as a failure by design; see the comment on
    // that arm in action.yml.
    //
    // Scoped to the `case ... esac` block, and matched against a
    // TRIMMED LINE START, because a bare `contains("4)")` is satisfied
    // by the `Linux/X64)` arm of the download dispatch and `contains
    // ("2)")` by a comment reading `# 12),`. A review proved it:
    // deleting both the 2) and 4) arms outright left every test in
    // this file green. That is the defect shape this project keeps
    // finding, inside the test written to prevent it.
    let case_block = {
        let start = script
            .find("case \"${status}\" in")
            .expect("the run script must dispatch on the harness's exit status");
        let end = script[start..]
            .find("esac")
            .expect("the case statement must be closed")
            + start;
        &script[start..end]
    };
    for code in 0..=5 {
        let arm = format!("{code})");
        assert!(
            case_block.lines().any(|l| l.trim_start().starts_with(&arm)),
            "exit code {code} from spec section 5.4 has no arm in the action's report"
        );
    }
    assert!(
        script.contains("Indeterminate"),
        "the Indeterminate arm no longer says what it is reporting"
    );
}

#[test]
fn the_action_downloads_from_the_current_repository_over_the_url_shape_github_serves() {
    let script = scripts(need(&action(), "runs"));
    // GitHub serves release assets at
    // https://github.com/OWNER/REPO/releases/download/TAG/ASSET.
    let expected = format!("https://github.com/{REPO}/releases/download/");
    assert!(
        script.contains(&expected),
        "the action's download URL is not `{expected}<tag>/<asset>`"
    );
    assert!(
        !script.contains("AIDAVA-DEV/sulo-testharness"),
        "the repository moved orgs; spec section 11's snippet is stale"
    );
}

// ---------------------------------------------------------------
// release.yml
// ---------------------------------------------------------------

#[test]
fn the_release_workflow_triggers_on_version_tags_only() {
    let release = release();
    let tags = need(need(trigger(&release), "push"), "tags")
        .as_sequence()
        .expect("push.tags is not a sequence");
    let patterns: Vec<&str> = tags.iter().filter_map(Value::as_str).collect();
    assert_eq!(
        patterns,
        vec!["v*"],
        "the release workflow no longer runs on exactly the pushed `v*` tags"
    );
    assert!(
        key(trigger(&release), "branches").is_none(),
        "a branch trigger would publish release assets from an untagged push"
    );
}

#[test]
fn the_release_workflow_builds_then_publishes_with_the_narrowest_write_scope() {
    let release = release();
    let jobs = need(&release, "jobs");

    let publish = need(jobs, "release");
    let needs = need(publish, "needs");
    let depends_on_build = needs.as_str() == Some("build")
        || needs
            .as_sequence()
            .is_some_and(|s| s.iter().any(|v| v.as_str() == Some("build")));
    assert!(
        depends_on_build,
        "the release job does not wait for the build job; a half-built release could publish"
    );

    // Read at the top, write only in the job that actually publishes.
    assert_eq!(text(need(&release, "permissions"), "contents"), "read");
    assert_eq!(text(need(publish, "permissions"), "contents"), "write");
    assert!(
        key(need(jobs, "build"), "permissions").is_none(),
        "the build job does not publish anything and needs no permission block"
    );
}

#[test]
fn the_release_workflow_builds_from_the_pinned_dependency_tree() {
    let release = release();
    let build = need(need(&release, "jobs"), "build");
    let script = scripts(build);
    // Cargo.toml's [patch.crates-io] redirect is what keeps `SetOntology` a
    // single type across rustdl and this crate. --locked means the shipped
    // binary is built from exactly the tree the tag was tested with, and
    // fails loudly if Cargo.lock is stale rather than resolving something
    // else.
    assert!(
        script.contains("--locked"),
        "the release build does not pass --locked"
    );
    assert!(
        script.contains("1.95.0"),
        "the release build does not pin the toolchain that rust-toolchain.toml names"
    );
}

#[test]
fn every_action_the_release_workflow_uses_is_pinned() {
    let release = release();
    let jobs = need(&release, "jobs").as_mapping().unwrap();
    let mut seen = 0;
    for (_, job) in jobs {
        for step in steps(job) {
            let Some(uses) = key(step, "uses").and_then(Value::as_str) else {
                continue;
            };
            seen += 1;
            let (_, git_ref) = uses
                .split_once('@')
                .unwrap_or_else(|| panic!("`uses: {uses}` names no ref at all"));
            let pinned = (git_ref.starts_with('v')
                && git_ref[1..].chars().all(|c| c.is_ascii_digit() || c == '.'))
                || (git_ref.len() == 40 && git_ref.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(
                pinned,
                "`uses: {uses}` floats; a workflow that can write releases must pin what it runs"
            );
        }
    }
    assert!(seen > 0, "found no `uses:` steps to check");
}

// ---------------------------------------------------------------
// The cross-file contract. This is the assertion that earns the file.
// ---------------------------------------------------------------

#[test]
fn the_action_and_the_release_workflow_agree_on_every_asset_name() {
    let action = action();
    let release = release();

    let matrix = build_matrix(&release);
    assert_eq!(
        matrix.len(),
        2,
        "the plan calls for exactly linux-x86_64 and macos-aarch64"
    );

    let uploaded: BTreeSet<String> = matrix
        .iter()
        .map(|entry| text(entry, "asset").to_string())
        .collect();

    let download = step_with_id(need(&action, "runs"), "download");
    let script = key(download, "run").and_then(Value::as_str).unwrap();
    let downloaded = asset_names(script);

    assert_eq!(
        downloaded, uploaded,
        "action.yml downloads assets the release workflow does not upload, or the other \
         way round; renaming one file without the other 404s at a consumer's first run"
    );

    // Names matching is not enough: the dispatch has to send each runner to
    // ITS asset. Every arm is one line, `<RUNNER_OS>/<RUNNER_ARCH>) asset=...`.
    for entry in &matrix {
        let arm = format!(
            "{}/{})",
            text(entry, "runner_os"),
            text(entry, "runner_arch")
        );
        let asset = text(entry, "asset");
        let line = script
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with(&arm))
            .unwrap_or_else(|| {
                panic!("action.yml has no dispatch arm for `{arm}`, which the release builds")
            });
        assert!(
            line.contains(asset),
            "action.yml sends `{arm}` to something other than `{asset}`: {line}"
        );
    }

    // The uploader must actually upload what the matrix declares. The
    // release job names the files explicitly, so a matrix rename that misses
    // that command is caught here.
    let publish = scripts(need(need(&release, "jobs"), "release"));
    for asset in &uploaded {
        assert!(
            publish.contains(asset.as_str()),
            "`{asset}` is built by the matrix but never attached to the release"
        );
    }
}

#[test]
fn the_action_and_the_release_workflow_agree_on_the_suite_bundle() {
    let action = action();
    let release = release();

    // The action downloads a binary, not a repository. Without the bundle,
    // spec section 11's `with: { ontology: sulo.ttl }` has an engine and no
    // cases, and a suite of zero cases is exit 2 by the plan's ruling 2.
    let download = step_with_id(need(&action, "runs"), "download");
    let script = key(download, "run").and_then(Value::as_str).unwrap();
    assert!(
        script.contains(SUITE_BUNDLE),
        "the action does not fetch `{SUITE_BUNDLE}`, so the default (empty) suite input \
         has nothing to run"
    );

    let publish = scripts(need(need(&release, "jobs"), "release"));
    assert!(
        publish.contains(SUITE_BUNDLE),
        "the release workflow does not attach `{SUITE_BUNDLE}`, which the action downloads"
    );
    assert!(
        publish.contains("tar -czf") || publish.contains("tar czf"),
        "the release workflow does not build the suite bundle it attaches"
    );

    // The bundle is rooted at `suites/` so the extracted tree matches the
    // repository layout the manifests' relative paths assume.
    assert!(
        script.contains("/suites/sulo"),
        "the action does not point at `suites/sulo` inside the extracted bundle"
    );
    assert!(
        repo_file("suites/sulo").is_dir(),
        "suites/sulo, the path the action defaults to inside the bundle, does not exist"
    );
}

#[test]
fn every_run_script_in_both_files_is_valid_bash() {
    // These scripts execute only on a runner, and the release workflow's
    // execute only on a pushed tag. A syntax error in one is not otherwise
    // observable until the moment it is most expensive.
    let mut checked = 0;
    for (file, root) in [
        ("action.yml", vec![need(&action(), "runs").clone()]),
        (
            ".github/workflows/release.yml",
            need(&release(), "jobs")
                .as_mapping()
                .unwrap()
                .iter()
                .map(|(_, job)| job.clone())
                .collect(),
        ),
        (
            ".github/workflows/differential.yml",
            need(&differential_workflow(), "jobs")
                .as_mapping()
                .unwrap()
                .iter()
                .map(|(_, job)| job.clone())
                .collect(),
        ),
    ] {
        for holder in &root {
            for step in steps(holder) {
                let Some(script) = key(step, "run").and_then(Value::as_str) else {
                    continue;
                };
                let name = key(step, "name")
                    .and_then(Value::as_str)
                    .unwrap_or("unnamed step");
                let path = std::env::temp_dir().join(format!(
                    "sulo-testharness-action-{}-{checked}.sh",
                    std::process::id()
                ));
                std::fs::write(&path, script).unwrap();
                let out = std::process::Command::new("bash")
                    .arg("-n")
                    .arg(&path)
                    .output()
                    .expect("bash is required to check the workflow scripts");
                let _ = std::fs::remove_file(&path);
                assert!(
                    out.status.success(),
                    "{file} step `{name}` is not valid bash: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 2, "found only {checked} run scripts to check");
}

/// `--allow-indeterminate` must be passed on the exact string "true".
///
/// The input's default is the NON-EMPTY string "false", so a `-n`
/// truthiness test would pass the flag on every consumer run and
/// silently lower exit 3 to 0: precisely the unearned green ruling 10
/// forbids. A review proved this was unguarded by swapping the
/// comparison for `-n` and watching all thirteen tests stay green.
#[test]
fn allow_indeterminate_is_gated_on_the_exact_string_true() {
    let script = scripts(need(&action(), "runs"));

    assert!(
        script.contains("--allow-indeterminate"),
        "the action declares an allow-indeterminate input but never passes the flag"
    );
    assert!(
        script.contains("\"${INPUT_ALLOW_INDETERMINATE}\" = \"true\""),
        "the flag must be gated on an equality test against \"true\": the input defaults to \
         the non-empty string \"false\", so any truthiness test turns an explicit no into a yes"
    );
    assert!(
        !script.contains("-n \"${INPUT_ALLOW_INDETERMINATE}\"")
            && !script.contains("-z \"${INPUT_ALLOW_INDETERMINATE}\""),
        "a -n/-z test on this input would fire on the default \"false\""
    );
}

// ---------------------------------------------------------------
// The differential job's one script, run for real.
// ---------------------------------------------------------------

/// GitHub runs a `run:` step that names no `shell:` as `bash -e {0}`,
/// so the Differential step's script starts with errexit ALREADY ON.
/// `set -uo pipefail` does not clear it. If it is left on, the first
/// `cargo run` to exit non-zero (which is every run this job exists to
/// report: 5 unpinned divergence, 4 stale pin, 3 unanswered question,
/// 2 misconfiguration) aborts the step at that pipeline, so
/// `differential.json` is never written and the upload step's
/// `if-no-files-found: warn` turns the missing report into a warning
/// nobody reads. The machine-readable report would exist only on the
/// runs that did not need it.
///
/// This is not a grep for `set +e`: the script is EXECUTED under the
/// same `bash -e` GitHub would use, with `cargo` stubbed to exit 5, and
/// the assertions are on what the step produced. A step that inherits
/// errexit prints nothing and writes no JSON, which is exactly the
/// failure the earlier fix to this step missed by reordering the lines
/// and leaving the `-e` alone.
#[test]
fn the_differential_step_does_not_inherit_errexit() {
    let step = differential_step("Differential");
    assert!(
        key(&step, "shell").is_none(),
        "this test models GitHub's DEFAULT shell for a run: step (`bash -e {{0}}`); if the \
         step now names its own `shell:`, this test must model that one instead"
    );
    let script = text(&step, "run");

    let dir = std::env::temp_dir().join(format!(
        "sulo-testharness-differential-step-{}",
        std::process::id()
    ));
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).expect("temp dir");
    let cargo = bin.join("cargo");
    // Every exit code this job cares about is non-zero; 5 is the
    // headline one, an unpinned divergence.
    std::fs::write(
        &cargo,
        "#!/bin/sh\necho 'stub differential output'\nexit 5\n",
    )
    .expect("stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let script_path = dir.join("step.sh");
    std::fs::write(&script_path, script).expect("script");

    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = std::process::Command::new("bash")
        .arg("-e")
        .arg(&script_path)
        .current_dir(&dir)
        .env("PATH", path)
        .output()
        .expect("bash is required to run the workflow script");

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let json = dir.join("differential.json");
    let json_written = json.is_file() && std::fs::metadata(&json).map(|m| m.len()).unwrap_or(0) > 0;
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        stdout.contains("differential exited 5"),
        "the step aborted before it could report its own exit code, which is what an \
         inherited `-e` does. Add `set +e` to the script's `set` line. stdout was: {stdout}"
    );
    assert!(
        json_written,
        "the step never wrote differential.json, so the machine-readable report would \
         exist only on the runs that did not need it. An inherited `-e` is the cause"
    );
    assert_eq!(
        out.status.code(),
        Some(5),
        "the step must re-raise the text run's exit code as its own"
    );
}
