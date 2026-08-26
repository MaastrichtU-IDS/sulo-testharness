//! Questions, probe encodings, and the comparison.
//!
//! Three layers, deliberately separate:
//!
//! 1. **Pinned probe text.** The exact Turtle each question shape
//!    builds, asserted character for character. A silent change to an
//!    encoding fails here first, with a diff a reader can check
//!    against the OWL 2 mapping by eye.
//! 2. **Name alignment.** The check names `differential::questions`
//!    looks rustdl's answers up under are the names
//!    `suite::run_case` actually records. This is the one piece of
//!    the design that could rot without anything else noticing:
//!    every lookup would miss, every question would carry
//!    `rustdl: None`, and the whole suite would report
//!    `Indeterminate`. Loud, but only if someone is watching, so this
//!    test watches.
//! 3. **Real controls, gated on `SULO_ROBOT_JAR`.** Every probe shape
//!    put to a real HermiT in BOTH directions against real SULO. This
//!    is the layer that matters most, because the way a probe fails is
//!    not "an error": a misparsed or vacuous probe comes back
//!    CONSISTENT, which reads as "not entailed", which is exactly what
//!    rustdl says, which is agreement. A probe suite that only ever
//!    asserted the CONSISTENT direction would be green with the
//!    encoding deleted.
//!
//! Layers 1 and 2 run everywhere. Layer 3 skips, by name, without a
//! jar; the gate itself is tested in `tests/hermit.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use sulo_testharness::differential::{
    Answer, Asked, Comparison, DifferentialOptions, DifferentialOutcome, Origin, Probe, Provenance,
    Question, QuestionKind, ask, cases_hermit_found_inconsistent, compare, differential_exit_code,
    explain_agreement, explain_divergence, no_questions_refusal, questions, run_differential,
};
use sulo_testharness::hermit::HermitAnswer;
use sulo_testharness::manifest::{Case, InstanceExpr, SubsumptionExpr};
use sulo_testharness::oracle::NO_PROOF_MARKER;
use sulo_testharness::suite::{GATE_EXPECT_CONSISTENT, run_case, select};
use sulo_testharness::verdict::{CheckOutcome, IndeterminateReason, Verdict};

const SULO: &str = "../sulo/sulo.ttl";
const JAR_VAR: &str = "SULO_ROBOT_JAR";

/// The jar, or `None` after saying why. The three-way resolution and
/// its own tests live in `tests/hermit.rs`; this is the same policy,
/// applied.
fn jar() -> Option<PathBuf> {
    match std::env::var_os(JAR_VAR) {
        None => {
            eprintln!(
                "SKIPPED: {JAR_VAR} is not set, so the probe encodings were not put to a \
                 real HermiT. Set it to a ROBOT 1.9.7 jar to run these controls."
            );
            None
        }
        Some(v) => {
            let path = PathBuf::from(v);
            assert!(
                path.is_file(),
                "{JAR_VAR} is set to {}, which is not a readable file. Refusing to skip: \
                 a control suite that silently ran nothing would report a confident green.",
                path.display()
            );
            Some(path)
        }
    }
}

fn sulo() -> &'static Path {
    let p = Path::new(SULO);
    assert!(p.is_file(), "{SULO} must exist for these tests");
    p
}

fn workdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sulo-testharness-differential-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A case asserting nothing, to be filled in per test. Built here
/// rather than read from YAML so each test's inputs are visible in the
/// test.
fn blank_case(id: &str) -> Case {
    Case {
        id: id.to_string(),
        description: "fixture".to_string(),
        ontology: None,
        imports: Vec::new(),
        data: Vec::new(),
        prefixes: BTreeMap::new(),
        expect_inconsistent: false,
        entails: None,
        not_entails: None,
        entails_manchester: Vec::new(),
        not_entails_manchester: Vec::new(),
        instance_of_expr: Vec::new(),
        satisfiable_expr: Vec::new(),
        unsatisfiable: Vec::new(),
        cq: Vec::new(),
        tags: Vec::new(),
        timeout_ms: 30_000,
        base_dir: PathBuf::from("."),
    }
}

fn ex_prefixes() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("ex".to_string(), "http://example.org/".to_string());
    m.insert(
        "obo".to_string(),
        "http://purl.obolibrary.org/obo/".to_string(),
    );
    m
}

/// The probe Turtle for the one question a case produces beyond the
/// gate.
fn only_probe(case: &Case) -> Probe {
    let mut qs = questions(case, sulo(), &[]);
    assert_eq!(
        qs.len(),
        2,
        "expected the gate question plus exactly one more, got {:?}",
        qs.iter()
            .map(|q| q.provenance.check.clone())
            .collect::<Vec<_>>()
    );
    qs.remove(1).probe
}

fn turtle(probe: &Probe) -> &str {
    match probe {
        Probe::Turtle(t) => t,
        other => panic!("expected a Turtle probe, got {other:?}"),
    }
}

const HEADER: &str = "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
                      @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
                      @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n";

// ---------------------------------------------------------------
// 1: the probe encodings, pinned as text.
// ---------------------------------------------------------------

/// The shape measured in both directions during design: a class
/// equivalent to `sub and not sup`, plus a witness of it.
#[test]
fn a_subsumption_claim_builds_the_measured_probe() {
    let mut case = blank_case("subsumption");
    case.not_entails = Some("sulo:Process rdfs:subClassOf sulo:Object .\n".to_string());

    assert_eq!(
        turtle(&only_probe(&case)),
        format!(
            "{HEADER}<https://w3id.org/sulo/Object> a owl:Class .\n\
             <https://w3id.org/sulo/Process> a owl:Class .\n\
             \n\
             <http://sulo-testharness.invalid/differential#probe> a owl:Class ;\n    \
             owl:equivalentClass [ a owl:Class ; owl:intersectionOf \
             ( <https://w3id.org/sulo/Process> [ a owl:Class ; owl:complementOf \
             <https://w3id.org/sulo/Object> ] ) ] .\n\
             \n\
             <http://sulo-testharness.invalid/differential#witness> a owl:NamedIndividual, \
             <http://sulo-testharness.invalid/differential#probe> .\n"
        )
    );
}

/// A Manchester subsumption, with a union on the right. Same shape,
/// with the union nested inside the complement rather than beside it:
/// `not (A or B)`, never `(not A) or (not B)`, which is a different
/// and much weaker question.
#[test]
fn a_manchester_subsumption_nests_the_union_inside_the_complement() {
    let mut case = blank_case("covering");
    case.not_entails_manchester = vec![SubsumptionExpr {
        sub_expr: "sulo:Object".to_string(),
        sup_expr: "sulo:SpatialObject or sulo:Feature".to_string(),
    }];

    assert_eq!(
        turtle(&only_probe(&case)),
        format!(
            "{HEADER}<https://w3id.org/sulo/Feature> a owl:Class .\n\
             <https://w3id.org/sulo/Object> a owl:Class .\n\
             <https://w3id.org/sulo/SpatialObject> a owl:Class .\n\
             \n\
             <http://sulo-testharness.invalid/differential#probe> a owl:Class ;\n    \
             owl:equivalentClass [ a owl:Class ; owl:intersectionOf \
             ( <https://w3id.org/sulo/Object> [ a owl:Class ; owl:complementOf \
             [ a owl:Class ; owl:unionOf ( <https://w3id.org/sulo/SpatialObject> \
             <https://w3id.org/sulo/Feature> ) ] ] ) ] .\n\
             \n\
             <http://sulo-testharness.invalid/differential#witness> a owl:NamedIndividual, \
             <http://sulo-testharness.invalid/differential#probe> .\n"
        )
    );
}

/// A class assertion is about ONE individual, so the named individual
/// is the witness. A fresh witness would ask whether the class is
/// empty, which is a different question with a different answer.
#[test]
fn a_class_assertion_makes_the_named_individual_the_witness() {
    let mut case = blank_case("class-assertion");
    case.prefixes = ex_prefixes();
    case.not_entails = Some("ex:unit a sulo:Unit .\n".to_string());

    assert_eq!(
        turtle(&only_probe(&case)),
        format!(
            "{HEADER}<https://w3id.org/sulo/Unit> a owl:Class .\n\
             <http://example.org/unit> a owl:NamedIndividual .\n\
             \n\
             <http://sulo-testharness.invalid/differential#probe> a owl:Class ;\n    \
             owl:equivalentClass [ a owl:Class ; owl:complementOf \
             <https://w3id.org/sulo/Unit> ] .\n\
             \n\
             <http://example.org/unit> a \
             <http://sulo-testharness.invalid/differential#probe> .\n"
        )
    );
}

#[test]
fn an_object_property_claim_builds_a_negative_property_assertion() {
    let mut case = blank_case("opa");
    case.prefixes = ex_prefixes();
    case.not_entails = Some("ex:alice sulo:hasParticipant ex:encounter .\n".to_string());

    assert_eq!(
        turtle(&only_probe(&case)),
        format!(
            "{HEADER}<https://w3id.org/sulo/hasParticipant> a owl:ObjectProperty .\n\
             <http://example.org/alice> a owl:NamedIndividual .\n\
             <http://example.org/encounter> a owl:NamedIndividual .\n\
             \n\
             [] a owl:NegativePropertyAssertion ;\n    \
             owl:sourceIndividual <http://example.org/alice> ;\n    \
             owl:assertionProperty <https://w3id.org/sulo/hasParticipant> ;\n    \
             owl:targetIndividual <http://example.org/encounter> .\n"
        )
    );
}

/// The data-property counterpart, which differs in exactly two places:
/// `owl:targetValue` and the datatype-carrying literal. The property
/// is declared `owl:DatatypeProperty`, not `owl:ObjectProperty`;
/// ROBOT parses the probe on its own, before any merge, so a wrong
/// declaration here is a probe about a different property.
#[test]
fn a_data_property_claim_builds_a_negative_assertion_with_a_target_value() {
    let mut case = blank_case("dpa");
    case.prefixes = ex_prefixes();
    case.not_entails = Some("ex:measurement sulo:hasValue \"171\"^^xsd:decimal .\n".to_string());

    assert_eq!(
        turtle(&only_probe(&case)),
        format!(
            "{HEADER}<https://w3id.org/sulo/hasValue> a owl:DatatypeProperty .\n\
             <http://example.org/measurement> a owl:NamedIndividual .\n\
             \n\
             [] a owl:NegativePropertyAssertion ;\n    \
             owl:sourceIndividual <http://example.org/measurement> ;\n    \
             owl:assertionProperty <https://w3id.org/sulo/hasValue> ;\n    \
             owl:targetValue \"171\"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n"
        )
    );
}

/// A language-tagged literal is written `"x"@fr`, never with an
/// `rdf:langString` datatype beside it: that is a Turtle syntax error,
/// and a probe ROBOT refuses is an Error, which is Indeterminate, not
/// a verdict.
#[test]
fn a_language_tagged_literal_is_written_with_its_tag_only() {
    let mut case = blank_case("lang");
    case.prefixes = ex_prefixes();
    case.not_entails = Some("ex:n sulo:hasValue \"bonjour\"@fr .\n".to_string());

    let probe = only_probe(&case);
    let text = turtle(&probe);
    assert!(
        text.contains("owl:targetValue \"bonjour\"@fr ."),
        "expected a plain language-tagged literal: {text}"
    );
    assert!(
        !text.contains("langString"),
        "a language tag and a datatype must not both be written: {text}"
    );
}

/// A quote or a backslash in a literal must not end the literal.
#[test]
fn a_literal_is_escaped() {
    let mut case = blank_case("escape");
    case.prefixes = ex_prefixes();
    case.not_entails = Some("ex:n sulo:hasValue \"a\\\"b\\\\c\\nd\"^^xsd:string .\n".to_string());

    let probe = only_probe(&case);
    let text = turtle(&probe);
    assert!(
        text.contains("owl:targetValue \"a\\\"b\\\\c\\nd\"^^"),
        "quotes, backslashes and newlines must be escaped: {text}"
    );
}

#[test]
fn a_satisfiability_question_asserts_a_witness_of_the_expression() {
    let mut case = blank_case("satisfiable");
    case.satisfiable_expr = vec!["sulo:Capability".to_string()];

    assert_eq!(
        turtle(&only_probe(&case)),
        format!(
            "{HEADER}<https://w3id.org/sulo/Capability> a owl:Class .\n\
             \n\
             <http://sulo-testharness.invalid/differential#probe> a owl:Class ;\n    \
             owl:equivalentClass <https://w3id.org/sulo/Capability> .\n\
             \n\
             <http://sulo-testharness.invalid/differential#witness> a owl:NamedIndividual, \
             <http://sulo-testharness.invalid/differential#probe> .\n"
        )
    );
}

/// Restrictions declare their property as an object property, and the
/// filler nests inside. Pinned because `owl:someValuesFrom` pointing
/// at the wrong node is the kind of mistake that produces a probe
/// which parses, merges, and can never clash.
#[test]
fn a_restriction_declares_its_property_and_nests_its_filler() {
    let mut case = blank_case("restriction");
    case.satisfiable_expr = vec!["sulo:Quantity and (sulo:hasPart some sulo:Unit)".to_string()];

    assert_eq!(
        turtle(&only_probe(&case)),
        format!(
            "{HEADER}<https://w3id.org/sulo/Quantity> a owl:Class .\n\
             <https://w3id.org/sulo/Unit> a owl:Class .\n\
             <https://w3id.org/sulo/hasPart> a owl:ObjectProperty .\n\
             \n\
             <http://sulo-testharness.invalid/differential#probe> a owl:Class ;\n    \
             owl:equivalentClass [ a owl:Class ; owl:intersectionOf \
             ( <https://w3id.org/sulo/Quantity> [ a owl:Restriction ; owl:onProperty \
             <https://w3id.org/sulo/hasPart> ; owl:someValuesFrom \
             <https://w3id.org/sulo/Unit> ] ) ] .\n\
             \n\
             <http://sulo-testharness.invalid/differential#witness> a owl:NamedIndividual, \
             <http://sulo-testharness.invalid/differential#probe> .\n"
        )
    );
}

/// A shape with no verified encoding is refused BY NAME, not
/// approximated and not dropped. Dropping it would remove the question
/// from the list, and a question nobody asked cannot diverge.
#[test]
fn an_unsupported_shape_is_unencodable_and_says_which() {
    let mut case = blank_case("unsupported");
    case.satisfiable_expr = vec!["sulo:hasValue some xsd:decimal".to_string()];

    match only_probe(&case) {
        Probe::Unencodable(reason) => {
            assert!(
                reason.contains("DataSomeValuesFrom"),
                "the reason must name the shape so it can be added: {reason}"
            );
        }
        other => panic!("a data range has no verified encoding, expected Unencodable: {other:?}"),
    }
}

/// A `not_entails:` fragment that parses to nothing is a question that
/// would otherwise vanish silently.
#[test]
fn an_empty_fragment_is_unencodable_rather_than_absent() {
    let mut case = blank_case("empty");
    case.not_entails = Some("# only a comment\n".to_string());

    match only_probe(&case) {
        Probe::Unencodable(reason) => assert!(reason.contains("zero claims"), "{reason}"),
        other => panic!("an empty fragment must be Unencodable, got {other:?}"),
    }
}

// ---------------------------------------------------------------
// 2: the questions a case produces, and rustdl's side of them.
// ---------------------------------------------------------------

/// The full question set for a case carrying every negative shape,
/// run against real SULO, with rustdl's answers read out of the very
/// `CaseResult` the suite produced.
///
/// Two things are pinned at once, and the second is the important
/// one:
///
/// * every question found a matching check, so the NAMES agree with
///   `suite::run_case`;
/// * every question therefore carries an answer. A drift in either
///   format string would leave `rustdl: None` everywhere, which
///   compares as `Indeterminate` forever: a differential that never
///   disagrees because it never compares.
#[test]
fn every_question_lines_up_with_the_check_the_suite_recorded() {
    let mut case = blank_case("alignment");
    case.base_dir = PathBuf::from("suites/sulo/patterns/solid");
    case.data = vec![PathBuf::from("data/measurement.ttl")];
    case.prefixes = ex_prefixes();
    case.not_entails = Some(
        "ex:unit a sulo:Unit .\n\
         ex:alice sulo:hasParticipant ex:measurement .\n\
         sulo:Process rdfs:subClassOf sulo:Object .\n\
         ex:measurement sulo:hasValue \"171\"^^xsd:decimal .\n"
            .to_string(),
    );
    case.not_entails_manchester = vec![SubsumptionExpr {
        sub_expr: "sulo:Object".to_string(),
        sup_expr: "sulo:SpatialObject or sulo:Feature".to_string(),
    }];
    case.satisfiable_expr = vec!["sulo:Capability".to_string()];

    let result = run_case(&case, sulo());
    let recorded: BTreeSet<&str> = result.checks.iter().map(|c| c.name.as_str()).collect();
    let qs = questions(&case, sulo(), &result.checks);

    assert_eq!(
        qs.len(),
        7,
        "one gate, four claims, one Manchester subsumption, one satisfiability"
    );

    for q in &qs {
        assert!(
            recorded.contains(q.provenance.check.as_str()),
            "the differential asks under the check name {:?}, which suite::run_case never \
             recorded. Recorded: {recorded:?}",
            q.provenance.check
        );
        assert!(
            q.rustdl.is_some(),
            "no rustdl answer for {:?}, so this question could only ever be \
             Indeterminate",
            q.provenance.check
        );
    }

    // The gate is consistent, and every negative is unrefuted, so the
    // answers themselves are known.
    let by_check: BTreeMap<&str, Option<Answer>> = qs
        .iter()
        .map(|q| (q.provenance.check.as_str(), q.rustdl))
        .collect();
    assert_eq!(
        by_check.get(GATE_EXPECT_CONSISTENT),
        Some(&Some(Answer::Consistent))
    );
    assert_eq!(
        by_check.get("sulo:Object subClassOf sulo:SpatialObject or sulo:Feature"),
        Some(&Some(Answer::NotEntailed))
    );
    assert_eq!(
        by_check.get("satisfiable: sulo:Capability"),
        Some(&Some(Answer::Satisfiable))
    );
}

/// A case that expects inconsistency asks the gate question under the
/// other name, and reads a PASS as "inconsistent". Getting this
/// backwards would report a divergence on every deliberate-clash case
/// in the suite.
#[test]
fn an_expect_inconsistent_case_reads_a_gate_pass_as_inconsistent() {
    let mut case = blank_case("clash");
    case.expect_inconsistent = true;
    case.ontology = Some(PathBuf::from("inconsistent.ttl"));
    case.base_dir = PathBuf::from("tests/fixtures");

    let result = run_case(&case, Path::new(""));
    assert_eq!(result.verdict, Verdict::Pass, "the fixture really clashes");

    let qs = questions(&case, Path::new(""), &result.checks);
    assert_eq!(qs.len(), 1, "an expect_inconsistent case is all gate");
    assert_eq!(qs[0].rustdl, Some(Answer::Inconsistent));
    assert_eq!(qs[0].kind, QuestionKind::Consistency);
    assert_eq!(qs[0].probe, Probe::None);
    assert_eq!(
        qs[0].ontology,
        PathBuf::from("tests/fixtures/inconsistent.ttl")
    );
}

/// An `Indeterminate` check gives rustdl no answer, and an answer
/// rustdl did not give must never be compared as though it had.
#[test]
fn an_indeterminate_check_leaves_rustdl_without_an_answer() {
    let mut case = blank_case("indeterminate");
    case.satisfiable_expr = vec!["sulo:Capability".to_string()];

    let checks = vec![CheckOutcome {
        name: "satisfiable: sulo:Capability".to_string(),
        verdict: Verdict::Indeterminate(IndeterminateReason::Timeout),
        rests_on_absence: false,
    }];
    let qs = questions(&case, sulo(), &checks);
    let q = qs
        .iter()
        .find(|q| q.provenance.check == "satisfiable: sulo:Capability")
        .expect("the satisfiability question exists");
    assert_eq!(q.rustdl, None);
}

// ---------------------------------------------------------------
// 3: the comparison.
// ---------------------------------------------------------------

fn question(kind: QuestionKind, rustdl: Option<Answer>) -> Question {
    Question {
        provenance: Provenance {
            case_id: "c".to_string(),
            check: "k".to_string(),
            asked: "q".to_string(),
            origin: Origin::Unrefuted,
        },
        kind,
        rustdl,
        ontology: PathBuf::from("o.ttl"),
        extra: Vec::new(),
        probe: Probe::None,
    }
}

#[test]
fn the_same_answer_twice_is_agreement() {
    let q = question(QuestionKind::Entailment, Some(Answer::NotEntailed));
    assert_eq!(
        compare(&q, &HermitAnswer::Consistent),
        Comparison::Agree {
            answer: Answer::NotEntailed
        }
    );
}

/// The outcome the whole differential exists to produce, carrying both
/// answers: the reader has to be able to work out WHICH reasoner is
/// wrong, and one answer alone never tells them.
#[test]
fn different_answers_are_a_divergence_carrying_both() {
    let q = question(QuestionKind::Entailment, Some(Answer::NotEntailed));
    match compare(&q, &HermitAnswer::Inconsistent) {
        Comparison::Divergence {
            question,
            rustdl,
            hermit,
        } => {
            assert_eq!(rustdl, Answer::NotEntailed);
            assert_eq!(hermit, Answer::Entailed);
            assert_eq!(question.check, "k");
            assert_eq!(question.case_id, "c");
        }
        other => panic!("expected a Divergence, got {other:?}"),
    }
}

/// Ruling 3, tested on its own because it is the single line whose
/// inversion would make the differential permanently green: the first
/// time a CI runner lost its jar, every question would "agree".
#[test]
fn a_robot_error_is_indeterminate_never_agreement() {
    for rustdl in [
        Answer::NotEntailed,
        Answer::Entailed,
        Answer::Consistent,
        Answer::Inconsistent,
    ] {
        let q = question(QuestionKind::Entailment, Some(rustdl));
        match compare(&q, &HermitAnswer::Error("robot fell over".to_string())) {
            Comparison::Indeterminate { question, reason } => {
                assert!(
                    reason.contains("robot fell over"),
                    "the reason ROBOT gave must survive: {reason}"
                );
                assert!(
                    reason.contains("nothing was cross-checked"),
                    "the message must say no cross-check happened: {reason}"
                );
                assert_eq!(question.check, "k");
            }
            other => panic!(
                "a ROBOT error must be Indeterminate whatever rustdl said ({rustdl}), got \
                 {other:?}"
            ),
        }
    }
}

/// The other half of the same rule, from the rustdl side.
#[test]
fn a_missing_rustdl_answer_is_indeterminate_never_agreement() {
    let q = question(QuestionKind::Entailment, None);
    match compare(&q, &HermitAnswer::Consistent) {
        Comparison::Indeterminate { reason, .. } => assert!(
            reason.contains("no comparable answer"),
            "the message must say which side is missing: {reason}"
        ),
        other => panic!("a missing rustdl answer must be Indeterminate, got {other:?}"),
    }
}

/// The three answer spaces read the same consistency result in
/// opposite directions. Pinned as a table because getting one row
/// backwards turns every question of that shape into a divergence, or
/// worse, into agreement on the wrong answer.
#[test]
fn hermits_one_primitive_is_read_per_question_kind() {
    let rows = [
        (
            QuestionKind::Consistency,
            HermitAnswer::Consistent,
            Answer::Consistent,
        ),
        (
            QuestionKind::Consistency,
            HermitAnswer::Inconsistent,
            Answer::Inconsistent,
        ),
        (
            QuestionKind::Entailment,
            HermitAnswer::Consistent,
            Answer::NotEntailed,
        ),
        (
            QuestionKind::Entailment,
            HermitAnswer::Inconsistent,
            Answer::Entailed,
        ),
        (
            QuestionKind::Satisfiability,
            HermitAnswer::Consistent,
            Answer::Satisfiable,
        ),
        (
            QuestionKind::Satisfiability,
            HermitAnswer::Inconsistent,
            Answer::Unsatisfiable,
        ),
    ];
    for (kind, hermit, expected) in rows {
        let q = question(kind, Some(expected));
        assert_eq!(
            compare(&q, &hermit),
            Comparison::Agree { answer: expected },
            "{kind:?} must read {hermit:?} as {expected}"
        );
    }
}

/// An unencodable question never reaches ROBOT, and comes back as an
/// error rather than as a quiet nothing.
#[test]
fn an_unencodable_question_is_never_asked_and_never_agrees() {
    let mut q = question(QuestionKind::Entailment, Some(Answer::NotEntailed));
    q.probe = Probe::Unencodable("no verified encoding".to_string());

    let answer = ask(
        Path::new("/nonexistent/robot.jar"),
        &q,
        &workdir("unencodable"),
    );
    match &answer {
        HermitAnswer::Error(msg) => assert!(msg.contains("never asked"), "{msg}"),
        other => panic!("an unencodable question must not be asked, got {other:?}"),
    }
    assert!(
        matches!(compare(&q, &answer), Comparison::Indeterminate { .. }),
        "and it must compare as Indeterminate"
    );
}

// ---------------------------------------------------------------
// 4: real controls. Every encoding, both directions, real HermiT.
// ---------------------------------------------------------------

/// Ask HermiT every question this case produces, keyed by check name.
fn hermit_answers(robot: &Path, case: &Case) -> BTreeMap<String, HermitAnswer> {
    let qs = questions(case, sulo(), &[]);
    let mut out = BTreeMap::new();
    for (i, q) in qs.iter().enumerate() {
        let dir = workdir(&format!("{}-{i}", case.id));
        out.insert(q.provenance.check.clone(), ask(robot, q, &dir));
    }
    out
}

/// One control: a case with a single negative claim, and the answer
/// HermiT must give for it.
fn control(robot: &Path, id: &str, fragment: &str, data: Option<&str>, expected: &HermitAnswer) {
    let mut case = blank_case(id);
    case.prefixes = ex_prefixes();
    case.not_entails = Some(fragment.to_string());
    if let Some(d) = data {
        case.base_dir = PathBuf::from(d).parent().unwrap().to_path_buf();
        case.data = vec![PathBuf::from(
            PathBuf::from(d)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        )];
    }
    let answers = hermit_answers(robot, &case);
    let (check, got) = answers
        .iter()
        .find(|(k, _)| k.as_str() != GATE_EXPECT_CONSISTENT)
        .expect("the case has one non-gate question");
    assert_eq!(got, expected, "control {id} ({check})");
}

const ENCOUNTER: &str = "suites/sulo/patterns/pro/data/encounter.ttl";
const MEASUREMENT: &str = "suites/sulo/patterns/solid/data/measurement.ttl";

/// Subsumption, the direction that must CLASH. Without this row, the
/// encoding could be deleted entirely and the "not entailed" rows
/// below would all still pass.
#[test]
fn control_subsumption_entailed() {
    let Some(robot) = jar() else { return };
    control(
        &robot,
        "sub-entailed",
        "sulo:Feature rdfs:subClassOf sulo:Object .\n",
        None,
        &HermitAnswer::Inconsistent,
    );
}

#[test]
fn control_subsumption_not_entailed() {
    let Some(robot) = jar() else { return };
    control(
        &robot,
        "sub-not-entailed",
        "sulo:Process rdfs:subClassOf sulo:Object .\n",
        None,
        &HermitAnswer::Consistent,
    );
}

#[test]
fn control_class_assertion_entailed() {
    let Some(robot) = jar() else { return };
    control(
        &robot,
        "ca-entailed",
        "ex:unit a sulo:Feature .\n",
        Some(MEASUREMENT),
        &HermitAnswer::Inconsistent,
    );
}

#[test]
fn control_class_assertion_not_entailed() {
    let Some(robot) = jar() else { return };
    control(
        &robot,
        "ca-not-entailed",
        "ex:unit a sulo:Unit .\n",
        Some(MEASUREMENT),
        &HermitAnswer::Consistent,
    );
}

/// The PRO role chain really does entail this, so the negative
/// property assertion must clash.
#[test]
fn control_object_property_entailed() {
    let Some(robot) = jar() else { return };
    control(
        &robot,
        "opa-entailed",
        "ex:encounter sulo:hasParticipant ex:alice .\n",
        Some(ENCOUNTER),
        &HermitAnswer::Inconsistent,
    );
}

/// The chain does not run backwards, which is what the real suite
/// case `chain-not-backward` asserts.
#[test]
fn control_object_property_not_entailed() {
    let Some(robot) = jar() else { return };
    control(
        &robot,
        "opa-not-entailed",
        "ex:alice sulo:hasParticipant ex:encounter .\n",
        Some(ENCOUNTER),
        &HermitAnswer::Consistent,
    );
}

#[test]
fn control_data_property_entailed() {
    let Some(robot) = jar() else { return };
    control(
        &robot,
        "dpa-entailed",
        "ex:measurement sulo:hasValue \"170\"^^xsd:decimal .\n",
        Some(MEASUREMENT),
        &HermitAnswer::Inconsistent,
    );
}

#[test]
fn control_data_property_not_entailed() {
    let Some(robot) = jar() else { return };
    control(
        &robot,
        "dpa-not-entailed",
        "ex:measurement sulo:hasValue \"171\"^^xsd:decimal .\n",
        Some(MEASUREMENT),
        &HermitAnswer::Consistent,
    );
}

/// The Manchester shapes, in one case so they share the JVM starts:
/// each entry is an expression and whether a witness of it clashes.
#[test]
fn control_expression_shapes() {
    let Some(robot) = jar() else { return };

    let rows: &[(&str, HermitAnswer)] = &[
        // A plain named class.
        ("sulo:Capability", HermitAnswer::Consistent),
        // Intersection, of two classes SULO declares disjoint.
        ("sulo:Object and sulo:Process", HermitAnswer::Inconsistent),
        // Union, inside a complement: Feature IS covered by its four
        // subclasses, so a Feature outside all four cannot exist.
        (
            "sulo:Feature and not (sulo:Capability or sulo:InformationObject or sulo:Quality or sulo:Role)",
            HermitAnswer::Inconsistent,
        ),
        // The same shape one class short of the covering axiom: now
        // satisfiable, which is what makes the row above evidence
        // about the union rather than about the complement.
        (
            "sulo:Feature and not (sulo:Capability or sulo:InformationObject or sulo:Quality)",
            HermitAnswer::Consistent,
        ),
        // someValuesFrom, satisfiable.
        (
            "sulo:Quantity and (sulo:hasPart some sulo:Unit)",
            HermitAnswer::Consistent,
        ),
        // someValuesFrom against Quantity's own restriction: every
        // Quantity has a Unit part, so forbidding one clashes.
        (
            "sulo:Quantity and not (sulo:hasPart some sulo:Unit)",
            HermitAnswer::Inconsistent,
        ),
        // allValuesFrom: Feature propagates Feature-hood over
        // hasPart, so a Feature with a non-Feature part cannot exist.
        (
            "sulo:Feature and (sulo:hasPart some (not sulo:Feature))",
            HermitAnswer::Inconsistent,
        ),
        // Qualified cardinality, both directions.
        (
            "sulo:Quantity and (sulo:hasPart min 1 sulo:Unit)",
            HermitAnswer::Consistent,
        ),
        (
            "sulo:Quantity and (sulo:hasPart max 0 sulo:Unit)",
            HermitAnswer::Inconsistent,
        ),
        (
            "sulo:Quantity and (sulo:hasPart exactly 0 sulo:Unit)",
            HermitAnswer::Inconsistent,
        ),
        // An inverse property expression: `inverse hasFeature` is
        // isFeatureOf, so a Feature of nothing at all clashes with
        // Feature's own isFeatureOf restriction.
        (
            "sulo:Feature and not (inverse sulo:hasFeature some (sulo:Object or sulo:Process))",
            HermitAnswer::Inconsistent,
        ),
        (
            "sulo:Feature and (inverse sulo:hasFeature some sulo:Object)",
            HermitAnswer::Consistent,
        ),
    ];

    let mut case = blank_case("shapes");
    case.satisfiable_expr = rows.iter().map(|(e, _)| (*e).to_string()).collect();
    let answers = hermit_answers(&robot, &case);

    for (expr, expected) in rows {
        let key = format!("satisfiable: {expr}");
        assert_eq!(
            answers.get(&key),
            Some(expected),
            "witness of {expr}: HermiT must answer {expected:?}. A CONSISTENT where \
             INCONSISTENT is expected is the signature of a probe that says less than it \
             looks like it says"
        );
    }
}

/// An individual named in an `oneOf` or a `hasValue` has to be the
/// individual the data talks about, which is what the declaration and
/// the IRI together buy.
#[test]
fn control_individual_shapes() {
    let Some(robot) = jar() else { return };

    let rows: &[(&str, HermitAnswer)] = &[
        // hasValue: the measurement really does have ex:unit as a
        // part, and ex:unit is entailed Feature, so a hasPart value
        // that is NOT a Feature clashes.
        (
            "(sulo:hasPart value ex:unit) and (sulo:hasPart only (not sulo:Feature))",
            HermitAnswer::Inconsistent,
        ),
        (
            "sulo:Quantity and (sulo:hasPart value ex:unit)",
            HermitAnswer::Consistent,
        ),
        // oneOf: alice is an Object in this data, so {alice} that is
        // not an Object clashes.
        (
            "{ ex:alice } and not sulo:Object",
            HermitAnswer::Inconsistent,
        ),
        ("{ ex:alice } and sulo:Object", HermitAnswer::Consistent),
    ];

    let mut case = blank_case("individuals");
    case.prefixes = ex_prefixes();
    case.base_dir = PathBuf::from("suites/sulo/patterns/solid");
    case.data = vec![PathBuf::from("data/measurement.ttl")];
    case.satisfiable_expr = rows.iter().map(|(e, _)| (*e).to_string()).collect();
    let answers = hermit_answers(&robot, &case);

    for (expr, expected) in rows {
        let key = format!("satisfiable: {expr}");
        assert_eq!(
            answers.get(&key),
            Some(expected),
            "witness of {expr}: HermiT must answer {expected:?}"
        );
    }
}

/// End to end, on the case the differential was built for: rustdl
/// says the ontology plus this data is CONSISTENT (it cannot see the
/// data-range axiom at all), HermiT says INCONSISTENT, and the
/// comparison is a Divergence naming both.
///
/// This is the proof that the machinery can report disagreement.
/// Everything else here could pass with `compare` hard-wired to
/// `Agree`.
#[test]
fn the_data_range_case_diverges() {
    let Some(robot) = jar() else { return };

    let mut case = blank_case("timeinstant-datarange");
    case.base_dir = PathBuf::from("suites/sulo/restrictions");
    case.data = vec![PathBuf::from("data/timeinstant-datarange.ttl")];
    case.prefixes = ex_prefixes();
    case.expect_inconsistent = true;

    let result = run_case(&case, sulo());
    let qs = questions(&case, sulo(), &result.checks);
    assert_eq!(qs.len(), 1, "the gate is the whole question here");

    assert_eq!(
        qs[0].rustdl,
        Some(Answer::Consistent),
        "rustdl cannot enforce the data-range allValuesFrom, so it finds no clash"
    );

    let hermit = ask(&robot, &qs[0], &workdir("datarange"));
    assert_eq!(hermit, HermitAnswer::Inconsistent);

    match compare(&qs[0], &hermit) {
        Comparison::Divergence { rustdl, hermit, .. } => {
            assert_eq!(rustdl, Answer::Consistent);
            assert_eq!(hermit, Answer::Inconsistent);
        }
        other => panic!("the two reasoners really do disagree here, got {other:?}"),
    }
}

// ---------------------------------------------------------------
// 5: ruling 7. The positive assertions rustdl could not prove.
// ---------------------------------------------------------------
//
// `oracle::verdict_for` tells the reader, on a positive assertion
// that found no proof, "Incompleteness is a possible cause; the CI
// differential settles it". That Fail rests on an absence of proof
// exactly as a negative `UnrefutedPass` does. If the differential
// never asks about it, that sentence ships as a falsehood, so these
// tests are what keep `oracle.rs`'s promise honest.

/// A positive assertion rustdl PROVED needs no oracle of record, and
/// must not produce a question.
///
/// Half of ruling 7's membership rule, and the half that would go
/// unnoticed if it broke: asking HermiT to confirm every proof rustdl
/// already found would multiply the CI job's runtime by the size of
/// the suite while establishing nothing soundness had not already
/// established.
#[test]
fn a_passing_positive_assertion_yields_no_question() {
    let mut case = blank_case("passing-positive");
    case.entails_manchester = vec![SubsumptionExpr {
        // Asserted in SULO, so rustdl proves it.
        sub_expr: "sulo:TimeInterval".to_string(),
        sup_expr: "sulo:Time".to_string(),
    }];

    let result = run_case(&case, sulo());
    let check = "sulo:TimeInterval subClassOf sulo:Time";
    assert!(
        result
            .checks
            .iter()
            .any(|c| c.name == check && c.verdict == Verdict::Pass),
        "the fixture must really PASS, or this test proves nothing: {:?}",
        result.checks
    );

    let qs = questions(&case, sulo(), &result.checks);
    assert_eq!(
        qs.len(),
        1,
        "a proved positive needs no oracle of record, so the gate is the only question: \
         {:?}",
        qs.iter().map(|q| &q.provenance.check).collect::<Vec<_>>()
    );
    assert_eq!(qs[0].provenance.origin, Origin::Gate);
}

/// Every positive shape that can produce a `NO_PROOF_MARKER` Fail
/// produces a question, under the same check name the `run` report
/// used, carrying rustdl's absence-of-proof answer.
///
/// All four shapes in one case, because the failure mode is per-shape:
/// a name format that drifts, or a loop someone forgets to add,
/// silently drops that shape from the differential and leaves its
/// `Fail` message promising a cross-check that never happens.
#[test]
fn every_failing_positive_shape_becomes_a_question() {
    let mut case = blank_case("failing-positives");
    case.base_dir = PathBuf::from("suites/sulo/patterns/solid");
    case.data = vec![PathBuf::from("data/measurement.ttl")];
    case.prefixes = ex_prefixes();
    // None of these four holds in SULO, so each is a Fail resting on
    // an absence of proof.
    case.entails = Some("sulo:Object rdfs:subClassOf sulo:Process .\n".to_string());
    case.entails_manchester = vec![SubsumptionExpr {
        sub_expr: "sulo:Object".to_string(),
        sup_expr: "sulo:Feature".to_string(),
    }];
    case.instance_of_expr = vec![InstanceExpr {
        individual: "ex:alice".to_string(),
        expr: "sulo:Process".to_string(),
    }];
    case.unsatisfiable = vec!["sulo:Object".to_string()];

    let result = run_case(&case, sulo());

    // The premise: every one of the four really is a
    // NO_PROOF_MARKER Fail in the `run` report. Without this the
    // assertions below could pass over a case that failed for some
    // other reason entirely.
    let unproven: BTreeSet<&str> = result
        .checks
        .iter()
        .filter(|c| matches!(&c.verdict, Verdict::Fail(m) if m.contains(NO_PROOF_MARKER)))
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(
        unproven.len(),
        4,
        "the fixture must produce exactly four absence-resting Fails, or ruling 7 is \
         being tested against the wrong thing: {:?}",
        result.checks
    );

    let qs = questions(&case, sulo(), &result.checks);
    let positives: BTreeMap<&str, &Question> = qs
        .iter()
        .filter(|q| q.provenance.origin == Origin::FailingPositive)
        .map(|q| (q.provenance.check.as_str(), q))
        .collect();

    assert_eq!(
        positives.keys().copied().collect::<BTreeSet<_>>(),
        unproven,
        "every positive assertion rustdl could not prove must become a question, under \
         the same check name the run report used"
    );

    for (check, q) in &positives {
        assert_eq!(
            q.rustdl,
            Some(Answer::NotEntailed),
            "rustdl's answer for {check} is an absence of proof, and must be recorded as \
             one: comparing it as anything else would manufacture agreement"
        );
        assert_eq!(q.kind, QuestionKind::Entailment);
        assert!(
            !matches!(q.probe, Probe::None),
            "{check} must carry a probe: a Probe::None here would ask HermiT a plain \
             consistency question and compare its answer against an entailment"
        );
    }
}

/// Ruling 7's membership rule is `NO_PROOF_MARKER`, not "any Fail",
/// and not "any check with this name".
///
/// Driven with a synthetic `CheckOutcome` per verdict shape rather
/// than with real SULO, because real SULO cannot reach four of the
/// five rows: every one of the four positive shapes goes through
/// `oracle::verdict_for` with `Expectation::Entailed`, whose only Fail
/// carries the marker. So the discriminating cases have to be
/// constructed.
///
/// That is not an argument for dropping the marker test from
/// `unproven`. It is an argument for having THIS test: the first
/// version of the test below used a `not_entails_manchester` entry and
/// a real run, and widening `unproven` to `Verdict::Fail(_)` left it
/// green, because the ruling 7 loops never look at
/// `not_entails_manchester` at all. A check that could not fail,
/// caught by mutating the code rather than by reading it.
#[test]
fn only_a_fail_resting_on_absence_of_proof_becomes_a_question() {
    let mut case = blank_case("membership");
    case.entails_manchester = vec![SubsumptionExpr {
        sub_expr: "sulo:Object".to_string(),
        sup_expr: "sulo:Process".to_string(),
    }];
    let check = "sulo:Object subClassOf sulo:Process";

    let rows: [(&str, Verdict, bool); 5] = [
        (
            "a Fail resting on an absence of proof: the whole point of ruling 7",
            Verdict::Fail(format!("expected to hold, but {NO_PROOF_MARKER}: {check}.")),
            true,
        ),
        (
            "a Fail resting on a clash the reasoner EXHIBITED, which soundness vouches \
             for and which needs no oracle of record",
            Verdict::Fail(format!("expected NOT to hold, but it does: {check}")),
            false,
        ),
        (
            "a Pass is a proof; asking HermiT to confirm a proof is not what the oracle \
             of record is for",
            Verdict::Pass,
            false,
        ),
        (
            "an Indeterminate is an answer rustdl never gave, and must never be \
             compared as though it had",
            Verdict::Indeterminate(IndeterminateReason::Timeout),
            false,
        ),
        (
            "an UnrefutedPass cannot arise for a positive assertion, and if it somehow \
             does it is not a Fail to reinterpret",
            Verdict::UnrefutedPass,
            false,
        ),
    ];

    for (why, verdict, expected) in rows {
        let checks = vec![CheckOutcome {
            name: check.to_string(),
            verdict,
            rests_on_absence: false,
        }];
        let got = questions(&case, sulo(), &checks)
            .iter()
            .any(|q| q.provenance.origin == Origin::FailingPositive);
        assert_eq!(got, expected, "{why}");
    }

    // And a check nobody recorded under that name is not a question
    // either: a name drift must leave the differential silent here
    // rather than inventing an answer rustdl never gave.
    let got = questions(&case, sulo(), &[])
        .iter()
        .any(|q| q.provenance.origin == Origin::FailingPositive);
    assert!(
        !got,
        "no recorded check means no rustdl verdict to reinterpret"
    );
}

/// The loops are over the POSITIVE fields. A negative assertion stays
/// a negative assertion, whatever its verdict.
#[test]
fn a_refuted_negative_is_not_a_failing_positive() {
    let mut case = blank_case("refuted-negative");
    case.not_entails_manchester = vec![SubsumptionExpr {
        // SULO DOES assert this, so the negative expectation is
        // refuted: a trustworthy Fail, no marker.
        sub_expr: "sulo:TimeInterval".to_string(),
        sup_expr: "sulo:Time".to_string(),
    }];

    let result = run_case(&case, sulo());
    let check = "sulo:TimeInterval subClassOf sulo:Time";
    let verdict = &result
        .checks
        .iter()
        .find(|c| c.name == check)
        .expect("the check ran")
        .verdict;
    match verdict {
        Verdict::Fail(msg) => assert!(
            !msg.contains(NO_PROOF_MARKER),
            "this Fail must rest on a proof, not on its absence: {msg}"
        ),
        other => panic!("the fixture must really Fail, got {other:?}"),
    }

    let qs = questions(&case, sulo(), &result.checks);
    let q = qs
        .iter()
        .find(|q| q.provenance.check == check)
        .expect("the negative assertion is still asked");
    assert_eq!(
        q.provenance.origin,
        Origin::Unrefuted,
        "this question comes from `not_entails_manchester`, not from ruling 7's positive \
         half"
    );
    assert_eq!(
        qs.iter()
            .filter(|q| q.provenance.origin == Origin::FailingPositive)
            .count(),
        0,
        "a refuted negative rests on a clash the reasoner found, so it is not a failing \
         positive"
    );
}

/// The probe a ruling 7 question carries is BYTE FOR BYTE the probe
/// the same claim would get as a negative assertion.
///
/// The two paths ask HermiT the same question and differ only in what
/// the answer MEANS to the reader, so they must not differ in the
/// probe. Pinned as an identity rather than as another copy of the
/// expected Turtle, because the encodings are already pinned
/// character for character in section 1: what could still go wrong
/// here is the positive path building a probe with `sub` and `sup` the
/// wrong way round, which would be a probe that answers a question
/// nobody asked and which no real-HermiT control in this file would
/// catch (against the fixture where HermiT proves the entailment, the
/// ontology is inconsistent, so ANY probe comes back INCONSISTENT).
#[test]
fn a_failing_positive_carries_the_same_probe_as_the_negative_of_the_same_claim() {
    let sub = "sulo:Object";
    let sup = "sulo:Process";
    let check = format!("{sub} subClassOf {sup}");

    let mut positive = blank_case("probe-identity-positive");
    positive.entails_manchester = vec![SubsumptionExpr {
        sub_expr: sub.to_string(),
        sup_expr: sup.to_string(),
    }];
    let checks = vec![CheckOutcome {
        name: check.clone(),
        verdict: Verdict::Fail(format!("expected to hold, but {NO_PROOF_MARKER}: {check}.")),
        rests_on_absence: false,
    }];

    let mut negative = blank_case("probe-identity-negative");
    negative.not_entails_manchester = vec![SubsumptionExpr {
        sub_expr: sub.to_string(),
        sup_expr: sup.to_string(),
    }];

    let from_positive = questions(&positive, sulo(), &checks);
    let from_positive = from_positive
        .iter()
        .find(|q| q.provenance.origin == Origin::FailingPositive)
        .expect("ruling 7 produces a question here");
    let from_negative = questions(&negative, sulo(), &[]);
    let from_negative = from_negative
        .iter()
        .find(|q| q.provenance.origin == Origin::Unrefuted)
        .expect("the negative assertion is asked");

    assert_eq!(
        turtle(&from_positive.probe),
        turtle(&from_negative.probe),
        "the two paths must put the SAME question to HermiT; only its meaning to the \
         reader differs"
    );
    assert_eq!(from_positive.kind, from_negative.kind);
}

/// Ruling 7, direction one, against a real HermiT: HermiT finds no
/// proof either, so the two reasoners AGREE and the `Fail` the `run`
/// subcommand reports is a genuine regression in the ontology.
///
/// `sulo:Object subClassOf sulo:Process` is not entailed by SULO (the
/// two are disjoint and `Object` is satisfiable), so neither reasoner
/// can prove it. The report has to SAY that, because `oracle`'s Fail
/// message explicitly defers the question here.
#[test]
fn a_failing_positive_both_reasoners_cannot_prove_agrees() {
    let Some(robot) = jar() else { return };

    let mut case = blank_case("failing-positive-agree");
    case.entails_manchester = vec![SubsumptionExpr {
        sub_expr: "sulo:Object".to_string(),
        sup_expr: "sulo:Process".to_string(),
    }];

    let result = run_case(&case, sulo());
    let qs = questions(&case, sulo(), &result.checks);
    let q = qs
        .iter()
        .find(|q| q.provenance.origin == Origin::FailingPositive)
        .expect("ruling 7 must produce a question for this Fail");

    let hermit = ask(&robot, q, &workdir("positive-agree"));
    assert_eq!(
        hermit,
        HermitAnswer::Consistent,
        "the probe `Object and not Process` must have a model: an INCONSISTENT here \
         would mean the probe is asking something else"
    );

    match compare(q, &hermit) {
        Comparison::Agree { answer } => {
            assert_eq!(answer, Answer::NotEntailed);
            let note = explain_agreement(Origin::FailingPositive, answer)
                .expect("an agreement on a failing positive must be explained");
            assert!(
                note.contains("genuine regression"),
                "the note must tell the reader the Fail is real: {note}"
            );
        }
        other => panic!("both reasoners fail to prove this, so they agree, got {other:?}"),
    }
}

/// Ruling 7, direction two, and the signal spec 5.3 calls the most
/// valuable either reasoner could produce: HermiT DOES find the proof
/// rustdl could not, so the `Fail` the `run` subcommand reports is a
/// rustdl INCOMPLETENESS rather than a SULO regression.
///
/// Built on the one gap that really exists: with
/// `timeinstant-datarange.ttl` merged in, the ontology is inconsistent
/// (a `TimeInstant` with an `xsd:string` value violates the data-range
/// `allValuesFrom`), so HermiT entails everything, including
/// `Object subClassOf Process`. rustdl cannot see that axiom at all,
/// reports the ontology consistent, and then fails to prove the
/// subsumption.
///
/// Without this test the positive half of the question set could be
/// wired to a probe that can never clash, and every failing positive
/// in the suite would come back "agreed", which is the exact defect
/// shape this project keeps finding.
#[test]
fn a_failing_positive_hermit_can_prove_diverges_and_names_rustdl() {
    let Some(robot) = jar() else { return };

    let mut case = blank_case("failing-positive-diverge");
    case.base_dir = PathBuf::from("suites/sulo/restrictions");
    case.data = vec![PathBuf::from("data/timeinstant-datarange.ttl")];
    case.prefixes = ex_prefixes();
    case.entails_manchester = vec![SubsumptionExpr {
        sub_expr: "sulo:Object".to_string(),
        sup_expr: "sulo:Process".to_string(),
    }];

    let result = run_case(&case, sulo());
    let qs = questions(&case, sulo(), &result.checks);
    let q = qs
        .iter()
        .find(|q| q.provenance.origin == Origin::FailingPositive)
        .expect("ruling 7 must produce a question for this Fail");
    assert_eq!(
        q.rustdl,
        Some(Answer::NotEntailed),
        "rustdl found no proof, which is the whole premise"
    );

    let hermit = ask(&robot, q, &workdir("positive-diverge"));
    assert_eq!(
        hermit,
        HermitAnswer::Inconsistent,
        "HermiT sees the data-range violation, so it entails everything"
    );

    match compare(q, &hermit) {
        Comparison::Divergence {
            question,
            rustdl,
            hermit,
        } => {
            assert_eq!(rustdl, Answer::NotEntailed);
            assert_eq!(hermit, Answer::Entailed);
            let note = explain_divergence(question.origin, rustdl, hermit, false);
            assert!(
                note.contains("rustdl is the outlier") && note.contains("INCOMPLETENESS"),
                "the note must name the outlier and the defect: {note}"
            );
        }
        other => panic!(
            "HermiT proves what rustdl cannot, which is a Divergence, got {other:?}. An \
             Agree here would mean the probe cannot clash"
        ),
    }

    // ...but this case's own gate is where the proof came from, and
    // the whole run knows that. HermiT answers the gate INCONSISTENT,
    // so the `false` passed above is NOT what the report passes, and
    // the wording the report actually prints must not read the vacuous
    // proof as news about rustdl or about the assertion. This is the
    // wiring, run end to end on the real jar.
    let asked: Vec<Asked> = qs
        .iter()
        .map(|q| Asked {
            provenance: q.provenance.clone(),
            comparison: compare(q, &ask(&robot, q, &workdir("positive-diverge-all"))),
        })
        .collect();
    let vacuous = cases_hermit_found_inconsistent(&asked);
    assert!(
        vacuous.contains("failing-positive-diverge"),
        "HermiT answers this case's gate INCONSISTENT while rustdl does not, so the case \
         must be recognised as one where HermiT's proofs are vacuous: {vacuous:?}"
    );

    let note = explain_divergence(
        Origin::FailingPositive,
        Answer::NotEntailed,
        Answer::Entailed,
        true,
    );
    assert!(
        !note.contains("AGAINST a SULO regression"),
        "a vacuous proof settles nothing, least of all against a regression: {note}"
    );
    assert!(
        !note.contains("not a finding about the ontology"),
        "HermiT found this case's ontology-plus-data inconsistent, which is precisely a \
         finding about the ontology: {note}"
    );
    assert!(
        note.contains("VACUOUS") && note.contains("gate divergence is the finding to read first"),
        "the note must name the vacuity and point at the gate: {note}"
    );
}

/// The ORDINARY failing-positive divergence, in a case whose ontology
/// still has a model: HermiT found a proof rustdl missed, and there is
/// nothing vacuous about it. The `Fail` the `run` subcommand reports IS
/// settled, and settled against a SULO regression.
///
/// This is the branch the vacuity flag must not swallow. Without it the
/// fix for the inconsistent-case wording could have been "delete the
/// sentence", which would lose the signal spec 5.3 calls the most
/// valuable either reasoner could produce.
#[test]
fn an_ordinary_failing_positive_divergence_still_settles_the_fail() {
    let note = explain_divergence(
        Origin::FailingPositive,
        Answer::NotEntailed,
        Answer::Entailed,
        false,
    );
    assert!(
        note.contains("AGAINST a SULO regression"),
        "with a consistent case, HermiT's proof is real and settles the Fail: {note}"
    );
    assert!(
        note.contains("not a finding about the ontology"),
        "with a consistent case, the divergence really is about rustdl: {note}"
    );
    assert!(
        !note.contains("VACUOUS"),
        "nothing is vacuous in a case that has a model: {note}"
    );
}

/// The gate divergence in an inconsistent case, which is the most
/// severe finding this harness can produce and used to be reported as
/// "not a finding about the ontology".
#[test]
fn an_inconsistent_gate_divergence_is_a_finding_about_the_ontology() {
    let note = explain_divergence(Origin::Gate, Answer::Consistent, Answer::Inconsistent, true);
    assert!(
        !note.contains("not a finding about the ontology"),
        "HermiT found the ontology plus this case's data to have no model; that is a \
         finding about the ontology: {note}"
    );
    assert!(
        note.contains("IS a finding about the ontology") && note.contains("NO MODEL"),
        "the note must say what HermiT actually found: {note}"
    );
    assert!(
        note.contains("Read this divergence before any other in the case"),
        "the gate divergence is the one to read first: {note}"
    );
}

/// An `UnrefutedPass` inside a case HermiT found inconsistent. The
/// pass still rests on an absence, but a vacuous proof does not refute
/// it, so the note must not claim the divergence contradicts it.
#[test]
fn a_vacuous_proof_does_not_refute_an_unrefuted_pass() {
    let note = explain_divergence(
        Origin::Unrefuted,
        Answer::NotEntailed,
        Answer::Entailed,
        true,
    );
    assert!(
        !note.contains("absence of proof this divergence contradicts"),
        "a proof that holds for every sentence alike contradicts nothing: {note}"
    );
    assert!(
        note.contains("A vacuous proof does not refute that pass"),
        "the note must say why the UnrefutedPass still stands for now: {note}"
    );

    // And the ordinary case is untouched.
    let ordinary = explain_divergence(
        Origin::Unrefuted,
        Answer::NotEntailed,
        Answer::Entailed,
        false,
    );
    assert!(
        ordinary.contains("absence of proof this divergence contradicts"),
        "in a case that has a model, HermiT's proof really does refute the pass: {ordinary}"
    );
}

/// The set is empty when no gate diverged with HermiT saying
/// INCONSISTENT, so the wording above cannot leak into an ordinary run.
#[test]
fn a_run_with_no_inconsistent_gate_names_no_vacuous_case() {
    let asked = vec![
        Asked {
            provenance: Provenance {
                case_id: "c".into(),
                check: GATE_EXPECT_CONSISTENT.into(),
                asked: "is the ontology, plus this case's data, consistent?".into(),
                origin: Origin::Gate,
            },
            comparison: Comparison::Agree {
                answer: Answer::Consistent,
            },
        },
        Asked {
            provenance: Provenance {
                case_id: "c".into(),
                check: "x instanceOf Y".into(),
                asked: "does the ontology entail that x is a Y?".into(),
                origin: Origin::FailingPositive,
            },
            comparison: Comparison::Divergence {
                question: Provenance {
                    case_id: "c".into(),
                    check: "x instanceOf Y".into(),
                    asked: "does the ontology entail that x is a Y?".into(),
                    origin: Origin::FailingPositive,
                },
                rustdl: Answer::NotEntailed,
                hermit: Answer::Entailed,
            },
        },
    ];
    assert!(
        cases_hermit_found_inconsistent(&asked).is_empty(),
        "a case whose gate both reasoners answered CONSISTENT is not a vacuous case"
    );

    let opts = DifferentialOptions {
        suite: Path::new("suites/sulo"),
        ontology: Path::new(SULO),
        robot: Path::new("robot.jar"),
        filter: None,
        workdir: Path::new("probes"),
        divergences: None,
        accept_divergences: false,
    };
    let text = sulo_testharness::differential::render(&asked, &opts, None);
    assert!(
        text.contains("AGAINST a SULO regression"),
        "the report must still settle an ordinary failing positive: {text}"
    );
}

/// The same two questions, with the gate DIVERGING and HermiT saying
/// INCONSISTENT. Both reports, text and JSON, must switch wording, and
/// this is the test that pins the wiring rather than the function.
#[test]
fn a_report_over_an_inconsistent_case_calls_its_proofs_vacuous() {
    let gate = Provenance {
        case_id: "c".into(),
        check: GATE_EXPECT_CONSISTENT.into(),
        asked: "is the ontology, plus this case's data, consistent?".into(),
        origin: Origin::Gate,
    };
    let positive = Provenance {
        case_id: "c".into(),
        check: "x instanceOf Y".into(),
        asked: "does the ontology entail that x is a Y?".into(),
        origin: Origin::FailingPositive,
    };
    let asked = vec![
        Asked {
            provenance: gate.clone(),
            comparison: Comparison::Divergence {
                question: gate,
                rustdl: Answer::Consistent,
                hermit: Answer::Inconsistent,
            },
        },
        Asked {
            provenance: positive.clone(),
            comparison: Comparison::Divergence {
                question: positive,
                rustdl: Answer::NotEntailed,
                hermit: Answer::Entailed,
            },
        },
    ];

    let opts = DifferentialOptions {
        suite: Path::new("suites/sulo"),
        ontology: Path::new(SULO),
        robot: Path::new("robot.jar"),
        filter: None,
        workdir: Path::new("probes"),
        divergences: None,
        accept_divergences: false,
    };

    let text = sulo_testharness::differential::render(&asked, &opts, None);
    assert!(
        !text.contains("AGAINST a SULO regression")
            && !text.contains("not a finding about the ontology"),
        "neither sentence is true of a case HermiT found inconsistent: {text}"
    );
    assert!(
        text.contains("NO MODEL") && text.contains("VACUOUS"),
        "the report must name the inconsistency and the vacuity: {text}"
    );

    let json = sulo_testharness::differential::render_json(&asked, &opts, None);
    let payload: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let notes: Vec<String> = payload["questions"]
        .as_array()
        .expect("questions array")
        .iter()
        .map(|q| q["note"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(notes.len(), 2, "{json}");
    assert!(
        notes
            .iter()
            .all(|n| !n.contains("AGAINST a SULO regression")
                && !n.contains("not a finding about the ontology")),
        "the JSON report must not contradict the text one: {json}"
    );
    assert!(
        notes.iter().any(|n| n.contains("VACUOUS")),
        "the JSON report must carry the vacuity too: {json}"
    );
}

/// The other direction of `explain_divergence`, which no fixture in
/// this repository can produce: rustdl claiming a proof HermiT
/// refutes. That would be an UNSOUNDNESS, and it must not be reported
/// with the reassuring incompleteness wording.
#[test]
fn a_proof_hermit_refutes_is_reported_as_unsoundness() {
    let note = explain_divergence(
        Origin::Unrefuted,
        Answer::Entailed,
        Answer::NotEntailed,
        false,
    );
    assert!(
        note.contains("UNSOUNDNESS") && note.contains("rustdl is the outlier"),
        "the alarming direction must be named as such: {note}"
    );
    assert!(
        !note.contains("INCOMPLETENESS"),
        "an unsoundness must not be described as an incompleteness: {note}"
    );
}

// ---------------------------------------------------------------
// 6: ruling 8. Nothing asked is not everything agreed.
// ---------------------------------------------------------------

/// The refusal itself, as a message.
#[test]
fn a_run_that_asked_nothing_is_refused_by_name() {
    let msg = no_questions_refusal(Path::new("suites/sulo"), 3, Some("nothing"));
    assert!(
        msg.contains("NO questions") && msg.contains("Nothing asked is not everything agreed"),
        "the refusal must say what went wrong and why it is not a pass: {msg}"
    );
    assert!(
        msg.contains("suites/sulo") && msg.contains("\"nothing\""),
        "the refusal must name the suite and the filter: {msg}"
    );
}

/// The other half of ruling 8, and the honest one: the guard above
/// CANNOT fire against this repository, because every case yields at
/// least its consistency-gate question.
///
/// Pinned rather than left implicit. If someone ever makes the gate
/// question conditional, this test fails and points at the guard that
/// then starts mattering, instead of the guard silently becoming
/// load-bearing with nobody having tested it.
#[test]
fn every_case_in_the_real_suite_yields_at_least_the_gate_question() {
    let selected =
        select(Path::new("suites/sulo"), Some(sulo()), None).expect("the SULO suite selects");
    assert!(!selected.is_empty(), "the suite is not empty");
    for (case, path) in &selected {
        // No rustdl answers passed: the question COUNT must not depend
        // on them, only which of them carry an answer.
        let qs = questions(case, sulo(), &[]);
        assert!(
            !qs.is_empty(),
            "{} yields no questions, so a --filter selecting only it would reach \
             ruling 8's refusal. That guard is untested against a real case; go read \
             differential::no_questions_refusal",
            path.display()
        );
        assert_eq!(
            qs[0].provenance.origin,
            Origin::Gate,
            "{} must ask its consistency gate first",
            case.id
        );
    }
}

// ---------------------------------------------------------------
// 7: the run's exit code.
// ---------------------------------------------------------------

fn asked_with(comparison: Comparison) -> Asked {
    Asked {
        provenance: Provenance {
            case_id: "c".to_string(),
            check: "k".to_string(),
            asked: "q".to_string(),
            origin: Origin::Gate,
        },
        comparison,
    }
}

/// All four rows, including the one that matters most: a divergence
/// buried among agreements still exits 5.
#[test]
fn the_exit_code_ranks_divergence_above_everything() {
    let agree = asked_with(Comparison::Agree {
        answer: Answer::Consistent,
    });
    let diverge = asked_with(Comparison::Divergence {
        question: Provenance {
            case_id: "c".to_string(),
            check: "k".to_string(),
            asked: "q".to_string(),
            origin: Origin::Gate,
        },
        rustdl: Answer::Consistent,
        hermit: Answer::Inconsistent,
    });
    let unknown = asked_with(Comparison::Indeterminate {
        question: Provenance {
            case_id: "c".to_string(),
            check: "k".to_string(),
            asked: "q".to_string(),
            origin: Origin::Gate,
        },
        reason: "robot fell over".to_string(),
    });

    assert_eq!(differential_exit_code(std::slice::from_ref(&agree)), 0);
    assert_eq!(differential_exit_code(std::slice::from_ref(&unknown)), 3);
    assert_eq!(differential_exit_code(std::slice::from_ref(&diverge)), 5);
    assert_eq!(
        differential_exit_code(&[agree.clone(), unknown.clone()]),
        3,
        "one unanswered question is not everything agreed"
    );
    assert_eq!(
        differential_exit_code(&[agree.clone(), unknown.clone(), diverge.clone()]),
        5,
        "a divergence is this job's headline result and must not be buried under a 3"
    );
    assert_eq!(
        differential_exit_code(&[agree.clone(), agree]),
        0,
        "every question asked and every answer matched"
    );
}

/// A configuration error is not a verdict about either reasoner: an
/// unusable `--robot` is refused before any case is loaded, rather
/// than becoming one ROBOT `Error` per question.
#[test]
fn an_unusable_robot_jar_is_a_configuration_error() {
    let workdir = workdir("no-jar");
    match run_differential(&DifferentialOptions {
        suite: Path::new("suites/sulo"),
        ontology: sulo(),
        robot: Path::new("/nonexistent/robot.jar"),
        filter: Some("taxonomy/deep-chain"),
        workdir: &workdir,
        divergences: None,
        accept_divergences: false,
    }) {
        DifferentialOutcome::Config(msg) => assert!(
            msg.contains("--robot") && msg.contains("not a readable file"),
            "{msg}"
        ),
        DifferentialOutcome::Ran(_) => {
            panic!("a missing jar must be a configuration error, not a run")
        }
    }
}

/// A filter matching nothing is exit 2 here for the same reason it is
/// on `run`: a differential over zero cases would report a green
/// cross-check having asked nothing.
#[test]
fn a_filter_matching_nothing_is_a_configuration_error() {
    let workdir = workdir("no-match");
    match run_differential(&DifferentialOptions {
        suite: Path::new("suites/sulo"),
        ontology: sulo(),
        robot: Path::new("tests/fixtures/clean.ttl"),
        filter: Some("no-such-case-anywhere"),
        workdir: &workdir,
        divergences: None,
        accept_divergences: false,
    }) {
        DifferentialOutcome::Config(msg) => assert!(msg.contains("matched none of the"), "{msg}"),
        DifferentialOutcome::Ran(_) => panic!("a filter matching nothing must be refused"),
    }
}
