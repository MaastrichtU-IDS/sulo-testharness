//! Group test for `suites/sulo/restrictions/`.
//!
//! Same reason this file exists as `tests/taxonomy.rs`: the harness
//! has no suite-directory runner yet, so nothing else runs these
//! cases at all. Without this file the YAML manifests under
//! `suites/sulo/restrictions/` would sit unexecuted, syntactically
//! valid and silently untested.
//!
//! Every case here is checked against its EXACT expected verdict
//! variant, not merely "not a Fail". See `tests/taxonomy.rs`'s module
//! doc for why that distinction matters (`Pass` is a trustworthy
//! positive; `UnrefutedPass` is an honest "failed to refute", per
//! `oracle::verdict_for`'s soundness-driven asymmetry). This group
//! has no `not_entails`-shaped case, so every enforced verdict below
//! is `Pass`.
//!
//! `timeinstant-datarange` is deliberately EXCLUDED from `EXPECTED`:
//! see `suites/sulo/restrictions/README.md` for why (a real, non-
//! tautological restriction the pinned reasoner cannot enforce). It
//! gets its own dedicated test below instead, documenting rather than
//! hiding the current `Fail`.

use std::path::Path;

use sulo_testharness::manifest::load_case;
use sulo_testharness::suite::run_case;
use sulo_testharness::verdict::Verdict;

const SULO: &str = "../sulo/sulo.ttl";
const DIR: &str = "suites/sulo/restrictions";

/// Which `Verdict` variant each case's id must produce, keyed by id so
/// a mismatched or renamed manifest file is caught rather than
/// silently skipped. `timeinstant-datarange` is intentionally absent;
/// see this file's module doc and `suites/sulo/restrictions/README.md`.
const EXPECTED: &[&str] = &[
    "hasPart-propagation-object",
    "hasPart-propagation-process",
    "hasPart-propagation-spatialobject",
    "hasPart-propagation-feature",
    "hasPart-propagation-informationobject",
    "quantity-haspart-some-unit",
    "feature-isfeatureof-some-object-or-process",
    "timeinterval-hasdirectpart-some-starttime",
    "timeinterval-hasdirectpart-some-endtime",
    "timeinterval-haspart-some-duration",
    "duration-nonnegative",
];

/// The one case on disk this group's `EXPECTED` table deliberately
/// does not cover. Named here, not just left out silently, so the
/// on-disk/`EXPECTED` diff below can still tell an untabled NEW case
/// apart from this known, documented exclusion.
const EXCLUDED: &str = "timeinstant-datarange";

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
fn every_restrictions_case_matches_its_expected_verdict() {
    let dir = Path::new(DIR);
    assert!(dir.is_dir(), "{DIR} must exist");

    let mut seen = std::collections::BTreeSet::new();

    for id in EXPECTED {
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
        assert_eq!(
            result.verdict,
            Verdict::Pass,
            "{id}: expected Pass, got {:?} (checks: {:#?})",
            result.verdict,
            result.checks
        );

        seen.insert(id.to_string());
    }

    // The one deliberate exclusion counts toward "on disk", so the
    // diff below flags only a genuinely untabled case, not this known
    // one.
    seen.insert(EXCLUDED.to_string());

    // No manifest under suites/sulo/restrictions left untested (save
    // the one documented exclusion), and no stale entry in EXPECTED
    // naming a file that no longer exists: a suite directory with a
    // case this table forgot to cover is exactly the "green while
    // testing nothing" failure this file exists to prevent.
    let on_disk: std::collections::BTreeSet<String> = std::fs::read_dir(dir)
        .expect("suites/sulo/restrictions should be readable")
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
        "suites/sulo/restrictions contains a case not covered by this test's EXPECTED \
         table (nor named as EXCLUDED), or EXPECTED/EXCLUDED names a case no longer \
         on disk"
    );
}

/// Every one of `hasPart-propagation-*` and `duration-nonnegative`
/// pulls its answer from real ABox data, not just the bare TBox, so
/// each needs its own `data:` fixture. This guards that structural
/// property directly: a case silently missing `data:` would still
/// parse and could still pass for a reason unrelated to the fixture
/// (e.g. the two `entails_manchester`-only TBox cases pass with no
/// `data:` at all, which is correct for THEM but would be a mistake
/// here).
#[test]
fn every_data_driven_case_declares_a_data_fixture() {
    let names = [
        "hasPart-propagation-object",
        "hasPart-propagation-process",
        "hasPart-propagation-spatialobject",
        "hasPart-propagation-feature",
        "hasPart-propagation-informationobject",
        "duration-nonnegative",
    ];
    for name in names {
        let path = Path::new(DIR).join(format!("{name}.yaml"));
        let case =
            load_case(&path).unwrap_or_else(|e| panic!("{} should parse: {e}", path.display()));
        assert!(
            !case.data.is_empty(),
            "{name} should declare a data: fixture; it asserts something about \
             instance data, not just the bare TBox"
        );
    }
}

/// `duration-nonnegative` asserts `expect_inconsistent: true`, so per
/// `run_case`'s consistency gate this must be the ONLY thing it
/// checks: pairing `expect_inconsistent: true` with any other
/// assertion field would have that field silently skipped, never run.
/// This guards the manifest itself, not just its verdict.
#[test]
fn duration_nonnegative_asserts_only_inconsistency() {
    let path = Path::new(DIR).join("duration-nonnegative.yaml");
    let case = load_case(&path).unwrap_or_else(|e| panic!("{} should parse: {e}", path.display()));

    assert!(
        case.expect_inconsistent,
        "duration-nonnegative must set expect_inconsistent: true"
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
        "duration-nonnegative pairs expect_inconsistent: true with another \
         assertion field, which run_case's consistency gate would silently skip"
    );

    let result = run_case(&case, sulo());
    assert_eq!(
        result.verdict,
        Verdict::Pass,
        "duration-nonnegative must be a trustworthy Pass on real SULO"
    );
}

/// `timeinstant-datarange` is a real, non-tautological restriction
/// the pinned reasoner cannot enforce (see this file's module doc and
/// `suites/sulo/restrictions/README.md`). This test documents the
/// current, known-wrong `Fail` rather than silently ignoring the
/// case: if this ever starts passing (a reasoner upgrade, or the
/// HermiT differential landing), THIS assertion breaks first, which
/// is the signal to move the case into `EXPECTED` above.
#[test]
fn timeinstant_datarange_is_tagged_and_currently_unenforced() {
    let path = Path::new(DIR).join(format!("{EXCLUDED}.yaml"));
    let case = load_case(&path).unwrap_or_else(|e| panic!("{} should parse: {e}", path.display()));

    assert_eq!(&case.id, EXCLUDED);
    assert!(
        case.tags.iter().any(|t| t == "oracle-hermit"),
        "{EXCLUDED} must carry the oracle-hermit tag"
    );
    assert!(
        case.expect_inconsistent,
        "{EXCLUDED} should still assert the true expectation, expect_inconsistent: true, \
         even though the pinned reasoner cannot yet confirm it"
    );

    let result = run_case(&case, sulo());
    match result.verdict {
        Verdict::Fail(_) => {}
        other => panic!(
            "{EXCLUDED}: expected the documented known-wrong Fail (rustdl v0.4.22 \
             cannot enforce data-range allValuesFrom), got {other:?} instead. If this \
             is now Pass, promote {EXCLUDED} into EXPECTED and update the README."
        ),
    }
}
