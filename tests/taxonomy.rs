//! Group test for `suites/sulo/taxonomy/`.
//!
//! This is what keeps the taxonomy suite honest in CI: the harness has
//! no suite-directory runner yet (that is a later task), so nothing
//! else runs these 22 cases at all. Without this file the YAML manifests
//! under `suites/sulo/taxonomy/` would sit unexecuted, syntactically
//! valid and silently untested.
//!
//! Every case here is checked against its EXACT expected verdict
//! variant, not merely "not a Fail". The variant matters: `Pass` is a
//! trustworthy positive (the reasoner found a proof), `UnrefutedPass`
//! is an honest negative (the reasoner merely failed to find a proof,
//! per `oracle::verdict_for`'s soundness-driven asymmetry). Asserting
//! only "case did not fail" would let a case silently downgrade from a
//! proof to an absence of proof, e.g. if an axiom needed for an
//! `entails:` claim were quietly lost.

use std::path::Path;

use sulo_testharness::manifest::load_case;
use sulo_testharness::suite::run_case;
use sulo_testharness::verdict::Verdict;

const SULO: &str = "../sulo/sulo.ttl";
const DIR: &str = "suites/sulo/taxonomy";

/// Which `Verdict` variant each case's id must produce, keyed by id so
/// a mismatched or renamed manifest file is caught rather than
/// silently skipped.
///
/// - `Pass`: every `entails:`/`entails_manchester:` case (the reasoner
///   proves the positive claim) and every `expect_inconsistent: true`
///   counter-example (the gate finds a real clash).
/// - `UnrefutedPass`: every `not_entails:`/`not_entails_manchester:`
///   case, and `all-classes-satisfiable`, whose `satisfiable_expr`
///   entries are, per `oracle::check_satisfiable_expr`'s own doc, an
///   absence-of-proof answer even when correct.
const EXPECTED: &[(&str, VerdictKind)] = &[
    ("all-classes-satisfiable", VerdictKind::UnrefutedPass),
    ("asserted-subsumptions", VerdictKind::Pass),
    ("deep-chain", VerdictKind::Pass),
    ("non-subsumptions", VerdictKind::UnrefutedPass),
    ("disjoint-capability-informationobject", VerdictKind::Pass),
    ("disjoint-capability-quality", VerdictKind::Pass),
    ("disjoint-capability-role", VerdictKind::Pass),
    ("disjoint-informationobject-quality", VerdictKind::Pass),
    ("disjoint-informationobject-role", VerdictKind::Pass),
    ("disjoint-quality-role", VerdictKind::Pass),
    ("disjoint-duration-timeinstant", VerdictKind::Pass),
    ("disjoint-duration-timeinterval", VerdictKind::Pass),
    ("disjoint-timeinstant-timeinterval", VerdictKind::Pass),
    ("disjoint-object-process", VerdictKind::Pass),
    ("disjoint-feature-spatialobject", VerdictKind::Pass),
    ("disjoint-time-unit", VerdictKind::Pass),
    ("disjoint-collection-quantity", VerdictKind::Pass),
    ("disjoint-endtime-starttime", VerdictKind::Pass),
    ("covering-feature", VerdictKind::Pass),
    ("covering-time", VerdictKind::Pass),
    ("non-covering-object", VerdictKind::UnrefutedPass),
    ("non-covering-informationobject", VerdictKind::UnrefutedPass),
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
/// `tests/mutation.rs` already guards.
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
fn every_taxonomy_case_matches_its_expected_verdict() {
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

    // No manifest under suites/sulo/taxonomy left untested, and no
    // stale entry in EXPECTED naming a file that no longer exists: a
    // suite directory with a case this table forgot to cover is
    // exactly the "green while testing nothing" failure this file
    // exists to prevent.
    let on_disk: std::collections::BTreeSet<String> = std::fs::read_dir(dir)
        .expect("suites/sulo/taxonomy should be readable")
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
        "suites/sulo/taxonomy contains a case not covered by this test's EXPECTED \
         table, or EXPECTED names a case no longer on disk"
    );
}

/// Every one of the 14 disjointness counter-examples asserts
/// `expect_inconsistent: true`, so per `run_case`'s consistency gate
/// this must be the ONLY thing each of them checks: pairing
/// `expect_inconsistent: true` with any of the case's other
/// assertion fields would have those fields silently skipped, never
/// run. This guards the manifests themselves, not just their
/// verdicts.
#[test]
fn every_disjointness_counter_example_asserts_only_inconsistency() {
    let names = [
        "disjoint-capability-informationobject",
        "disjoint-capability-quality",
        "disjoint-capability-role",
        "disjoint-informationobject-quality",
        "disjoint-informationobject-role",
        "disjoint-quality-role",
        "disjoint-duration-timeinstant",
        "disjoint-duration-timeinterval",
        "disjoint-timeinstant-timeinterval",
        "disjoint-object-process",
        "disjoint-feature-spatialobject",
        "disjoint-time-unit",
        "disjoint-collection-quantity",
        "disjoint-endtime-starttime",
    ];
    assert_eq!(
        names.len(),
        14,
        "the brief calls for exactly 14 counter-examples"
    );

    for name in names {
        let path = Path::new(DIR).join(format!("{name}.yaml"));
        let case =
            load_case(&path).unwrap_or_else(|e| panic!("{} should parse: {e}", path.display()));

        assert!(
            case.expect_inconsistent,
            "{name} must set expect_inconsistent: true"
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
            "{name} pairs expect_inconsistent: true with another assertion field, \
             which run_case's consistency gate would silently skip"
        );

        let result = run_case(&case, sulo());
        assert_eq!(
            result.verdict,
            Verdict::Pass,
            "{name} must be a trustworthy Pass on real SULO"
        );
    }
}
