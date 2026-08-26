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
    Answer, Comparison, Probe, Provenance, Question, QuestionKind, ask, compare, questions,
};
use sulo_testharness::hermit::HermitAnswer;
use sulo_testharness::manifest::{Case, SubsumptionExpr};
use sulo_testharness::suite::{GATE_EXPECT_CONSISTENT, run_case};
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
