//! Mutation self-test.
//!
//! Each mutant is a deliberately broken copy of SULO. For every
//! (mutant, case) pair listed here, the case MUST fail on the mutant
//! and MUST pass on clean SULO. A mutant nothing catches is a
//! coverage hole in the suite, not a passing test.
//!
//! `CLEAN` is `mutants/clean.ttl`, not `../sulo/sulo.ttl` directly.
//! See `mutants/README.md` for why: real SULO permanently carries two
//! `SubClassOf` axioms (on `Duration` and `InformationObject`) that
//! rustdl's IR conversion cannot represent and drops, unrelated to
//! any of the four axioms under test here. That drop is real loss and
//! `downgrade_for_loss` is right to distrust "no proof was found"
//! whenever it is present, but it means a positive-expectation case
//! run against literal `../sulo/sulo.ttl` can never resolve to a
//! trustworthy `Fail`, on a mutant or otherwise: this loss is baked
//! into the shipped file regardless of which axiom a mutant targets.
//! `clean.ttl` is real SULO with exactly those two already-dropped,
//! irrelevant restrictions textually removed; the reasoner never saw
//! them either way, so nothing this suite tests is weakened by their
//! absence. Every mutant is generated from `clean.ttl`, not from raw
//! SULO, for the same reason.

use std::path::{Path, PathBuf};

use sulo_testharness::manifest::load_case;
use sulo_testharness::suite::run_case;
use sulo_testharness::verdict::Verdict;

const CLEAN: &str = "mutants/clean.ttl";

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

#[test]
fn deleting_the_role_chain_breaks_the_pro_case() {
    assert_caught("no-role-chain.ttl", "suites/proof/role-chain.yaml");
}

#[test]
fn dropping_ispartof_transitivity_breaks_the_transitivity_case() {
    // KNOWN COVERAGE HOLE, left failing on purpose. See
    // `mutants/README.md` for the diagnosis: `sulo:hasPart` is
    // `owl:inverseOf sulo:isPartOf` and is independently declared
    // `owl:TransitiveProperty`, and OWL DL entails that a property's
    // inverse is transitive whenever it is. Removing ONLY isPartOf's
    // own `owl:TransitiveProperty` therefore removes nothing
    // reachable: isPartOf's transitivity is still fully entailed via
    // hasPart. Verified empirically, not just argued: this case
    // resolves to `Pass` on this mutant, not `Fail`.
    assert_caught(
        "no-transitive-ispartof.ttl",
        "suites/proof/transitivity-ispartof.yaml",
    );
}

#[test]
fn deleting_the_feature_disjoint_union_breaks_only_the_covering_case() {
    assert_caught("no-feature-union.ttl", "suites/proof/covering-feature.yaml");
}

#[test]
fn deleting_the_ispartof_isin_subproperty_axiom_breaks_the_isin_case() {
    // KNOWN COVERAGE HOLE, left failing on purpose, same shape as the
    // transitivity hole above. See `mutants/README.md` for the full
    // diagnosis: `sulo:contains` is `owl:inverseOf sulo:isIn`,
    // `sulo:hasPart rdfs:subPropertyOf sulo:contains`, and `sulo:hasPart`
    // is `owl:inverseOf sulo:isPartOf`. That parallel path (isPartOf ->
    // inverse hasPart -> subproperty contains -> inverse isIn), plus
    // isIn's own untouched transitivity, re-derives the same `isIn`
    // conclusion this mutant's removed axiom was supposed to be
    // necessary for. Verified empirically: this case resolves to
    // `Pass`, not `Fail`, on this mutant.
    assert_caught(
        "no-subproperty-isin.ttl",
        "suites/proof/subproperty-isin.yaml",
    );
}
