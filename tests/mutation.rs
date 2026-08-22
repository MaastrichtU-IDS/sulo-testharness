//! Mutation self-test.
//!
//! Each mutant is a deliberately broken copy of SULO. For every
//! (mutant, case) pair listed here, the case MUST fail on the mutant
//! and MUST pass on clean SULO. A mutant nothing catches is a
//! coverage hole in the suite, not a passing test.
//!
//! `CLEAN` is the real, unmodified `../sulo/sulo.ttl`. An earlier
//! version of this file used a doctored `mutants/clean.ttl` copy,
//! because real SULO permanently carries a `SubClassOf` loss on its
//! conversion channel (two axioms, on `TimeInstant` and
//! `InformationObject`, whose data ranges rustdl's IR cannot
//! represent) that used to downgrade every positive-expectation
//! `Fail` to `Indeterminate` regardless of whether a mutation was the
//! cause. `src/load.rs` now recognises that exact, permanent shape as
//! a known baseline (`KNOWN_BASELINE_KIND`/`KNOWN_BASELINE_COUNT`,
//! anchored on the two named axioms actually being present, not just
//! their aggregate shape) and reports it via `Loaded::baseline_loss`
//! instead of `Loaded::loss`, so it no longer downgrades anything.
//! That fixed the root cause, so this file tests the real ontology,
//! not a copy of it.
//!
//! `mutants_are_not_stale_against_current_sulo`, below, is a second,
//! independent kind of test from the four `assert_caught` tests: it
//! does not run the reasoner at all. It re-derives, in Rust, exactly
//! what each mutant should be from whatever `../sulo/sulo.ttl`
//! currently contains, and compares byte-for-byte against the
//! committed mutant file. Without it, a SULO edit could go
//! unreflected in the mutant files forever: `assert_caught`'s "clean"
//! half would read the new ontology while its "mutant" half kept
//! reading the old, frozen one, so all four tests could stay green
//! while proving nothing about the CURRENT ontology, the exact
//! "green while testing nothing" failure this whole file exists to
//! rule out, reintroduced one level up. `mutants/regenerate.sh`
//! performs the same four edits from the shell/Python side, so a
//! SULO bump is one command; this test is what makes forgetting to
//! run it a build failure instead of a silent gap.

use std::path::{Path, PathBuf};

use sulo_testharness::manifest::load_case;
use sulo_testharness::suite::run_case;
use sulo_testharness::verdict::Verdict;

const CLEAN: &str = "../sulo/sulo.ttl";

fn verdict_of(case_file: &str, ontology: &Path) -> Verdict {
    let case = load_case(Path::new(case_file)).expect("case should parse");
    run_case(&case, ontology).verdict
}

fn assert_caught(mutant: &str, case_file: &str) {
    let mutant_path = PathBuf::from("mutants").join(mutant);

    let clean = verdict_of(case_file, Path::new(CLEAN));
    assert!(
        matches!(clean, Verdict::Pass | Verdict::UnrefutedPass),
        "{case_file} must pass on clean SULO, got {clean:?}"
    );

    let mutated = verdict_of(case_file, &mutant_path);
    assert!(
        matches!(mutated, Verdict::Fail(_)),
        "{case_file} must FAIL on mutant {mutant}, got {mutated:?}. \
         An uncaught mutant is a coverage hole."
    );
}

fn assert_passes(case_file: &str, ontology: &Path) {
    let v = verdict_of(case_file, ontology);
    assert!(
        matches!(v, Verdict::Pass | Verdict::UnrefutedPass),
        "{case_file} must still pass on {ontology:?}, got {v:?}"
    );
}

#[test]
fn deleting_the_role_chain_breaks_the_pro_case() {
    assert_caught("no-role-chain.ttl", "suites/proof/role-chain.yaml");
}

#[test]
fn dropping_parthood_transitivity_breaks_the_transitivity_case() {
    // `no-transitive-parthood.ttl` removes owl:TransitiveProperty from
    // BOTH sulo:isPartOf and its inverse sulo:hasPart. An earlier,
    // narrower mutant (removing it only from isPartOf) was
    // semantically inert: hasPart's own, untouched transitivity plus
    // the inverseOf link fully re-derives isPartOf's transitivity
    // regardless. See mutants/README.md for the empirical trace.
    assert_caught(
        "no-transitive-parthood.ttl",
        "suites/proof/transitivity-ispartof.yaml",
    );
}

#[test]
fn deleting_the_feature_disjoint_union_breaks_only_the_covering_case() {
    assert_caught("no-feature-union.ttl", "suites/proof/covering-feature.yaml");

    // "Only": the other three proof cases must still pass on this
    // exact mutant. This is the whole point of the AllDisjointClasses
    // recovery in src/load.rs: the redundant AllDisjointClasses axiom
    // over the same four classes still reaches the reasoner once the
    // disjointUnionOf covering half is gone, so pairwise disjointness
    // survives even though the covering property does not. If this
    // failed, either the recovery regressed or the mutant edit spilled
    // into an axiom it should not have touched.
    let mutant = PathBuf::from("mutants").join("no-feature-union.ttl");
    assert_passes("suites/proof/role-chain.yaml", &mutant);
    assert_passes("suites/proof/transitivity-ispartof.yaml", &mutant);
    assert_passes("suites/proof/subproperty-isin.yaml", &mutant);
}

#[test]
fn deleting_the_parthood_containment_subproperty_axioms_breaks_the_isin_case() {
    // `no-subproperty-containment.ttl` removes BOTH
    // `isPartOf rdfs:subPropertyOf isIn` and its inverse-side
    // counterpart `hasPart rdfs:subPropertyOf contains`. An earlier,
    // narrower mutant (removing only the isPartOf/isIn link) was
    // semantically inert for the same shape of reason as the
    // transitivity mutant above: the parallel route through
    // hasPart/contains's inverse fully re-derives the isIn conclusion.
    // See mutants/README.md for the empirical trace.
    assert_caught(
        "no-subproperty-containment.ttl",
        "suites/proof/subproperty-isin.yaml",
    );
}

// ---------------------------------------------------------------
// Staleness guard (fix round 2, IMPORTANT 3): each mutant must equal
// CURRENT clean SULO with exactly its documented edit applied. These
// functions independently re-derive, in Rust, the same four edits
// mutants/regenerate.sh performs in Python/shell, so drift between
// "what the mutant file contains" and "what today's SULO plus one
// edit would produce" is a test failure, not a silent gap.
// ---------------------------------------------------------------

fn expected_no_role_chain(sulo: &str) -> String {
    let needle = "    owl:inverseOf sulo:isParticipantIn ;\n    \
                  owl:propertyChainAxiom ( sulo:hasParticipant [ owl:inverseOf sulo:hasFeature ] ) .";
    assert_eq!(
        sulo.matches(needle).count(),
        1,
        "propertyChainAxiom anchor text not found exactly once in current SULO; \
         mutants/regenerate.sh and this staleness check both need updating"
    );
    sulo.replacen(needle, "    owl:inverseOf sulo:isParticipantIn .", 1)
}

fn strip_transitive_block(text: &str, anchor: &str) -> String {
    let start = text
        .find(anchor)
        .unwrap_or_else(|| panic!("anchor {anchor:?} not found in current SULO"));
    let end = text[start..]
        .find("\n\n")
        .map(|i| start + i)
        .unwrap_or_else(|| panic!("no blank-line block end found after {anchor:?}"));
    let block = &text[start..end];
    let patched = block.replace(
        "owl:ReflexiveProperty,\n        owl:TransitiveProperty ;",
        "owl:ReflexiveProperty ;",
    );
    assert_ne!(
        patched, block,
        "transitivity pattern not found for {anchor:?} in current SULO"
    );
    format!("{}{}{}", &text[..start], patched, &text[end..])
}

fn expected_no_transitive_parthood(sulo: &str) -> String {
    let out = strip_transitive_block(sulo, "sulo:isPartOf a owl:ObjectProperty");
    strip_transitive_block(&out, "sulo:hasPart a owl:ObjectProperty")
}

fn expected_no_feature_union(sulo: &str) -> String {
    let needle = "    owl:disjointUnionOf ( sulo:Capability sulo:InformationObject \
                  sulo:Quality sulo:Role ) ;\n";
    assert_eq!(
        sulo.matches(needle).count(),
        1,
        "disjointUnionOf line not found exactly once in current SULO; \
         mutants/regenerate.sh and this staleness check both need updating"
    );
    sulo.replacen(needle, "", 1)
}

fn expected_no_subproperty_containment(sulo: &str) -> String {
    let needle_isin = "    rdfs:subPropertyOf sulo:isIn .";
    assert_eq!(
        sulo.matches(needle_isin).count(),
        1,
        "isPartOf's subPropertyOf isIn line not found exactly once in current SULO"
    );
    let out = sulo.replacen(needle_isin, "    a owl:ObjectProperty .", 1);

    let needle_contains =
        "    rdfs:subPropertyOf sulo:contains ;\n    owl:inverseOf sulo:isPartOf .";
    assert_eq!(
        out.matches(needle_contains).count(),
        1,
        "hasPart's subPropertyOf contains line not found exactly once in current SULO"
    );
    out.replacen(needle_contains, "    owl:inverseOf sulo:isPartOf .", 1)
}

/// (mutant file name, function that derives its expected content from
/// current SULO).
type StalenessCase = (&'static str, fn(&str) -> String);

#[test]
fn mutants_are_not_stale_against_current_sulo() {
    let sulo = std::fs::read_to_string(CLEAN).expect("real SULO should be readable");

    let cases: [StalenessCase; 4] = [
        ("no-role-chain.ttl", expected_no_role_chain),
        (
            "no-transitive-parthood.ttl",
            expected_no_transitive_parthood,
        ),
        ("no-feature-union.ttl", expected_no_feature_union),
        (
            "no-subproperty-containment.ttl",
            expected_no_subproperty_containment,
        ),
    ];

    for (name, derive) in cases {
        let expected = derive(&sulo);
        let mutant_path = PathBuf::from("mutants").join(name);
        let actual = std::fs::read_to_string(&mutant_path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", mutant_path.display()));
        assert_eq!(
            expected, actual,
            "mutants/{name} is stale: it no longer equals current SULO with exactly \
             its documented edit applied. Run ./mutants/regenerate.sh, then re-review \
             the diff (a SULO change may have altered more than the target axiom)."
        );
    }
}
