//! Group test for `suites/sulo/patterns/solid/`.
//!
//! Same reason this file exists as `tests/taxonomy.rs`: the harness
//! has no suite-directory runner yet, so nothing else runs these
//! cases at all. `data/measurement.ttl` adapts the SULO paper's
//! Figure 4 with skolemised IRIs; see
//! `suites/sulo/patterns/solid/README.md` for why (spec 9.1).

use std::path::Path;

use sulo_testharness::manifest::load_case;
use sulo_testharness::suite::run_case;
use sulo_testharness::verdict::Verdict;

const SULO: &str = "../sulo/sulo.ttl";
const DIR: &str = "suites/sulo/patterns/solid";

/// Which `Verdict` variant each case's id must produce. See
/// `tests/taxonomy.rs`'s module doc for why `Pass` vs `UnrefutedPass`
/// is checked as an exact variant: `unit-not-forced-unit` is a
/// `not_entails:` case, so its trustworthy outcome is `UnrefutedPass`,
/// never `Pass`.
const EXPECTED: &[(&str, VerdictKind)] = &[
    ("typing-chain", VerdictKind::Pass),
    ("unit-forced-feature", VerdictKind::Pass),
    ("unit-not-forced-unit", VerdictKind::UnrefutedPass),
    ("single-value", VerdictKind::Pass),
    ("value-quality-unit", VerdictKind::Pass),
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
fn every_solid_case_matches_its_expected_verdict() {
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

    let on_disk: std::collections::BTreeSet<String> = std::fs::read_dir(dir)
        .expect("suites/sulo/patterns/solid should be readable")
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
        "suites/sulo/patterns/solid contains a case not covered by this test's \
         EXPECTED table, or EXPECTED names a case no longer on disk"
    );
}

/// `single-value` asserts `expect_inconsistent: true`, so per
/// `run_case`'s consistency gate this must be the ONLY thing it
/// checks: pairing it with any other assertion field (in particular a
/// `cq:` block) would have that field silently skipped, never run.
#[test]
fn single_value_asserts_only_inconsistency() {
    let path = Path::new(DIR).join("single-value.yaml");
    let case = load_case(&path).unwrap_or_else(|e| panic!("{} should parse: {e}", path.display()));

    assert!(
        case.expect_inconsistent,
        "single-value must set expect_inconsistent: true"
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
        "single-value pairs expect_inconsistent: true with another assertion \
         field, which run_case's consistency gate would silently skip"
    );
}

/// The vacuous-pass trap: a `cq:` entry with an empty `expect_rows`
/// PASSES whenever the query returns zero rows. `value-quality-unit`
/// must declare a real, non-empty `expect_rows`, and it must not pair
/// its cq with `expect_inconsistent: true` (which would silently skip
/// the competency question per `run_case`'s gate).
#[test]
fn value_quality_unit_declares_non_empty_expect_rows() {
    let path = Path::new(DIR).join("value-quality-unit.yaml");
    let case = load_case(&path).unwrap_or_else(|e| panic!("{} should parse: {e}", path.display()));

    assert!(
        !case.expect_inconsistent,
        "value-quality-unit must not pair cq: with expect_inconsistent: true"
    );
    assert_eq!(
        case.cq.len(),
        1,
        "value-quality-unit should declare one cq entry"
    );
    let spec = &case.cq[0];
    assert!(
        !spec.expect_rows.is_empty(),
        "value-quality-unit's cq entry must declare non-empty expect_rows, or \
         it passes vacuously whenever the query returns zero rows"
    );
    let row = &spec.expect_rows[0];
    for var in ["value", "quality", "unit"] {
        assert!(
            row.get(var).is_some_and(|v| v.is_some()),
            "value-quality-unit's expected row should bind {var}"
        );
    }
}

/// Verifies the opposite direction of the `expect_rows` trap: a CQ
/// that would pass with WRONG expectations is asserting nothing. This
/// does not mutate the suite manifest; it rebuilds an equivalent
/// `CqSpec` with a deliberately wrong value and confirms `check_cq`
/// fails.
#[test]
fn value_quality_unit_fails_with_a_wrong_expected_row() {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use sulo_testharness::cq::check_cq;
    use sulo_testharness::load::{load_file, merge};
    use sulo_testharness::manifest::CqSpec;
    use sulo_testharness::materialize::materialize;
    use sulo_testharness::prefixes::{base_mapping, with_overrides};

    let mut onto = load_file(sulo()).unwrap().ontology;
    let data = load_file(&Path::new(DIR).join("data/measurement.ttl"))
        .unwrap()
        .ontology;
    merge(&mut onto, data);
    let store = materialize(&onto, Duration::from_secs(30)).unwrap();

    let mut overrides = BTreeMap::new();
    overrides.insert("ex".to_string(), "http://example.org/".to_string());
    overrides.insert(
        "obo".to_string(),
        "http://purl.obolibrary.org/obo/".to_string(),
    );
    let pm = with_overrides(&base_mapping(), &overrides);

    let mut wrong_row = BTreeMap::new();
    wrong_row.insert(
        "value".to_string(),
        Some(r#""999"^^xsd:decimal"#.to_string()),
    );
    wrong_row.insert("quality".to_string(), Some("ex:height-quality".to_string()));
    wrong_row.insert("unit".to_string(), Some("ex:unit".to_string()));

    let spec = CqSpec {
        query: Path::new("queries/value-quality-unit.rq").to_path_buf(),
        expect_rows: vec![wrong_row],
        exact: true,
        ordered: false,
    };

    let out = check_cq(&store, &spec, Path::new(DIR), &pm);
    match out.verdict {
        Verdict::Fail(_) => {}
        other => panic!(
            "value-quality-unit with a deliberately wrong expected value must \
             FAIL, got {other:?}. A CQ that passes with wrong expectations is \
             asserting nothing."
        ),
    }
}
