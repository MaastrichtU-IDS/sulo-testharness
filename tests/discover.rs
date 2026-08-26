//! `suite::discover`, the step that decides what the harness will
//! actually check.
//!
//! Discovery is where this project's recurring defect shape lives at
//! the program level: a suite root that silently matches nothing would
//! produce a confident green having asked the reasoner nothing at all.
//! So the tests below are as interested in what discovery REFUSES (a
//! missing root, a file for a root, a root with no cases, a root whose
//! only `*.yaml` are fixtures) as in what it finds.
//!
//! Every fixture tree is built here rather than committed, so each
//! rule can be exercised in isolation. A committed tree would make
//! "the `data/` skip works" and "the sort works" share one fixture,
//! and a bug in either could hide behind the other.

use std::path::{Path, PathBuf};

use sulo_testharness::manifest::load_case;
use sulo_testharness::suite::{SuiteError, discover};

/// A process-and-test-unique scratch directory, removed and recreated
/// so a previous run's leftovers can never be counted as cases (which
/// would be a discovery test that passes for the wrong reason).
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sulo-testharness-discover-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

/// Create `root/rel`, parents included, with placeholder content.
fn touch(root: &Path, rel: &str) -> PathBuf {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().expect("a file has a parent"))
        .expect("parent dirs should be creatable");
    std::fs::write(&path, "# fixture\n").expect("file should be writable");
    path
}

// ---------------------------------------------------------------
// What discovery finds.
// ---------------------------------------------------------------

#[test]
fn yaml_is_found_recursively() {
    let root = scratch("recursive");
    let top = touch(&root, "top.yaml");
    let nested = touch(&root, "group/nested.yaml");
    let deeper = touch(&root, "group/sub/deeper.yaml");

    let found = discover(&root).expect("a root with cases should discover them");

    assert_eq!(
        found,
        vec![nested, deeper, top],
        "every *.yaml at every depth should be discovered"
    );
}

#[test]
fn non_yaml_files_are_not_cases() {
    let root = scratch("non-yaml");
    let case = touch(&root, "case.yaml");
    touch(&root, "notes.md");
    touch(&root, "ontology.ttl");
    touch(&root, "question.rq");

    let found = discover(&root).expect("the one yaml should be discovered");

    assert_eq!(found, vec![case], "only *.yaml files are cases");
}

/// Ruling 13: a `*.yml` is REFUSED, not skipped.
///
/// An earlier version of this suite pinned the opposite, that `.yml`
/// was silently not discovered. One stray `.yml` among 66 `.yaml`
/// files would then be read by nobody and reported by nothing: this
/// project's recurring defect shape, a check that cannot fail, arrived
/// at through a file extension. Refusing keeps one convention and
/// makes the silent skip impossible.
#[test]
fn a_stray_yml_is_refused_rather_than_skipped() {
    let root = scratch("stray-yml");
    touch(&root, "case.yaml");
    touch(&root, "oops.yml");

    let err = discover(&root).expect_err("a stray *.yml must be refused");
    let msg = err.to_string();
    assert!(
        msg.contains("oops.yml") && msg.contains("Rename it to .yaml"),
        "the refusal must name the file and the remedy: {msg}"
    );
}

/// The refusal is scoped to cases. A `.yml` inside a fixture directory
/// is not a case manifest and must not trip it, or a suite could never
/// hold a `.yml` fixture at all.
#[test]
fn a_yml_inside_a_fixture_directory_is_not_refused() {
    let root = scratch("yml-fixture");
    let case = touch(&root, "case.yaml");
    touch(&root, "data/thing.yml");
    touch(&root, "queries/thing.yml");

    let found = discover(&root).expect("a *.yml under data/ or queries/ is a fixture, not a case");
    assert_eq!(found, vec![case]);
}

// ---------------------------------------------------------------
// What discovery skips: the fixture directories.
// ---------------------------------------------------------------

#[test]
fn a_yaml_under_data_is_not_a_case() {
    let root = scratch("skip-data");
    let case = touch(&root, "case.yaml");
    touch(&root, "data/fixture.yaml");

    let found = discover(&root).expect("the real case should still be discovered");

    assert_eq!(
        found,
        vec![case],
        "a *.yaml under data/ is a fixture, not a case"
    );
}

#[test]
fn a_yaml_under_queries_is_not_a_case() {
    let root = scratch("skip-queries");
    let case = touch(&root, "case.yaml");
    touch(&root, "queries/fixture.yaml");

    let found = discover(&root).expect("the real case should still be discovered");

    assert_eq!(
        found,
        vec![case],
        "a *.yaml under queries/ is a fixture, not a case"
    );
}

#[test]
fn fixture_directories_are_skipped_at_any_depth() {
    let root = scratch("skip-deep");
    let case = touch(&root, "group/sub/case.yaml");
    touch(&root, "group/sub/data/fixture.yaml");
    touch(&root, "group/queries/fixture.yaml");

    let found = discover(&root).expect("the nested case should be discovered");

    assert_eq!(
        found,
        vec![case],
        "the skip is by directory name at any depth, not just directly under the root"
    );
}

// ---------------------------------------------------------------
// What discovery refuses. Each of these would otherwise be a run
// that checks nothing and reports a pass.
// ---------------------------------------------------------------

#[test]
fn a_missing_root_is_an_error() {
    let root = scratch("missing").join("no-such-directory");

    let err = discover(&root).expect_err("a root that does not exist must not discover cases");

    assert!(
        matches!(err, SuiteError::RootMissing { .. }),
        "expected RootMissing, got {err:?}"
    );
}

#[test]
fn a_file_for_a_root_is_an_error() {
    let root = scratch("file-root");
    let file = touch(&root, "case.yaml");

    let err = discover(&file).expect_err("a file is not a suite root");

    assert!(
        matches!(err, SuiteError::RootNotDirectory { .. }),
        "expected RootNotDirectory, got {err:?}"
    );
}

#[test]
fn an_empty_root_is_an_error() {
    let root = scratch("empty");

    let err = discover(&root).expect_err("a root with no cases must not report success");

    assert!(
        matches!(err, SuiteError::NoCases { .. }),
        "expected NoCases, got {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("check nothing"),
        "the message must say why zero cases is refused, got: {msg}"
    );
}

#[test]
fn a_root_holding_only_fixtures_is_an_error() {
    // The two rules meeting: everything present is skipped as a
    // fixture, so the run would have had nothing to check. This is the
    // shape a mistyped `--suite suites/sulo/taxonomy/dat` would take
    // if the skip were ever widened, and the one that must not come
    // back green.
    let root = scratch("only-fixtures");
    touch(&root, "data/a.yaml");
    touch(&root, "queries/b.yaml");

    let err = discover(&root).expect_err("a root holding only fixtures has no cases");

    assert!(
        matches!(err, SuiteError::NoCases { .. }),
        "expected NoCases, got {err:?}"
    );
}

// ---------------------------------------------------------------
// Sortedness.
// ---------------------------------------------------------------

#[test]
fn results_are_sorted_regardless_of_creation_order() {
    // Created in deliberately reverse-sorted order, across two depths,
    // so a walk that simply appends what `read_dir` hands back cannot
    // produce the expected vector by construction. Enough entries that
    // an accidentally-sorted `read_dir` is not a plausible way for
    // this to pass with the sort removed. Verified by removing the
    // `found.sort()` in `discover` and watching this test fail.
    let root = scratch("sorted");
    let names = [
        "zulu.yaml",
        "yankee.yaml",
        "x-group/zulu.yaml",
        "x-group/alpha.yaml",
        "mike.yaml",
        "b-group/mike.yaml",
        "b-group/alpha.yaml",
        "alpha.yaml",
    ];
    let created: Vec<PathBuf> = names.iter().map(|n| touch(&root, n)).collect();

    let found = discover(&root).expect("the tree has cases");

    let mut expected = created.clone();
    expected.sort();
    assert_ne!(
        created, expected,
        "the fixture must be created in an order that differs from sorted order, \
         or this test would pass whether or not discover sorts"
    );
    assert_eq!(
        found, expected,
        "discover must return paths sorted, so two runs are diffable"
    );
}

// ---------------------------------------------------------------
// The real suite.
// ---------------------------------------------------------------

/// An independent walk of `root`, written differently from
/// `suite::walk` on purpose: it collects EVERY `*.yaml` first and
/// filters afterwards on the full relative path, where `discover`
/// prunes during descent. Two implementations of the same rule, so a
/// change to one is not silently mirrored by the other.
fn independent_walk(root: &Path) -> Vec<PathBuf> {
    fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("suite dir should be readable") {
            let path = entry.expect("dir entry should be readable").path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.extension().is_some_and(|e| e == "yaml") {
                out.push(path);
            }
        }
    }

    let mut all = Vec::new();
    collect(root, &mut all);
    all.retain(|p| {
        let rel = p.strip_prefix(root).expect("collected under root");
        !rel.components()
            .any(|c| c.as_os_str() == "data" || c.as_os_str() == "queries")
    });
    all.sort();
    all
}

/// The 66 real cases must all be discovered.
///
/// Three assertions, because each alone could pass vacuously:
///
/// 1. Set equality against an independent walk. Keeps the test true as
///    the suite grows, but would pass if BOTH walks returned nothing.
/// 2. A FLOOR of 66, the inventory as of this commit (spec section 9).
///    A floor and not an equality, so adding a case does not break the
///    test; its job is to stop assertion 1 from holding vacuously, and
///    to catch a discovery bug that halves the suite.
/// 3. Every discovered path parses as a real case manifest. Without
///    this, a discovery bug that returned fixture `*.yaml` files, or
///    the README, would satisfy both counts above. This is also what
///    makes the count meaningful: 66 CASES, not 66 files.
#[test]
fn the_real_sulo_suite_is_discovered_in_full() {
    let root = Path::new("suites/sulo");
    assert!(
        root.is_dir(),
        "these tests read the committed suite by relative path from the crate root"
    );

    let found = discover(root).expect("the real suite has cases");
    let independent = independent_walk(root);

    assert_eq!(
        found, independent,
        "discover must agree with an independent walk of the same tree"
    );
    assert!(
        found.len() >= 66,
        "the suite held 66 cases when this test was written; discover found {}. \
         The suite may grow, so this is a floor, but it must never shrink silently.",
        found.len()
    );
    for path in &found {
        assert!(
            !path
                .components()
                .any(|c| c.as_os_str() == "data" || c.as_os_str() == "queries"),
            "{} is a fixture, not a case",
            path.display()
        );
        load_case(path)
            .unwrap_or_else(|e| panic!("{} should parse as a case manifest: {e}", path.display()));
    }
}
