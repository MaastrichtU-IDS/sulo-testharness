//! Group test for `suites/sulo/domains-ranges/`.
//!
//! Same reason this file exists as `tests/taxonomy.rs` and
//! `tests/restrictions.rs`: the harness has no suite-directory runner
//! yet, so nothing else runs these cases at all. Without this file the
//! YAML manifests under `suites/sulo/domains-ranges/` would sit
//! unexecuted, syntactically valid and silently untested.
//!
//! Every case here is a positive `entails:`/`instance_of_expr:` case
//! or an `expect_inconsistent: true` counter-example, so every
//! enforced verdict below is `Pass`; see `tests/taxonomy.rs`'s module
//! doc for why `Pass` vs `UnrefutedPass` matters and is checked as an
//! exact variant, not just "did not fail".

use std::path::Path;

use sulo_testharness::manifest::load_case;
use sulo_testharness::suite::run_case;
use sulo_testharness::verdict::Verdict;

const SULO: &str = "../sulo/sulo.ttl";
const DIR: &str = "suites/sulo/domains-ranges";

/// Every case id this group's `EXPECTED` table enforces, all `Pass`.
/// See `suites/sulo/domains-ranges/README.md` for why six of SULO's
/// 18 object properties (the parthood/containment family) have no
/// case at all: they carry no `rdfs:domain`/`rdfs:range` axiom in
/// `sulo.ttl`, and why the `owl:Thing` side of six more is skipped as
/// a tautology.
const EXPECTED: &[&str] = &[
    "attime",
    "istimeof",
    "isprecededby",
    "precedes",
    "isreferredtoin",
    "refersto",
    "hasfeature",
    "isfeatureof",
    "hasitem",
    "isitemin",
    "hasparticipant",
    "isparticipantin",
    "hasvalue",
    "range-violation-hasparticipant",
];

/// `SULO`, checked to exist first, matching the same prerequisite
/// every other group test in this crate already guards.
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
fn every_domains_ranges_case_matches_its_expected_verdict() {
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

    // No manifest under suites/sulo/domains-ranges left untested, and
    // no stale entry in EXPECTED naming a file that no longer exists.
    let on_disk: std::collections::BTreeSet<String> = std::fs::read_dir(dir)
        .expect("suites/sulo/domains-ranges should be readable")
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
        "suites/sulo/domains-ranges contains a case not covered by this test's \
         EXPECTED table, or EXPECTED names a case no longer on disk"
    );
}

/// `range-violation-hasparticipant` asserts `expect_inconsistent:
/// true`, so per `run_case`'s consistency gate this must be the ONLY
/// thing it checks: pairing `expect_inconsistent: true` with any
/// other assertion field would have that field silently skipped,
/// never run. This guards the manifest itself, not just its verdict.
#[test]
fn the_violation_case_asserts_only_inconsistency() {
    let path = Path::new(DIR).join("range-violation-hasparticipant.yaml");
    let case = load_case(&path).unwrap_or_else(|e| panic!("{} should parse: {e}", path.display()));

    assert!(
        case.expect_inconsistent,
        "range-violation-hasparticipant must set expect_inconsistent: true"
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
        "range-violation-hasparticipant pairs expect_inconsistent: true with \
         another assertion field, which run_case's consistency gate would \
         silently skip"
    );

    let result = run_case(&case, sulo());
    assert_eq!(
        result.verdict,
        Verdict::Pass,
        "range-violation-hasparticipant must be a trustworthy Pass on real SULO"
    );
}

/// The two union-domain/range cases (`hasfeature`, `isfeatureof`) must
/// actually exercise `instance_of_expr`, not just `entails:`: a case
/// that silently dropped its `instance_of_expr` entry would still
/// parse and could still pass on the named-class half alone, leaving
/// the union half untested while looking identical to a passing case.
#[test]
fn the_union_cases_declare_an_instance_of_expr() {
    for id in ["hasfeature", "isfeatureof"] {
        let path = Path::new(DIR).join(format!("{id}.yaml"));
        let case =
            load_case(&path).unwrap_or_else(|e| panic!("{} should parse: {e}", path.display()));
        assert!(
            !case.instance_of_expr.is_empty(),
            "{id} should declare instance_of_expr to exercise its union \
             domain/range side"
        );
    }
}
