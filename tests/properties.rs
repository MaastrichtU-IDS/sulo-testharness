//! Group test for `suites/sulo/properties/`.
//!
//! Same reason this file exists as `tests/taxonomy.rs`: the harness
//! has no suite-directory runner yet, so nothing else runs these 9
//! cases at all. Without this file the YAML manifests under
//! `suites/sulo/properties/` would sit unexecuted, syntactically
//! valid and silently untested.
//!
//! Every case here is checked against its EXACT expected verdict
//! variant, not merely "not a Fail". See `tests/taxonomy.rs`'s module
//! doc for why that distinction matters (`Pass` is a trustworthy
//! positive; `UnrefutedPass` is an honest "failed to refute", per
//! `oracle::verdict_for`'s soundness-driven asymmetry).

use std::path::Path;

use sulo_testharness::manifest::load_case;
use sulo_testharness::suite::run_case;
use sulo_testharness::verdict::Verdict;

const SULO: &str = "../sulo/sulo.ttl";
const DIR: &str = "suites/sulo/properties";

/// Which `Verdict` variant each case's id must produce, keyed by id so
/// a mismatched or renamed manifest file is caught rather than
/// silently skipped.
///
/// - `Pass`: every `entails:` case (the reasoner proves the positive
///   claim) and the `expect_inconsistent: true` counter-example (the
///   gate finds a real clash).
/// - `UnrefutedPass`: the one `not_entails:` case, an honest
///   absence-of-proof answer even when correct.
const EXPECTED: &[(&str, VerdictKind)] = &[
    ("subproperty-axioms", VerdictKind::Pass),
    ("inverse-pairs", VerdictKind::Pass),
    ("transitivity-ispartof", VerdictKind::Pass),
    ("transitivity-haspart", VerdictKind::Pass),
    ("transitivity-isin", VerdictKind::Pass),
    ("transitivity-contains", VerdictKind::Pass),
    ("reflexivity", VerdictKind::Pass),
    ("functional-hasvalue", VerdictKind::Pass),
    (
        "non-transitivity-isdirectpartof",
        VerdictKind::UnrefutedPass,
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerdictKind {
    Pass,
    UnrefutedPass,
}

fn matches_kind(v: &Verdict, kind: VerdictKind) -> bool {
    matches!(
        (v, kind),
        (Verdict::Pass, VerdictKind::Pass) | (Verdict::UnrefutedPass, VerdictKind::UnrefutedPass)
    )
}

/// `SULO`, checked to exist first, matching the same prerequisite
/// `tests/mutation.rs` and `tests/taxonomy.rs` already guard.
fn sulo() -> &'static Path {
    assert!(
        Path::new(SULO).is_file(),
        "{SULO} not found. These tests read real SULO, so the sulo repo must \
         be checked out as a sibling of sulo-testharness."
    );
    Path::new(SULO)
}

/// One file per case, and the file's own `id:` field must equal the
/// filename stem: catches a manifest that was copy-pasted and not
/// renamed, or renamed and not re-keyed.
#[test]
fn every_properties_case_matches_its_expected_verdict() {
    let dir = Path::new(DIR);
    assert!(dir.is_dir(), "{DIR} must exist");

    let mut seen = std::collections::BTreeSet::new();

    for (id, kind) in EXPECTED {
        let path = dir.join(format!("{id}.yaml"));
        let case = load_case(&path)
            .unwrap_or_else(|e| panic!("{} should parse as a manifest: {e}", path.display()));
        assert_eq!(
            &case.id,
            id,
            "{} declares id {:?}, expected {:?}",
            path.display(),
            case.id,
            id
        );

        let result = run_case(&case, sulo());
        assert!(
            matches_kind(&result.verdict, *kind),
            "{id}: expected {kind:?}, got {:?} (checks: {:#?})",
            result.verdict,
            result.checks
        );

        seen.insert(id.to_string());
    }

    // No manifest under suites/sulo/properties left untested, and no
    // stale entry in EXPECTED naming a file that no longer exists: a
    // suite directory with a case this table forgot to cover is
    // exactly the "green while testing nothing" failure this file
    // exists to prevent.
    let on_disk: std::collections::BTreeSet<String> = std::fs::read_dir(dir)
        .expect("suites/sulo/properties should be readable")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|e| e.to_str()) == Some("yaml"))
        .map(|entry| {
            entry
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap()
                .to_string()
        })
        .collect();

    assert_eq!(
        on_disk, seen,
        "suites/sulo/properties contains a case not covered by this test's EXPECTED \
         table, or EXPECTED names a case no longer on disk"
    );
}

/// `functional-hasvalue` asserts `expect_inconsistent: true`, so per
/// `run_case`'s consistency gate this must be the ONLY thing it
/// checks: pairing `expect_inconsistent: true` with any other
/// assertion field would have that field silently skipped, never run.
/// This guards the manifest itself, not just its verdict.
#[test]
fn functional_hasvalue_asserts_only_inconsistency() {
    let path = Path::new(DIR).join("functional-hasvalue.yaml");
    let case = load_case(&path).unwrap_or_else(|e| panic!("{} should parse: {e}", path.display()));

    assert!(
        case.expect_inconsistent,
        "functional-hasvalue must set expect_inconsistent: true"
    );
    assert!(
        case.entails.is_none()
            && case.not_entails.is_none()
            && case.entails_manchester.is_empty()
            && case.not_entails_manchester.is_empty()
            && case.instance_of_expr.is_empty()
            && case.satisfiable_expr.is_empty()
            && case.unsatisfiable.is_empty()
            && case.cq.is_empty(),
        "functional-hasvalue pairs expect_inconsistent: true with another assertion \
         field, which run_case's consistency gate would silently skip"
    );

    let result = run_case(&case, sulo());
    assert_eq!(
        result.verdict,
        Verdict::Pass,
        "functional-hasvalue must be a trustworthy Pass on real SULO"
    );
}
