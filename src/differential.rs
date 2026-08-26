//! Questions for HermiT, and what it means when the two reasoners
//! disagree.
//!
//! rustdl is sound but incomplete, so "not entailed" from it is an
//! absence of proof, never a proof of absence. HermiT is complete for
//! OWL 2 DL, so it is the oracle of record for exactly those answers.
//! This module turns a `Case` into questions HermiT can be asked
//! through `crate::hermit`, and compares the two answers.
//!
//! # Everything reduces to consistency
//!
//! ROBOT gives us one primitive: is this ontology consistent? Each of
//! the three question shapes below is a standard reduction onto it,
//! and every one of them was verified in BOTH directions against real
//! SULO before it was written down, because a probe that is silently
//! MISPARSED comes back CONSISTENT, "consistent" reads as "not
//! entailed", and "not entailed" is what rustdl already said. That is
//! a differential that agrees with itself forever while proving
//! nothing, which is this project's recurring defect shape wearing a
//! new hat. Observed answers, `../sulo/sulo.ttl` plus ROBOT 1.9.7:
//!
//! | probe | question | HermiT |
//! |---|---|---|
//! | `Feature and not Object` | entailed? | INCONSISTENT (entailed) |
//! | `Object and not (SpatialObject or Feature)` | entailed? | CONSISTENT (not entailed) |
//! | `unit a not Feature` | entailed? | INCONSISTENT (entailed) |
//! | `unit a not Unit` | entailed? | CONSISTENT (not entailed) |
//! | NPA `encounter hasParticipant alice` | entailed? | INCONSISTENT (entailed) |
//! | NPA `alice hasParticipant encounter` | entailed? | CONSISTENT (not entailed) |
//! | NPA `measurement hasValue "170"^^xsd:decimal` | entailed? | INCONSISTENT (entailed) |
//! | NPA `measurement hasValue "171"^^xsd:decimal` | entailed? | CONSISTENT (not entailed) |
//! | witness of `Capability` | satisfiable? | CONSISTENT (satisfiable) |
//! | witness of `Object and Process` | satisfiable? | INCONSISTENT (unsatisfiable) |
//!
//! Every one of those has a matching control in `tests/differential.rs`,
//! so the encodings cannot rot silently: half of them would have to
//! flip for the suite to stay green.
//!
//! # A question that cannot be encoded is not a question that agreed
//!
//! Two routes lead to "no comparison was possible", and both end in
//! [`Comparison::Indeterminate`], never [`Comparison::Agree`]:
//!
//! * HermiT could not answer ([`crate::hermit::HermitAnswer::Error`]:
//!   a ROBOT failure, an expired deadline, an unrunnable jar). This is
//!   ruling 3 of the plan.
//! * The question could not be put to HermiT at all
//!   ([`Probe::Unencodable`]), or rustdl never answered it
//!   ([`Question::rustdl`] is `None`, e.g. the check timed out, or the
//!   consistency gate skipped it).
//!
//! In particular a class expression this module cannot render into
//! OWL/RDF is `Unencodable` and LOUD, rather than dropped from the
//! question list. A dropped question is invisible; an `Indeterminate`
//! is exit 3 and a line in the report naming the shape that needs
//! support.
//!
//! # What this module does not do
//!
//! It builds and compares. It does not decide the run's exit code, and
//! it does not check that a run asked any questions at all: a case
//! with only POSITIVE assertions yields zero questions, since a
//! positive `Pass` from a sound reasoner needs no oracle of record.
//! Whoever drives a whole suite through here (the `differential`
//! subcommand, phase 7 task 4) must refuse a run that produced no
//! questions, for the same reason `suite::discover` refuses a suite
//! with no cases: nothing asked is not everything agreed.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use horned_owl::model::{
    Build, Class, ClassExpression, Individual, ObjectProperty, ObjectPropertyExpression, RcStr,
};

use crate::claim::{Claim, Literal, parse_ce, parse_fragment};
use crate::hermit::{self, HermitAnswer};
use crate::manifest::Case;
use crate::prefixes::{base_mapping, with_overrides};
use crate::suite::{GATE_EXPECT_CONSISTENT, GATE_EXPECT_INCONSISTENT};
use crate::verdict::{CheckOutcome, Verdict};

/// The class the probe defines. Named rather than inlined because
/// that is the shape the non-entailment encoding was measured with.
///
/// Both probe terms live under the reserved `.invalid` TLD, which is
/// guaranteed never to resolve and, more to the point, guaranteed not
/// to collide with a suite's own `ex:` individuals. A collision would
/// not error: it would silently change what the probe asks, by making
/// the witness an individual the data already says things about.
const PROBE_CLASS: &str = "http://sulo-testharness.invalid/differential#probe";

/// The individual asserted to be in [`PROBE_CLASS`].
const PROBE_WITNESS: &str = "http://sulo-testharness.invalid/differential#witness";

const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

/// One reasoner's answer, in the terms of the question that was asked.
///
/// Three answer spaces share one enum. They never mix in practice
/// because [`questions`] produces both sides of a comparison from the
/// same [`QuestionKind`]; if they ever did, the mismatch would surface
/// as a loud `Divergence`, never as a quiet `Agree`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// The consistency question: a model exists.
    Consistent,
    /// The consistency question: a clash was found.
    Inconsistent,
    /// An entailment question: the entailment holds. From rustdl this
    /// is a proof; from HermiT it is a proof too (the probe clashed).
    Entailed,
    /// An entailment question: from HermiT, a countermodel exists and
    /// the entailment provably does NOT hold. From rustdl, only that
    /// no proof was found. That asymmetry is the entire reason this
    /// module exists, and it is why the two reasoners saying
    /// `NotEntailed` together is worth more than either alone.
    NotEntailed,
    /// A satisfiability question: the expression has a model.
    Satisfiable,
    /// A satisfiability question: the expression is empty in every
    /// model.
    Unsatisfiable,
}

impl fmt::Display for Answer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Answer::Consistent => "consistent",
            Answer::Inconsistent => "inconsistent",
            Answer::Entailed => "entailed",
            Answer::NotEntailed => "not entailed",
            Answer::Satisfiable => "satisfiable",
            Answer::Unsatisfiable => "unsatisfiable",
        };
        f.write_str(s)
    }
}

/// What a question asks, which is also how HermiT's one primitive
/// (consistency) is read back into the question's own answer space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionKind {
    /// Is the case's ontology plus its data consistent?
    Consistency,
    /// Does the ontology entail something? Encoded so that a CLASH
    /// means "entailed": the probe asserts the negation.
    Entailment,
    /// Does an expression have a model? Encoded so that a clash means
    /// "unsatisfiable": the probe asserts a witness of it.
    Satisfiability,
}

impl QuestionKind {
    /// Read HermiT's consistency answer as an answer to THIS question.
    ///
    /// `Err` carries the reason nothing was learned. Note that the
    /// entailment and satisfiability rows invert: for those, the
    /// probe's inconsistency is the positive finding.
    fn read(self, hermit: &HermitAnswer) -> Result<Answer, String> {
        match (self, hermit) {
            (_, HermitAnswer::Error(msg)) => Err(msg.clone()),
            (QuestionKind::Consistency, HermitAnswer::Consistent) => Ok(Answer::Consistent),
            (QuestionKind::Consistency, HermitAnswer::Inconsistent) => Ok(Answer::Inconsistent),
            (QuestionKind::Entailment, HermitAnswer::Consistent) => Ok(Answer::NotEntailed),
            (QuestionKind::Entailment, HermitAnswer::Inconsistent) => Ok(Answer::Entailed),
            (QuestionKind::Satisfiability, HermitAnswer::Consistent) => Ok(Answer::Satisfiable),
            (QuestionKind::Satisfiability, HermitAnswer::Inconsistent) => Ok(Answer::Unsatisfiable),
        }
    }
}

/// The extra ontology merged alongside the case's own inputs to turn
/// a question into a consistency check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// The question IS consistency, so nothing is added.
    None,
    /// Turtle to merge in.
    Turtle(String),
    /// The question could not be encoded. Kept as a question rather
    /// than dropped from the list: see the module doc.
    Unencodable(String),
}

/// Where a question came from, in enough detail to name a
/// disagreement without holding the whole `Case`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// The case's `id:`.
    pub case_id: String,
    /// The check name the suite gave this same question, so a
    /// divergence can be lined up against the run's own report.
    pub check: String,
    /// What was asked, in prose.
    pub asked: String,
}

impl fmt::Display for Provenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} / {} / {}", self.case_id, self.check, self.asked)
    }
}

/// One question, both reasoners' terms of reference for it, and the
/// files needed to put it to HermiT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub provenance: Provenance,
    pub kind: QuestionKind,
    /// rustdl's answer, or `None` when rustdl did not produce one
    /// (an `Indeterminate` check, or a check the consistency gate
    /// skipped). `None` is never treated as agreement.
    pub rustdl: Option<Answer>,
    /// The ontology under test.
    pub ontology: PathBuf,
    /// The case's `imports:` and `data:`, already resolved.
    pub extra: Vec<PathBuf>,
    pub probe: Probe,
}

/// How the two reasoners compared on one question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comparison {
    /// Both reasoners gave the same answer.
    Agree { answer: Answer },
    /// They gave different answers, which means one of them is WRONG.
    /// Carries both answers and the question, because the reader's
    /// job is to work out which one, and neither answer alone lets
    /// them.
    Divergence {
        question: Provenance,
        rustdl: Answer,
        hermit: Answer,
    },
    /// No comparison was possible. See the module doc for the two
    /// routes here; neither is agreement.
    Indeterminate {
        question: Provenance,
        reason: String,
    },
}

/// Compare the two reasoners on one question.
///
/// Ruling 3, and the single most dangerous line in this module if it
/// is written the other way: a ROBOT `Error` becomes `Indeterminate`,
/// never `Agree`. A differential that counted "HermiT fell over" as
/// agreement would go permanently green the first time the jar path
/// broke in CI, and every question would report a cross-check that
/// never happened.
#[must_use]
pub fn compare(question: &Question, hermit: &HermitAnswer) -> Comparison {
    let hermit = match question.kind.read(hermit) {
        Ok(a) => a,
        Err(reason) => {
            return Comparison::Indeterminate {
                question: question.provenance.clone(),
                reason: format!("HermiT gave no answer, so nothing was cross-checked: {reason}"),
            };
        }
    };

    let Some(rustdl) = question.rustdl else {
        return Comparison::Indeterminate {
            question: question.provenance.clone(),
            reason: format!(
                "HermiT answered {hermit}, but rustdl produced no comparable answer for \
                 this check (it was Indeterminate, or the consistency gate skipped it), \
                 so there is nothing to compare it against"
            ),
        };
    };

    if rustdl == hermit {
        Comparison::Agree { answer: hermit }
    } else {
        Comparison::Divergence {
            question: question.provenance.clone(),
            rustdl,
            hermit,
        }
    }
}

/// Put one question to HermiT.
///
/// Writes the probe ontology into `workdir` (with a comment naming the
/// question, since a CI artifact full of anonymous `probe.ttl` files
/// helps nobody) and runs the two-step driver. Give each question its
/// own `workdir`.
///
/// An `Unencodable` question never reaches ROBOT: it returns
/// `HermitAnswer::Error`, so it travels the same route to
/// `Indeterminate` as a ROBOT failure and cannot take a different one.
#[must_use]
pub fn ask(robot: &Path, question: &Question, workdir: &Path) -> HermitAnswer {
    let mut extra = question.extra.clone();

    match &question.probe {
        Probe::Unencodable(reason) => {
            return HermitAnswer::Error(format!(
                "this question could not be encoded for HermiT, so it was never asked: \
                 {reason}"
            ));
        }
        Probe::None => {}
        Probe::Turtle(text) => {
            if let Err(e) = std::fs::create_dir_all(workdir) {
                return HermitAnswer::Error(format!(
                    "cannot create the probe directory {}: {e}",
                    workdir.display()
                ));
            }
            let path = workdir.join("probe.ttl");
            let document = format!("# Differential probe for {}\n{text}", question.provenance);
            if let Err(e) = std::fs::write(&path, document) {
                return HermitAnswer::Error(format!(
                    "cannot write the probe ontology {}: {e}",
                    path.display()
                ));
            }
            extra.push(path);
        }
    }

    hermit::consistency(robot, &question.ontology, &extra, workdir)
}

// -------------------------------------------------------------------
// Building the questions.
// -------------------------------------------------------------------

/// Every question this case puts to HermiT, with rustdl's own answers
/// read out of `rustdl`, the checks `suite::run_case` recorded for the
/// same case.
///
/// The negative assertions and the consistency gate only, per spec
/// 5.3: those are exactly the answers soundness cannot vouch for. A
/// positive `entails:` that rustdl PASSED is proved, and asking HermiT
/// to confirm a proof is not what the oracle of record is for.
///
/// Answers are matched to checks BY NAME, using the same format
/// strings `suite::run_case` builds its check names from
/// (`tests/differential.rs` pins that they line up, over a fixture
/// carrying every shape). A name that fails to match yields
/// `rustdl: None`, which is `Indeterminate`: loud, and never
/// agreement.
#[must_use]
pub fn questions(case: &Case, default_ontology: &Path, rustdl: &[CheckOutcome]) -> Vec<Question> {
    let ontology = case
        .ontology
        .as_ref()
        .map(|p| case.base_dir.join(p))
        .unwrap_or_else(|| default_ontology.to_path_buf());
    let extra: Vec<PathBuf> = case
        .imports
        .iter()
        .chain(case.data.iter())
        .map(|p| case.base_dir.join(p))
        .collect();

    let pm = with_overrides(&base_mapping(), &case.prefixes);
    let mut out = Vec::new();

    let build =
        |provenance: Provenance, kind: QuestionKind, rustdl: Option<Answer>, probe: Probe| {
            Question {
                provenance,
                kind,
                rustdl,
                ontology: ontology.clone(),
                extra: extra.clone(),
                probe,
            }
        };

    // The consistency gate. Always asked: it is the one question every
    // case has, and `expect_inconsistent` cases have no other.
    {
        let (check, pass, fail) = if case.expect_inconsistent {
            // The gate passes by FINDING the clash the case expects.
            (
                GATE_EXPECT_INCONSISTENT,
                Answer::Inconsistent,
                Answer::Consistent,
            )
        } else {
            // The gate passes by finding no clash, which is the
            // absence-resting direction.
            (
                GATE_EXPECT_CONSISTENT,
                Answer::Consistent,
                Answer::Inconsistent,
            )
        };
        out.push(build(
            Provenance {
                case_id: case.id.clone(),
                check: check.to_string(),
                asked: "is the ontology, plus this case's data, consistent?".to_string(),
            },
            QuestionKind::Consistency,
            answer_of(rustdl, check, Some(pass), None, Some(fail)),
            Probe::None,
        ));
    }

    // `not_entails:`, one question per claim in the fragment.
    if let Some(fragment) = &case.not_entails {
        match parse_fragment(fragment, &pm) {
            Ok(claims) if claims.is_empty() => out.push(build(
                Provenance {
                    case_id: case.id.clone(),
                    check: "empty fragment".to_string(),
                    asked: "not_entails".to_string(),
                },
                QuestionKind::Entailment,
                None,
                Probe::Unencodable(
                    "the not_entails fragment parsed to zero claims, so there is nothing \
                     to cross-check"
                        .to_string(),
                ),
            )),
            Ok(claims) => {
                for claim in &claims {
                    let (kind, probe) = encode_claim(claim);
                    let check = format!("{claim:?}");
                    let rustdl = answer_of(
                        rustdl,
                        &check,
                        None,
                        Some(kind_negative(kind)),
                        Some(kind_positive(kind)),
                    );
                    out.push(build(
                        Provenance {
                            case_id: case.id.clone(),
                            check,
                            asked: describe_claim(claim),
                        },
                        kind,
                        rustdl,
                        probe,
                    ));
                }
            }
            Err(e) => out.push(build(
                Provenance {
                    case_id: case.id.clone(),
                    check: "fragment parse".to_string(),
                    asked: "not_entails".to_string(),
                },
                QuestionKind::Entailment,
                None,
                Probe::Unencodable(format!("the not_entails fragment does not parse: {e}")),
            )),
        }
    }

    // `not_entails_manchester:`, one question per subsumption.
    for s in &case.not_entails_manchester {
        let check = format!("{} subClassOf {}", s.sub_expr, s.sup_expr);
        let probe = match (parse_ce(&s.sub_expr, &pm), parse_ce(&s.sup_expr, &pm)) {
            (Ok(sub), Ok(sup)) => subsumption_probe(&sub, &sup),
            (Err(e), _) | (_, Err(e)) => Probe::Unencodable(e.to_string()),
        };
        let rustdl = answer_of(
            rustdl,
            &check,
            None,
            Some(Answer::NotEntailed),
            Some(Answer::Entailed),
        );
        out.push(build(
            Provenance {
                case_id: case.id.clone(),
                check: check.clone(),
                asked: format!("does the ontology entail {check}?"),
            },
            QuestionKind::Entailment,
            rustdl,
            probe,
        ));
    }

    // `satisfiable_expr:`, one question per expression.
    for expr in &case.satisfiable_expr {
        let check = format!("satisfiable: {expr}");
        let probe = match parse_ce(expr, &pm) {
            Ok(ce) => witness_probe(&ce),
            Err(e) => Probe::Unencodable(e.to_string()),
        };
        let rustdl = answer_of(
            rustdl,
            &check,
            None,
            Some(Answer::Satisfiable),
            Some(Answer::Unsatisfiable),
        );
        out.push(build(
            Provenance {
                case_id: case.id.clone(),
                check: check.clone(),
                asked: format!("does {expr} have a model?"),
            },
            QuestionKind::Satisfiability,
            rustdl,
            probe,
        ));
    }

    out
}

/// The answer meaning "the reasoner found the proof it was probing
/// for" in this question's answer space.
fn kind_positive(kind: QuestionKind) -> Answer {
    match kind {
        QuestionKind::Consistency => Answer::Inconsistent,
        QuestionKind::Entailment => Answer::Entailed,
        QuestionKind::Satisfiability => Answer::Unsatisfiable,
    }
}

/// The answer meaning "no proof was found".
fn kind_negative(kind: QuestionKind) -> Answer {
    match kind {
        QuestionKind::Consistency => Answer::Consistent,
        QuestionKind::Entailment => Answer::NotEntailed,
        QuestionKind::Satisfiability => Answer::Satisfiable,
    }
}

/// Read rustdl's answer out of the check the suite recorded under
/// `name`.
///
/// The three verdicts map differently per question shape, so each is
/// passed in explicitly rather than inferred; `None` for an arm means
/// "this verdict cannot arise here, and if it somehow does, claim no
/// answer". `Indeterminate` and a missing check are always `None`: an
/// answer rustdl did not give must never be compared as though it had.
fn answer_of(
    checks: &[CheckOutcome],
    name: &str,
    on_pass: Option<Answer>,
    on_unrefuted: Option<Answer>,
    on_fail: Option<Answer>,
) -> Option<Answer> {
    let outcome = checks.iter().find(|c| c.name == name)?;
    match &outcome.verdict {
        Verdict::Pass => on_pass,
        Verdict::UnrefutedPass => on_unrefuted,
        Verdict::Fail(_) => on_fail,
        Verdict::Indeterminate(_) => None,
    }
}

/// Prose for the report. `Claim`'s `Debug` is the check NAME (that is
/// what `oracle::check` uses); this is the human sentence beside it.
///
/// ASCII only, in the vocabulary `oracle`'s own check names already
/// use (`subClassOf`, not the inclusion sign): this string is report
/// output, and the logic notation in this file's doc comments is for
/// the reader of the source, not for a CI log.
fn describe_claim(claim: &Claim) -> String {
    match claim {
        Claim::Subsumption { sub, sup } => {
            format!("does the ontology entail {sub} subClassOf {sup}?")
        }
        Claim::Equivalence { left, right } => {
            format!("does the ontology entail {left} equivalentClass {right}?")
        }
        Claim::Unsatisfiable { class } => {
            format!("does the ontology entail that {class} is unsatisfiable?")
        }
        Claim::ClassAssertion { individual, class } => {
            format!("does the ontology entail that {individual} is a {class}?")
        }
        Claim::ObjectPropertyAssertion {
            subject,
            property,
            object,
        } => format!("does the ontology entail {subject} {property} {object}?"),
        Claim::DataPropertyAssertion {
            subject,
            property,
            literal,
        } => format!(
            "does the ontology entail {subject} {property} {}?",
            render_literal(literal)
        ),
    }
}

// -------------------------------------------------------------------
// The probe encodings.
// -------------------------------------------------------------------

/// Turn one claim into the question "does the ontology entail this?".
///
/// Every arm is an entailment question, including `Unsatisfiable`:
/// "is `C` entailed to be empty" is asked by asserting a witness of
/// `C` and seeing whether that clashes, which is the same probe
/// `witness_probe` builds but read in the entailment answer space.
fn encode_claim(claim: &Claim) -> (QuestionKind, Probe) {
    let probe = match claim {
        Claim::Subsumption { sub, sup } => subsumption_probe(&named_class(sub), &named_class(sup)),
        Claim::Equivalence { left, right } => {
            equivalence_probe(&named_class(left), &named_class(right))
        }
        Claim::Unsatisfiable { class } => witness_probe(&named_class(class)),
        Claim::ClassAssertion { individual, class } => {
            individual_complement_probe(individual, &named_class(class))
        }
        Claim::ObjectPropertyAssertion {
            subject,
            property,
            object,
        } => negative_object_property_probe(subject, property, object),
        Claim::DataPropertyAssertion {
            subject,
            property,
            literal,
        } => negative_data_property_probe(subject, property, literal),
    };
    (QuestionKind::Entailment, probe)
}

fn named_class(iri: &str) -> ClassExpression<RcStr> {
    let build: Build<RcStr> = Build::new();
    ClassExpression::Class(Class(build.iri(iri)))
}

/// `O ⊨ sub ⊑ sup` iff `O ∪ {probe ≡ sub ⊓ ¬sup, witness: probe}` is
/// inconsistent. Measured in both directions; see the module doc.
fn subsumption_probe(sub: &ClassExpression<RcStr>, sup: &ClassExpression<RcStr>) -> Probe {
    let mut decls = Decls::default();
    let (sub, sup) = match (render_ce(sub, &mut decls), render_ce(sup, &mut decls)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => return Probe::Unencodable(e),
    };
    Probe::Turtle(document(
        &decls,
        &format!(
            "<{PROBE_CLASS}> a owl:Class ;\n    owl:equivalentClass \
             [ a owl:Class ; owl:intersectionOf ( {sub} [ a owl:Class ; \
             owl:complementOf {sup} ] ) ] .\n\n<{PROBE_WITNESS}> a owl:NamedIndividual, \
             <{PROBE_CLASS}> .\n"
        ),
    ))
}

/// `O ⊨ left ≡ right` iff BOTH `left ⊓ ¬right` and `right ⊓ ¬left` are
/// empty, which is one question: is their union empty?
fn equivalence_probe(left: &ClassExpression<RcStr>, right: &ClassExpression<RcStr>) -> Probe {
    let mut decls = Decls::default();
    let (left, right) = match (render_ce(left, &mut decls), render_ce(right, &mut decls)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => return Probe::Unencodable(e),
    };
    Probe::Turtle(document(
        &decls,
        &format!(
            "<{PROBE_CLASS}> a owl:Class ;\n    owl:equivalentClass \
             [ a owl:Class ; owl:unionOf ( \
             [ a owl:Class ; owl:intersectionOf ( {left} [ a owl:Class ; \
             owl:complementOf {right} ] ) ] \
             [ a owl:Class ; owl:intersectionOf ( {right} [ a owl:Class ; \
             owl:complementOf {left} ] ) ] ) ] .\n\n<{PROBE_WITNESS}> a \
             owl:NamedIndividual, <{PROBE_CLASS}> .\n"
        ),
    ))
}

/// `expr` has a model iff `O ∪ {probe ≡ expr, witness: probe}` is
/// consistent.
fn witness_probe(expr: &ClassExpression<RcStr>) -> Probe {
    let mut decls = Decls::default();
    let expr = match render_ce(expr, &mut decls) {
        Ok(t) => t,
        Err(e) => return Probe::Unencodable(e),
    };
    Probe::Turtle(document(
        &decls,
        &format!(
            "<{PROBE_CLASS}> a owl:Class ;\n    owl:equivalentClass {expr} \
             .\n\n<{PROBE_WITNESS}> a owl:NamedIndividual, <{PROBE_CLASS}> .\n"
        ),
    ))
}

/// `O ⊨ individual: class` iff `O ∪ {individual: ¬class}` is
/// inconsistent. The named individual is the witness here; a fresh one
/// would ask a different question entirely.
fn individual_complement_probe(individual: &str, class: &ClassExpression<RcStr>) -> Probe {
    let mut decls = Decls::default();
    decls.individuals.insert(individual.to_string());
    let class = match render_ce(class, &mut decls) {
        Ok(t) => t,
        Err(e) => return Probe::Unencodable(e),
    };
    Probe::Turtle(document(
        &decls,
        &format!(
            "<{PROBE_CLASS}> a owl:Class ;\n    owl:equivalentClass \
             [ a owl:Class ; owl:complementOf {class} ] .\n\n<{individual}> a \
             <{PROBE_CLASS}> .\n"
        ),
    ))
}

/// `O ⊨ s p o` iff `O` plus the negative property assertion is
/// inconsistent.
fn negative_object_property_probe(subject: &str, property: &str, object: &str) -> Probe {
    let mut decls = Decls::default();
    decls.individuals.insert(subject.to_string());
    decls.individuals.insert(object.to_string());
    decls.object_properties.insert(property.to_string());
    Probe::Turtle(document(
        &decls,
        &format!(
            "[] a owl:NegativePropertyAssertion ;\n    owl:sourceIndividual \
             <{subject}> ;\n    owl:assertionProperty <{property}> ;\n    \
             owl:targetIndividual <{object}> .\n"
        ),
    ))
}

/// The data-property counterpart of [`negative_object_property_probe`],
/// differing only in `owl:targetValue`.
fn negative_data_property_probe(subject: &str, property: &str, literal: &Literal) -> Probe {
    let mut decls = Decls::default();
    decls.individuals.insert(subject.to_string());
    decls.data_properties.insert(property.to_string());
    Probe::Turtle(document(
        &decls,
        &format!(
            "[] a owl:NegativePropertyAssertion ;\n    owl:sourceIndividual \
             <{subject}> ;\n    owl:assertionProperty <{property}> ;\n    \
             owl:targetValue {} .\n",
            render_literal(literal)
        ),
    ))
}

/// The entities a probe mentions, so it can declare them.
///
/// Declarations are not decoration. ROBOT parses each `--input`
/// SEPARATELY and merges afterwards, so a probe naming
/// `sulo:hasPart` in an `owl:onProperty` is parsed with no idea what
/// kind of entity that is, and OWLAPI is left to guess. A guess that
/// goes the wrong way produces an ontology that says something other
/// than what was asked, and the usual consequence is a probe that
/// cannot clash: CONSISTENT, "not entailed", agreement with rustdl,
/// green.
#[derive(Default)]
struct Decls {
    classes: BTreeSet<String>,
    object_properties: BTreeSet<String>,
    data_properties: BTreeSet<String>,
    individuals: BTreeSet<String>,
}

impl Decls {
    fn render(&self) -> String {
        let mut out = String::new();
        for (set, kind) in [
            (&self.classes, "owl:Class"),
            (&self.object_properties, "owl:ObjectProperty"),
            (&self.data_properties, "owl:DatatypeProperty"),
            (&self.individuals, "owl:NamedIndividual"),
        ] {
            for iri in set {
                out.push_str(&format!("<{iri}> a {kind} .\n"));
            }
        }
        out
    }
}

/// Wrap declarations and a body in the prefix header every probe
/// shares.
fn document(decls: &Decls, body: &str) -> String {
    format!(
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n\
         {}\n{body}",
        decls.render()
    )
}

/// Render a class expression as a Turtle term (an IRI, or an inline
/// blank node), recording every entity it names.
///
/// Unsupported shapes are an `Err`, never an approximation. A shape
/// rendered wrongly is a probe that quietly asks a different question;
/// an `Err` is an `Indeterminate` naming the shape. The supported set
/// is exactly what `tests/differential.rs` verifies against a real
/// HermiT in both directions, plus nothing.
fn render_ce(ce: &ClassExpression<RcStr>, decls: &mut Decls) -> Result<String, String> {
    Ok(match ce {
        ClassExpression::Class(Class(iri)) => {
            decls.classes.insert(iri.to_string());
            format!("<{iri}>")
        }
        ClassExpression::ObjectIntersectionOf(v) => {
            format!(
                "[ a owl:Class ; owl:intersectionOf ( {} ) ]",
                list(v, decls)?
            )
        }
        ClassExpression::ObjectUnionOf(v) => {
            format!("[ a owl:Class ; owl:unionOf ( {} ) ]", list(v, decls)?)
        }
        ClassExpression::ObjectComplementOf(b) => {
            format!(
                "[ a owl:Class ; owl:complementOf {} ]",
                render_ce(b, decls)?
            )
        }
        ClassExpression::ObjectOneOf(is) => {
            let mut terms = Vec::with_capacity(is.len());
            for i in is {
                terms.push(render_individual(i, decls)?);
            }
            format!("[ a owl:Class ; owl:oneOf ( {} ) ]", terms.join(" "))
        }
        ClassExpression::ObjectSomeValuesFrom { ope, bce } => format!(
            "[ a owl:Restriction ; owl:onProperty {} ; owl:someValuesFrom {} ]",
            render_ope(ope, decls),
            render_ce(bce, decls)?
        ),
        ClassExpression::ObjectAllValuesFrom { ope, bce } => format!(
            "[ a owl:Restriction ; owl:onProperty {} ; owl:allValuesFrom {} ]",
            render_ope(ope, decls),
            render_ce(bce, decls)?
        ),
        ClassExpression::ObjectHasValue { ope, i } => format!(
            "[ a owl:Restriction ; owl:onProperty {} ; owl:hasValue {} ]",
            render_ope(ope, decls),
            render_individual(i, decls)?
        ),
        ClassExpression::ObjectMinCardinality { n, ope, bce } => {
            cardinality("min", *n, ope, bce, decls)?
        }
        ClassExpression::ObjectMaxCardinality { n, ope, bce } => {
            cardinality("max", *n, ope, bce, decls)?
        }
        ClassExpression::ObjectExactCardinality { n, ope, bce } => {
            cardinality("exact", *n, ope, bce, decls)?
        }
        other => {
            return Err(format!(
                "the class expression shape {} has no verified OWL/RDF encoding in this \
                 harness, so the question was not put to HermiT rather than put to it \
                 wrongly. Add it to differential::render_ce, with a control in \
                 tests/differential.rs proving it clashes when it should",
                shape_name(other)
            ));
        }
    })
}

/// The qualified form for all three cardinality restrictions.
/// horned-owl always carries a filler (`owl:Thing` for the unqualified
/// form), and `owl:minQualifiedCardinality ... owl:onClass owl:Thing`
/// is semantically the unqualified restriction, so one rendering
/// serves both.
fn cardinality(
    which: &str,
    n: u32,
    ope: &ObjectPropertyExpression<RcStr>,
    bce: &ClassExpression<RcStr>,
    decls: &mut Decls,
) -> Result<String, String> {
    let predicate = match which {
        "min" => "owl:minQualifiedCardinality",
        "max" => "owl:maxQualifiedCardinality",
        _ => "owl:qualifiedCardinality",
    };
    Ok(format!(
        "[ a owl:Restriction ; owl:onProperty {} ; {predicate} \
         \"{n}\"^^xsd:nonNegativeInteger ; owl:onClass {} ]",
        render_ope(ope, decls),
        render_ce(bce, decls)?
    ))
}

fn list(v: &[ClassExpression<RcStr>], decls: &mut Decls) -> Result<String, String> {
    let mut terms = Vec::with_capacity(v.len());
    for ce in v {
        terms.push(render_ce(ce, decls)?);
    }
    Ok(terms.join(" "))
}

fn render_ope(ope: &ObjectPropertyExpression<RcStr>, decls: &mut Decls) -> String {
    match ope {
        ObjectPropertyExpression::ObjectProperty(ObjectProperty(iri)) => {
            decls.object_properties.insert(iri.to_string());
            format!("<{iri}>")
        }
        ObjectPropertyExpression::InverseObjectProperty(ObjectProperty(iri)) => {
            decls.object_properties.insert(iri.to_string());
            format!("[ owl:inverseOf <{iri}> ]")
        }
    }
}

/// Anonymous individuals are refused: a blank node in a probe cannot
/// denote the same thing as a blank node in the data, so the question
/// would silently stop being about the individual the author meant.
/// `claim.rs` refuses them on the rustdl side for the same reason.
fn render_individual(i: &Individual<RcStr>, decls: &mut Decls) -> Result<String, String> {
    match i {
        Individual::Named(ni) => {
            decls.individuals.insert(ni.0.to_string());
            Ok(format!("<{}>", ni.0))
        }
        Individual::Anonymous(a) => Err(format!(
            "the anonymous individual _:{} cannot be named in a probe ontology; use a \
             skolemised IRI",
            a.0
        )),
    }
}

/// A Turtle literal. Language tags win over the datatype, matching
/// `oracle::to_horned_literal` and RDF 1.1 (a language-tagged literal
/// IS an `rdf:langString`, and writing both is a syntax error).
fn render_literal(literal: &Literal) -> String {
    let lexical = escape(&literal.lexical);
    match &literal.language {
        Some(lang) => format!("\"{lexical}\"@{lang}"),
        None if literal.datatype == RDF_LANG_STRING => {
            // An rdf:langString with no tag is not writable in Turtle
            // and is not a legal RDF term either. Emit it as a plain
            // string rather than invalid syntax; ROBOT would reject
            // the file, which is an Error, which is Indeterminate.
            format!("\"{lexical}\"")
        }
        None => format!("\"{lexical}\"^^<{}>", literal.datatype),
    }
}

/// The five escapes Turtle requires inside a `"..."` literal.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// The variant name, for the "no verified encoding" message. Written
/// out rather than derived from `Debug`, whose output for a nested
/// expression is a page long.
fn shape_name(ce: &ClassExpression<RcStr>) -> &'static str {
    match ce {
        ClassExpression::Class(_) => "Class",
        ClassExpression::ObjectIntersectionOf(_) => "ObjectIntersectionOf",
        ClassExpression::ObjectUnionOf(_) => "ObjectUnionOf",
        ClassExpression::ObjectComplementOf(_) => "ObjectComplementOf",
        ClassExpression::ObjectOneOf(_) => "ObjectOneOf",
        ClassExpression::ObjectSomeValuesFrom { .. } => "ObjectSomeValuesFrom",
        ClassExpression::ObjectAllValuesFrom { .. } => "ObjectAllValuesFrom",
        ClassExpression::ObjectHasValue { .. } => "ObjectHasValue",
        ClassExpression::ObjectHasSelf(_) => "ObjectHasSelf",
        ClassExpression::ObjectMinCardinality { .. } => "ObjectMinCardinality",
        ClassExpression::ObjectMaxCardinality { .. } => "ObjectMaxCardinality",
        ClassExpression::ObjectExactCardinality { .. } => "ObjectExactCardinality",
        ClassExpression::DataSomeValuesFrom { .. } => "DataSomeValuesFrom",
        ClassExpression::DataAllValuesFrom { .. } => "DataAllValuesFrom",
        ClassExpression::DataHasValue { .. } => "DataHasValue",
        ClassExpression::DataMinCardinality { .. } => "DataMinCardinality",
        ClassExpression::DataMaxCardinality { .. } => "DataMaxCardinality",
        ClassExpression::DataExactCardinality { .. } => "DataExactCardinality",
    }
}
