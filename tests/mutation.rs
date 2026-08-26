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
//! independent kind of test from the `assert_caught` tests: it
//! does not run the reasoner at all. It re-derives, in Rust, exactly
//! what each mutant should be from whatever `../sulo/sulo.ttl`
//! currently contains, and compares byte-for-byte against the
//! committed mutant file. Without it, a SULO edit could go
//! unreflected in the mutant files forever: `assert_caught`'s "clean"
//! half would read the new ontology while its "mutant" half kept
//! reading the old, frozen one, so every one of them could stay green
//! while proving nothing about the CURRENT ontology, the exact
//! "green while testing nothing" failure this whole file exists to
//! rule out, reintroduced one level up. `mutants/regenerate.sh`
//! performs the same edits from the shell/Python side, so a
//! SULO bump is one command; this test is what makes forgetting to
//! run it a build failure instead of a silent gap.

use std::path::{Path, PathBuf};

use sulo_testharness::manifest::load_case;
use sulo_testharness::suite::run_case;
use sulo_testharness::verdict::Verdict;

const CLEAN: &str = "../sulo/sulo.ttl";

/// `CLEAN`, checked to exist first.
///
/// These tests read real SULO by relative path, so a checkout without
/// the sulo repo as a sibling directory would otherwise fail deep
/// inside `run_case` or on a bare `.expect()` with an `Io` error that
/// reads like a harness bug. `mutants/regenerate.sh` guards the same
/// prerequisite with an explicit message; this is that message.
fn clean_sulo() -> &'static Path {
    assert!(
        Path::new(CLEAN).is_file(),
        "{CLEAN} not found. These tests compare against real SULO, so the sulo \
         repo must be checked out as a sibling of sulo-testharness (the same \
         prerequisite mutants/regenerate.sh checks for)."
    );
    Path::new(CLEAN)
}

fn verdict_of(case_file: &str, ontology: &Path) -> Verdict {
    let case = load_case(Path::new(case_file)).expect("case should parse");
    run_case(&case, ontology).verdict
}

fn assert_caught(mutant: &str, case_file: &str) {
    let mutant_path = PathBuf::from("mutants").join(mutant);

    let clean = verdict_of(case_file, clean_sulo());
    assert_eq!(
        clean,
        Verdict::Pass,
        "{case_file} must pass on clean SULO with a TRUSTWORTHY Pass. Every case
         this helper is invoked for asserts something POSITIVE (an entailment, a
         competency-question row, or an expected inconsistency), never a negative
         expectation, so an UnrefutedPass here would itself be the defect: it
         would mean the proof this mutant is supposed to break was never found in
         the first place, and the mutant's Fail below would be proving nothing.
         Do not relax this to `Pass | UnrefutedPass` to make a new (mutant, case)
         pair go green; pick a case that proves something instead."
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
// Task 10: extending existing mutants' coverage to the new suite
// groups (taxonomy, properties, restrictions, domains-ranges,
// patterns/pro, patterns/solid), plus five brand-new mutants for the
// axiom shapes those groups exercise that the four predecessor
// mutants do not touch at all (a single named-class subClassOf, a
// hasPart-only-self restriction pair, a standalone hasPart-only-self
// restriction, a someValuesFrom restriction, and a domain/inverse-range
// pair). Every (mutant, case) pair below was verified empirically:
// Pass on clean SULO, Fail on the mutant.
// ---------------------------------------------------------------

#[test]
fn deleting_the_role_chain_also_breaks_the_new_pro_cases() {
    // Same mutant as `deleting_the_role_chain_breaks_the_pro_case`
    // above (`no-role-chain.ttl`), extended to the two new PRO group
    // cases per the task-10 brief: do not duplicate the mutant, only
    // the coverage.
    assert_caught(
        "no-role-chain.ttl",
        "suites/sulo/patterns/pro/role-chain.yaml",
    );
    assert_caught(
        "no-role-chain.ttl",
        "suites/sulo/patterns/pro/pattern-membership.yaml",
    );
    // The flagship competency question, and the spec's own worked
    // example of the `cq` schema. Its query asks for the encounter's
    // participants joined against their NCBITaxon typing, which is
    // reachable only through the role chain, so deleting the chain
    // suppresses the ex:alice row. Observed on the mutant:
    // Fail("missing expected row: {?p = <http://example.org/alice>}").
    // Added because the CQ path is what this branch delivers and,
    // before this, only patterns/solid/value-quality-unit of the
    // suite's two competency questions was mutation-proven.
    assert_caught(
        "no-role-chain.ttl",
        "suites/sulo/patterns/pro/who-participated.yaml",
    );
}

#[test]
fn deleting_object_process_disjointness_breaks_the_taxonomy_counter_example() {
    // The taxonomy group's 14 disjointness counter-examples had ZERO
    // committed mutant coverage before this: they were flip-verified
    // during design with scratch mutants that were deleted before
    // commit, so that verification was neither repeatable nor in CI,
    // and the one taxonomy mutant that did exist
    // (`no-feature-union.ttl`) asserts they must NOT react. This is
    // the first mutant that makes one of them react.
    //
    // `no-object-process-disjoint.ttl` deletes sulo:Object's
    // `owl:disjointWith sulo:Process`, which is the whole of the
    // Object/Process disjointness in sulo.ttl: the two
    // owl:AllDisjointClasses lists cover {Capability,
    // InformationObject, Quality, Role} and {Duration, TimeInstant,
    // TimeInterval}, neither of which mentions Object or Process, and
    // no disjointUnionOf covers the pair either. Unlike the four
    // paired-removal mutants, one deletion suffices here, verified
    // empirically rather than assumed: see mutants/README.md for why
    // Object's own `complementOf (hasPart some Process)` restriction
    // does not re-entail it under the pinned reasoner.
    assert_caught(
        "no-object-process-disjoint.ttl",
        "suites/sulo/taxonomy/disjoint-object-process.yaml",
    );
}

#[test]
fn deleting_the_feature_disjoint_union_also_breaks_the_taxonomy_covering_case() {
    // Same mutant as `deleting_the_feature_disjoint_union_breaks_only_the_covering_case`
    // above, extended to the taxonomy group's own covering-feature
    // case (a distinct file from suites/proof/covering-feature.yaml,
    // same entailed fact).
    assert_caught(
        "no-feature-union.ttl",
        "suites/sulo/taxonomy/covering-feature.yaml",
    );
}

#[test]
fn dropping_parthood_transitivity_also_breaks_the_properties_group_cases() {
    // Same mutant as `dropping_parthood_transitivity_breaks_the_transitivity_case`
    // above, extended to the properties group's own transitivity
    // cases for both isPartOf and hasPart.
    assert_caught(
        "no-transitive-parthood.ttl",
        "suites/sulo/properties/transitivity-ispartof.yaml",
    );
    assert_caught(
        "no-transitive-parthood.ttl",
        "suites/sulo/properties/transitivity-haspart.yaml",
    );
}

#[test]
fn deleting_the_parthood_containment_subproperty_axioms_also_breaks_the_properties_group_case() {
    // Same mutant as
    // `deleting_the_parthood_containment_subproperty_axioms_breaks_the_isin_case`
    // above. `subproperty-axioms.yaml` bundles four subPropertyOf
    // facts in one entails block; the isIn and contains conjuncts are
    // exactly what this mutant removes.
    assert_caught(
        "no-subproperty-containment.ttl",
        "suites/sulo/properties/subproperty-axioms.yaml",
    );
}

#[test]
fn deleting_features_subclassof_object_breaks_the_solid_typing_chain_and_cq() {
    // `no-feature-object.ttl` removes Feature's own, single, named-class
    // `rdfs:subClassOf sulo:Object` axiom (not a blank-node restriction,
    // and not redundant: nothing else in sulo.ttl re-derives Feature
    // subClassOf Object). This is the corrected diagnosis from Task 9:
    // its earlier report wrongly attributed typing-chain's dependence to
    // a concept-level (inverse-pair) mutant; the real dependency is this
    // one axiom.
    assert_caught(
        "no-feature-object.ttl",
        "suites/sulo/patterns/solid/typing-chain.yaml",
    );
    // The CQ case also depends on the same chain: its query joins
    // "a sulo:Object" on the measurement individual, entailed only
    // through this axiom (see the case's own description).
    assert_caught(
        "no-feature-object.ttl",
        "suites/sulo/patterns/solid/value-quality-unit.yaml",
    );
}

#[test]
fn deleting_both_haspart_only_self_restrictions_breaks_unit_forced_feature_and_two_propagation_cases()
 {
    // `no-selfpart-feature-and-informationobject.ttl` removes BOTH
    // `Feature rdfs:subClassOf (hasPart only Feature)` and
    // `InformationObject rdfs:subClassOf (hasPart only
    // InformationObject)`. Mutation-verified as needing both: the
    // measurement individual in unit-forced-feature's data is typed
    // (via the entailed chain) both Feature and InformationObject, so
    // either restriction alone still propagates Feature-hood onto the
    // unit. See suites/sulo/patterns/solid/unit-forced-feature.yaml's
    // own description for the same finding.
    assert_caught(
        "no-selfpart-feature-and-informationobject.ttl",
        "suites/sulo/patterns/solid/unit-forced-feature.yaml",
    );
    // The same two restrictions are, independently, exactly what
    // restrictions/hasPart-propagation-feature and
    // restrictions/hasPart-propagation-informationobject each test
    // directly, so this one mutant catches those two as well.
    assert_caught(
        "no-selfpart-feature-and-informationobject.ttl",
        "suites/sulo/restrictions/hasPart-propagation-feature.yaml",
    );
    assert_caught(
        "no-selfpart-feature-and-informationobject.ttl",
        "suites/sulo/restrictions/hasPart-propagation-informationobject.yaml",
    );
}

#[test]
fn deleting_processs_haspart_only_self_restriction_breaks_its_propagation_case() {
    // `no-selfpart-process.ttl` removes Process's own `rdfs:subClassOf
    // (hasPart only Process)`, its only rdfs:subClassOf member (so the
    // whole predicate-object pair is deleted, not just the blank
    // node). Unlike the Feature/InformationObject pair above, no
    // sibling class re-derives this for Process, so a single-axiom
    // mutant suffices here.
    assert_caught(
        "no-selfpart-process.ttl",
        "suites/sulo/restrictions/hasPart-propagation-process.yaml",
    );
}

#[test]
fn deleting_quantitys_haspart_some_unit_restriction_breaks_its_case() {
    // `no-quantity-unit-somevaluesfrom.ttl` removes Quantity's
    // `rdfs:subClassOf (hasPart some Unit)`, its only other
    // rdfs:subClassOf member besides the named class
    // sulo:InformationObject. This is the axiom
    // suites/sulo/restrictions/README.md documents as load-bearing on
    // its own (as opposed to TimeInterval's identically-shaped
    // restriction, found semantically inert by Task 9's mutation pass,
    // re-derived via TimeInterval subClassOf Time subClassOf
    // Quantity).
    assert_caught(
        "no-quantity-unit-somevaluesfrom.ttl",
        "suites/sulo/restrictions/quantity-haspart-some-unit.yaml",
    );
}

#[test]
fn deleting_hasparticipants_domain_and_isparticipantins_inverse_range_breaks_both_cases() {
    // `no-participant-domain-and-inverse-range.ttl` removes BOTH
    // hasParticipant's own `rdfs:domain sulo:Process` AND
    // isParticipantIn's own `rdfs:range sulo:Process`. A single-axiom
    // deletion here is inert, per domains-ranges/README.md:
    // ObjectPropertyDomain(hasParticipant, Process) is re-derivable
    // from ObjectPropertyRange(isParticipantIn, Process) plus
    // InverseObjectProperties(hasParticipant, isParticipantIn), by
    // standard OWL 2 DL model theory, so both halves of this one
    // shared fact must be removed together.
    assert_caught(
        "no-participant-domain-and-inverse-range.ttl",
        "suites/sulo/domains-ranges/hasparticipant.yaml",
    );
    // Verified to also catch isparticipantin.yaml: that case's own
    // entailment block requires the identical "?p a Process" fact,
    // reached from the opposite direction.
    assert_caught(
        "no-participant-domain-and-inverse-range.ttl",
        "suites/sulo/domains-ranges/isparticipantin.yaml",
    );
}

// ---------------------------------------------------------------
// Staleness guard (fix round 2, IMPORTANT 3): each mutant must equal
// CURRENT clean SULO with exactly its documented edit applied. These
// functions independently re-derive, in Rust, the same edits
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

fn expected_no_feature_object(sulo: &str) -> String {
    let needle = "        [ a owl:Restriction ;\n            owl:allValuesFrom sulo:Feature ;\n            \
                  owl:onProperty sulo:hasPart ],\n        sulo:Object ;";
    assert_eq!(
        sulo.matches(needle).count(),
        1,
        "Feature's hasPart-only-self / sulo:Object list tail not found exactly once in \
         current SULO; mutants/regenerate.sh and this staleness check both need updating"
    );
    let replacement = "        [ a owl:Restriction ;\n            owl:allValuesFrom sulo:Feature ;\n            \
                        owl:onProperty sulo:hasPart ] ;";
    sulo.replacen(needle, replacement, 1)
}

fn expected_no_selfpart_feature_and_informationobject(sulo: &str) -> String {
    let needle_feature = "        [ a owl:Restriction ;\n            owl:allValuesFrom sulo:Feature ;\n            \
                          owl:onProperty sulo:hasPart ],\n        sulo:Object ;";
    assert_eq!(
        sulo.matches(needle_feature).count(),
        1,
        "Feature's hasPart-only-self restriction not found exactly once in current SULO"
    );
    let out = sulo.replacen(needle_feature, "        sulo:Object ;", 1);

    let needle_io = "    rdfs:subClassOf [ a owl:Restriction ;\n            owl:allValuesFrom sulo:InformationObject ;\n            \
                     owl:onProperty sulo:hasPart ],\n        [ a owl:Restriction ;\n            owl:allValuesFrom rdfs:Literal ;\n            \
                     owl:onProperty sulo:hasValue ],\n        sulo:Feature .";
    assert_eq!(
        out.matches(needle_io).count(),
        1,
        "InformationObject's hasPart-only-self restriction not found exactly once in current SULO"
    );
    let replacement_io = "    rdfs:subClassOf [ a owl:Restriction ;\n            owl:allValuesFrom rdfs:Literal ;\n            \
                          owl:onProperty sulo:hasValue ],\n        sulo:Feature .";
    out.replacen(needle_io, replacement_io, 1)
}

fn expected_no_selfpart_process(sulo: &str) -> String {
    let needle = "    rdfs:subClassOf [ a owl:Restriction ;\n            owl:allValuesFrom sulo:Process ;\n            \
                  owl:onProperty sulo:hasPart ] ;\n";
    assert_eq!(
        sulo.matches(needle).count(),
        1,
        "Process's hasPart-only-self restriction not found exactly once in current SULO"
    );
    sulo.replacen(needle, "", 1)
}

fn expected_no_quantity_unit_somevaluesfrom(sulo: &str) -> String {
    let needle = "    rdfs:subClassOf [ a owl:Restriction ;\n            owl:onProperty sulo:hasPart ;\n            \
                  owl:someValuesFrom sulo:Unit ],\n        sulo:InformationObject .";
    assert_eq!(
        sulo.matches(needle).count(),
        1,
        "Quantity's hasPart-some-Unit restriction not found exactly once in current SULO"
    );
    sulo.replacen(needle, "    rdfs:subClassOf sulo:InformationObject .", 1)
}

fn expected_no_participant_domain_and_inverse_range(sulo: &str) -> String {
    let needle_domain = "    rdfs:domain sulo:Process ;\n    rdfs:range sulo:Object ;\n    \
                         owl:inverseOf sulo:isParticipantIn ;";
    assert_eq!(
        sulo.matches(needle_domain).count(),
        1,
        "hasParticipant's domain not found exactly once in current SULO"
    );
    let out = sulo.replacen(
        needle_domain,
        "    rdfs:range sulo:Object ;\n    owl:inverseOf sulo:isParticipantIn ;",
        1,
    );

    let needle_range = "    rdfs:domain sulo:Object ;\n    rdfs:range sulo:Process .";
    assert_eq!(
        out.matches(needle_range).count(),
        1,
        "isParticipantIn's range not found exactly once in current SULO"
    );
    out.replacen(needle_range, "    rdfs:domain sulo:Object .", 1)
}

fn expected_no_object_process_disjoint(sulo: &str) -> String {
    let needle = "    owl:disjointWith sulo:Process ;\n";
    assert_eq!(
        sulo.matches(needle).count(),
        1,
        "Object's owl:disjointWith sulo:Process line not found exactly once in current SULO; \
         mutants/regenerate.sh and this staleness check both need updating"
    );
    sulo.replacen(needle, "", 1)
}

/// (mutant file name, function that derives its expected content from
/// current SULO).
type StalenessCase = (&'static str, fn(&str) -> String);

#[test]
fn mutants_are_not_stale_against_current_sulo() {
    let sulo = std::fs::read_to_string(clean_sulo()).expect("real SULO should be readable");

    let cases: [StalenessCase; 10] = [
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
        ("no-feature-object.ttl", expected_no_feature_object),
        (
            "no-selfpart-feature-and-informationobject.ttl",
            expected_no_selfpart_feature_and_informationobject,
        ),
        ("no-selfpart-process.ttl", expected_no_selfpart_process),
        (
            "no-quantity-unit-somevaluesfrom.ttl",
            expected_no_quantity_unit_somevaluesfrom,
        ),
        (
            "no-participant-domain-and-inverse-range.ttl",
            expected_no_participant_domain_and_inverse_range,
        ),
        (
            "no-object-process-disjoint.ttl",
            expected_no_object_process_disjoint,
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
