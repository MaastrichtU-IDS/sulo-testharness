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
//! a known baseline (`KNOWN_BASELINE_KIND`/`KNOWN_BASELINE_COUNT`) and
//! reports it via `Loaded::baseline_loss` instead of `Loaded::loss`,
//! so it no longer downgrades anything. That fixed the root cause, so
//! this file tests the real ontology, not a copy of it.

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
