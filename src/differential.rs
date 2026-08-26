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
//! # Which checks become questions, and why the positives are here
//!
//! Not every check needs an oracle of record. A positive assertion
//! rustdl PASSED is proved, and asking HermiT to confirm a proof is
//! not what a complete reasoner is for. Four kinds do need one, and
//! all four rest on the same thing: an ABSENCE of proof.
//!
//! 1. The consistency gate, in both directions.
//! 2. `not_entails` and `not_entails_manchester`, whose `UnrefutedPass`
//!    is by construction "no proof was found".
//! 3. `satisfiable_expr`, whose expected answer is also `UnrefutedPass`
//!    (see `oracle::check_satisfiable_expr`: UNSAT is the provable
//!    direction, so "satisfiable" is the untrusted one).
//! 4. **Positive assertions rustdl FAILED**, ruling 7. `oracle`'s own
//!    message for that Fail says "Incompleteness is a possible cause;
//!    the CI differential settles it". That `Fail` rests on absence of
//!    proof exactly as a negative `UnrefutedPass` does, and if the
//!    differential never asked about it that sentence would ship as a
//!    falsehood. Recognised by [`crate::oracle::NO_PROOF_MARKER`], the
//!    same constant `suite::downgrade_for_loss` matches on and the
//!    same constant the message is built from, so editing the wording
//!    cannot silently switch this off.
//!
//! Case 4 is also the most valuable direction. If HermiT finds no
//! proof either, the two reasoners AGREE and the `Fail` is a genuine
//! SULO regression. If HermiT DOES find the proof, that is a
//! `Divergence` meaning rustdl is incomplete on this query rather than
//! that SULO regressed, which is precisely the signal spec 5.3 calls
//! the most valuable either reasoner could produce. [`Origin`] carries
//! which of the four a question came from, so a report can say which.
//!
//! # Nothing asked is not everything agreed
//!
//! [`run_differential`] refuses a run that produced no questions
//! (exit 2), for the same reason `suite::discover` refuses a suite with
//! no cases. See [`no_questions_refusal`] for why that guard cannot
//! fire today and is kept anyway.

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

impl Answer {
    /// Does this answer rest on a proof the reasoner EXHIBITED, or on
    /// the absence of one?
    ///
    /// The three "true" answers are each a clash the reasoner actually
    /// found, which soundness vouches for and which incompleteness
    /// cannot manufacture. The three "false" answers are each "I
    /// looked and found nothing", which from a complete reasoner is a
    /// proof of absence and from an incomplete one is not.
    ///
    /// That distinction is what lets a divergence be reported in the
    /// right direction without the report having to know which
    /// question shape it came from.
    #[must_use]
    pub fn rests_on_a_proof(self) -> bool {
        match self {
            Answer::Inconsistent | Answer::Entailed | Answer::Unsatisfiable => true,
            Answer::Consistent | Answer::NotEntailed | Answer::Satisfiable => false,
        }
    }
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

/// Which of the four absence-resting check kinds a question came from.
///
/// Carried because a `Divergence` means something DIFFERENT depending
/// on it, and the reader cannot work out which from the two answers
/// alone. On a failing positive assertion, a divergence means rustdl
/// is incomplete and the `Fail` that `run` reported is NOT a SULO
/// regression; an agreement means it is one. On the gate or a negative
/// assertion there is no `Fail` in the `run` report to reinterpret.
///
/// See the module doc for the full list and why positives are in it at
/// all (ruling 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The consistency gate, in either direction.
    Gate,
    /// A negative assertion (`not_entails`, `not_entails_manchester`)
    /// or a `satisfiable_expr`. rustdl's expected answer here is an
    /// `UnrefutedPass`, which is by construction "no proof was found".
    Unrefuted,
    /// A POSITIVE assertion rustdl reported as a `Fail` carrying
    /// [`crate::oracle::NO_PROOF_MARKER`]. Ruling 7.
    FailingPositive,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Origin::Gate => "consistency gate",
            Origin::Unrefuted => "negative assertion",
            Origin::FailingPositive => "positive assertion rustdl could not prove",
        };
        f.write_str(s)
    }
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
    /// Which check kind produced it. See [`Origin`].
    pub origin: Origin,
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
/// The four absence-resting check kinds, per the module doc: the
/// consistency gate, the negative assertions, `satisfiable_expr`, and
/// (ruling 7) every POSITIVE assertion whose rustdl verdict is a
/// `Fail` carrying [`crate::oracle::NO_PROOF_MARKER`]. A positive
/// assertion that PASSED is proved, and asking HermiT to confirm a
/// proof is not what the oracle of record is for, so it yields no
/// question.
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
                origin: Origin::Gate,
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
                    origin: Origin::Unrefuted,
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
                            origin: Origin::Unrefuted,
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
                    origin: Origin::Unrefuted,
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
                origin: Origin::Unrefuted,
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
                origin: Origin::Unrefuted,
            },
            QuestionKind::Satisfiability,
            rustdl,
            probe,
        ));
    }

    // Ruling 7: every POSITIVE assertion rustdl failed to prove.
    //
    // Only the failing ones. A positive `Pass` is a proof, and the
    // oracle of record exists for the answers soundness cannot vouch
    // for, not to double-check the ones it can. `unproven` is what
    // decides membership, and it matches on the same constant
    // `oracle` builds the message from.

    // `entails:`, one question per claim in the fragment. The probe is
    // the same one `not_entails` uses; only the expected reading of
    // the answer differs, and that lives in the `run` report rather
    // than here.
    if let Some(fragment) = &case.entails
        && let Ok(claims) = parse_fragment(fragment, &pm)
    {
        for claim in &claims {
            let check = format!("{claim:?}");
            if !unproven(rustdl, &check) {
                continue;
            }
            let (kind, probe) = encode_claim(claim);
            out.push(build(
                Provenance {
                    case_id: case.id.clone(),
                    check,
                    asked: describe_claim(claim),
                    origin: Origin::FailingPositive,
                },
                kind,
                Some(kind_negative(kind)),
                probe,
            ));
        }
        // A fragment that does not parse produced an `Indeterminate`
        // in the `run` report, not a Fail, so there is no absence-
        // resting Fail here for HermiT to settle. The `not_entails`
        // arm above surfaces its own parse failure because THAT
        // fragment's questions are the whole reason the case is in the
        // differential at all.
    }

    // `entails_manchester:`.
    for s in &case.entails_manchester {
        let check = format!("{} subClassOf {}", s.sub_expr, s.sup_expr);
        if !unproven(rustdl, &check) {
            continue;
        }
        let probe = match (parse_ce(&s.sub_expr, &pm), parse_ce(&s.sup_expr, &pm)) {
            (Ok(sub), Ok(sup)) => subsumption_probe(&sub, &sup),
            (Err(e), _) | (_, Err(e)) => Probe::Unencodable(e.to_string()),
        };
        out.push(build(
            Provenance {
                case_id: case.id.clone(),
                check: check.clone(),
                asked: format!("does the ontology entail {check}?"),
                origin: Origin::FailingPositive,
            },
            QuestionKind::Entailment,
            Some(Answer::NotEntailed),
            probe,
        ));
    }

    // `instance_of_expr:`. The individual is expanded through the same
    // prefix map `suite::run_case` expands it through, because that is
    // the token the check name was built from AND the individual the
    // probe has to be about. Using the raw CURIE would miss the check
    // and ask HermiT about an IRI nobody meant.
    for i in &case.instance_of_expr {
        let Ok(individual) = crate::prefixes::expand(&pm, &i.individual) else {
            // An unexpandable prefix was already an `Indeterminate` in
            // the `run` report, not an absence-resting Fail.
            continue;
        };
        let check = format!("{individual} instanceOf {}", i.expr);
        if !unproven(rustdl, &check) {
            continue;
        }
        let probe = match parse_ce(&i.expr, &pm) {
            Ok(ce) => individual_complement_probe(&individual, &ce),
            Err(e) => Probe::Unencodable(e.to_string()),
        };
        out.push(build(
            Provenance {
                case_id: case.id.clone(),
                check: check.clone(),
                asked: format!(
                    "does the ontology entail that {individual} is a {}?",
                    i.expr
                ),
                origin: Origin::FailingPositive,
            },
            QuestionKind::Entailment,
            Some(Answer::NotEntailed),
            probe,
        ));
    }

    // `unsatisfiable:`. `run_case` turns each into a
    // `Claim::Unsatisfiable` with a positive expectation, so a Fail
    // here is "the reasoner could not prove this class empty".
    for class in &case.unsatisfiable {
        let Ok(iri) = crate::prefixes::expand(&pm, class) else {
            continue;
        };
        let claim = Claim::Unsatisfiable { class: iri };
        let check = format!("{claim:?}");
        if !unproven(rustdl, &check) {
            continue;
        }
        let (kind, probe) = encode_claim(&claim);
        out.push(build(
            Provenance {
                case_id: case.id.clone(),
                check,
                asked: describe_claim(&claim),
                origin: Origin::FailingPositive,
            },
            kind,
            Some(kind_negative(kind)),
            probe,
        ));
    }

    out
}

/// Did rustdl report the check named `name` as a `Fail` resting on an
/// ABSENCE of proof?
///
/// Ruling 7's membership test. Matched against
/// [`crate::oracle::NO_PROOF_MARKER`], the same constant `oracle`
/// builds the message from and the same one
/// `suite::downgrade_for_loss` matches on, so editing the wording can
/// never silently empty the positive half of the question set.
///
/// A `Fail` WITHOUT the marker (a negative expectation that was
/// refuted, an unsatisfiable `satisfiable_expr`) rests on a clash the
/// reasoner exhibited, which soundness vouches for; it needs no oracle
/// of record and is not included. Neither is a missing check, an
/// `Indeterminate`, or a `Pass`.
fn unproven(checks: &[CheckOutcome], name: &str) -> bool {
    checks.iter().any(|c| {
        c.name == name
            && matches!(&c.verdict, Verdict::Fail(msg) if msg.contains(crate::oracle::NO_PROOF_MARKER))
    })
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

// -------------------------------------------------------------------
// Driving a whole suite, and reporting what came back.
// -------------------------------------------------------------------

/// What one differential run was asked to do.
pub struct DifferentialOptions<'a> {
    /// Directory of case manifests, walked recursively.
    pub suite: &'a Path,
    /// The ontology under test. REQUIRED here, unlike `run`'s
    /// optional `--ontology`: the differential's whole purpose is to
    /// ask two reasoners about one ontology, and a suite where every
    /// case names its own is not a shape this subcommand needs to
    /// support.
    pub ontology: &'a Path,
    /// The ROBOT jar.
    pub robot: &'a Path,
    /// Substring matched against the manifest PATH, exactly as
    /// `run --filter` is.
    pub filter: Option<&'a str>,
    /// Where probe ontologies, merged files and ROBOT's own stdout and
    /// stderr are written, one directory per question. Kept rather
    /// than deleted: a divergence is only actionable if the reader can
    /// open the probe that produced it.
    pub workdir: &'a Path,
}

/// One question, asked and compared.
///
/// [`Comparison::Agree`] deliberately carries no provenance (an
/// agreement is about the answer, not about where it came from), so
/// this pairs every comparison with the question that produced it and
/// a report can name all three outcomes uniformly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub provenance: Provenance,
    pub comparison: Comparison,
}

/// The result of one whole-suite differential run.
pub enum DifferentialOutcome {
    /// Every selected case was run through both reasoners. Guaranteed
    /// non-empty: see [`no_questions_refusal`].
    Ran(Vec<Asked>),
    /// A configuration error: exit 2, and NOT a statement about either
    /// reasoner. Carries the message to print.
    Config(String),
}

/// Ruling 8's refusal message.
///
/// # This guard cannot fire today, and is kept anyway
///
/// Stated plainly rather than left for someone to discover: as
/// [`questions`] is written, EVERY case yields at least its
/// consistency-gate question, and [`crate::suite::select`] already
/// refuses a selection with no cases. So there is no input to the
/// `differential` subcommand that reaches this message.
///
/// The plan's ruling 8 motivates it with "a case carrying only
/// positive assertions that all passed yields zero questions", which
/// was true of an earlier draft of `questions` and is not true of this
/// one. The guard is kept because the property it defends is real and
/// cheap to lose: the day someone makes the gate question conditional
/// (on a case having no other questions, say, or on the ontology being
/// unchanged since the last run), a filtered run would silently
/// produce zero questions and this function is what stands between
/// that and a green cross-check that asked nothing.
///
/// `tests/differential.rs` pins BOTH halves: that the message is
/// produced for an empty question list, and that no case in the real
/// suite currently produces one.
#[must_use]
pub fn no_questions_refusal(suite: &Path, cases: usize, filter: Option<&str>) -> String {
    let narrowed = match filter {
        Some(f) => format!(" (narrowed by --filter {f:?})"),
        None => String::new(),
    };
    format!(
        "the differential asked NO questions at all over the {cases} selected case(s) \
         under {}{narrowed}, so it would report a green cross-check having put nothing \
         to either reasoner. Nothing asked is not everything agreed. A case yields \
         questions from its consistency gate, from every negative assertion, and from \
         every positive assertion rustdl could not prove; a selection producing none of \
         those is a configuration error.",
        suite.display()
    )
}

/// Discover, run both reasoners, and compare, over a whole suite.
///
/// Deferred cases are INCLUDED, unconditionally and with no flag to
/// turn that off. They are deferred in the `run` path precisely
/// because this is their oracle of record (`suite::DEFERRED_TAG`), so
/// a differential that skipped them would leave them checked by
/// nothing, which is the state this subcommand exists to end.
///
/// rustdl is re-run here rather than its results being passed in: the
/// comparison needs the per-check outcomes for the SAME ontology
/// HermiT is about to be asked about, and taking them from a separate
/// invocation would let the two halves drift apart silently.
pub fn run_differential(opts: &DifferentialOptions) -> DifferentialOutcome {
    // Checked before any case is loaded, so a bad `--robot` is one
    // clear message rather than one ROBOT `Error` per question. An
    // unrunnable jar would otherwise be `Indeterminate` everywhere,
    // which is exit 3 and honest but much harder to read.
    if !opts.robot.is_file() {
        return DifferentialOutcome::Config(format!(
            "--robot {} is not a readable file: pass the path to a ROBOT jar (1.9.7 is \
             what the encodings in this module were measured against). The differential \
             needs a JVM and that jar; neither is on this harness's default path.",
            opts.robot.display()
        ));
    }

    let selected = match crate::suite::select(opts.suite, Some(opts.ontology), opts.filter) {
        Ok(s) => s,
        Err(msg) => return DifferentialOutcome::Config(msg),
    };

    let mut asked = Vec::new();
    for (case, _path) in &selected {
        let result = crate::suite::run_case(case, opts.ontology);
        for (i, question) in questions(case, opts.ontology, &result.checks)
            .iter()
            .enumerate()
        {
            let dir = opts.workdir.join(format!("{}-{i}", sanitise(&case.id)));
            let hermit = ask(opts.robot, question, &dir);
            asked.push(Asked {
                provenance: question.provenance.clone(),
                comparison: compare(question, &hermit),
            });
        }
    }

    if asked.is_empty() {
        return DifferentialOutcome::Config(no_questions_refusal(
            opts.suite,
            selected.len(),
            opts.filter,
        ));
    }

    DifferentialOutcome::Ran(asked)
}

/// A case id, made safe to use as one path component.
fn sanitise(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The process exit code for a whole differential run.
///
/// Divergence outranks Indeterminate, per ruling 2: a run that found
/// the two reasoners disagreeing has produced its headline result, and
/// burying it under a 3 because some other question also went
/// unanswered would hide the one thing this job exists to report.
///
/// `0` requires that EVERY question was asked and every answer
/// matched. It is never reached over an empty list, because
/// [`run_differential`] refuses one.
#[must_use]
pub fn differential_exit_code(asked: &[Asked]) -> i32 {
    if asked
        .iter()
        .any(|a| matches!(a.comparison, Comparison::Divergence { .. }))
    {
        5
    } else if asked
        .iter()
        .any(|a| matches!(a.comparison, Comparison::Indeterminate { .. }))
    {
        3
    } else {
        0
    }
}

/// What a divergence MEANS, in the reader's terms, naming the outlier.
///
/// HermiT is sound and complete for OWL 2 DL and rustdl is sound but
/// incomplete, so in either direction rustdl is the outlier. Which
/// DEFECT it is differs, and so does what the divergence implies for
/// the `run` report:
///
/// * rustdl found no proof and HermiT exhibited one: an
///   INCOMPLETENESS. Harmless to soundness, and on a failing positive
///   assertion it means the `Fail` the `run` subcommand reported is
///   the incompleteness rather than a SULO regression. Spec 5.3 calls
///   that the most valuable signal either reasoner could produce.
/// * rustdl claims a proof HermiT refutes: an UNSOUNDNESS. HermiT's
///   completeness makes its "no proof" a proof of absence, so this is
///   the alarming direction and every verdict from the same code path
///   is suspect.
///
/// The third possibility is named too, because omitting it would be
/// the overstatement this project exists to avoid: the two reasoners
/// may have been asked DIFFERENT questions, if a probe encoding is
/// wrong. The probe is on disk, so the reader can check.
#[must_use]
pub fn explain_divergence(origin: Origin, rustdl: Answer, hermit: Answer) -> String {
    let direction = if !rustdl.rests_on_a_proof() && hermit.rests_on_a_proof() {
        format!(
            "rustdl is the outlier: HermiT exhibited a proof ({hermit}) that rustdl did \
             not find ({rustdl}). That is a rustdl INCOMPLETENESS on this query, not a \
             finding about the ontology."
        )
    } else if rustdl.rests_on_a_proof() && !hermit.rests_on_a_proof() {
        format!(
            "rustdl is the outlier, in the alarming direction: it claims a proof \
             ({rustdl}) that HermiT refutes ({hermit}). HermiT is complete for OWL 2 DL, \
             so its \"no proof\" is a proof of absence, and this reads as a rustdl \
             UNSOUNDNESS. Every verdict resting on the same code path is suspect."
        )
    } else {
        // Two answers that are neither each other nor opposite sides
        // of the proof/absence line means the two reasoners were asked
        // questions in different answer spaces, which is a harness
        // bug, not a reasoner one.
        format!(
            "rustdl said {rustdl} and HermiT said {hermit}, which are not two answers to \
             one question. That is a defect in this harness's own question building, not \
             in either reasoner."
        )
    };

    let consequence = match origin {
        Origin::FailingPositive => {
            " The `run` subcommand reports this check as a Fail resting on absence of \
             proof; this divergence settles that message, and it settles it AGAINST a \
             SULO regression."
        }
        Origin::Gate => {
            " This is the case's consistency gate, so every other check in \
             the case was judged against a consistency verdict the two reasoners do not \
             share."
        }
        Origin::Unrefuted => {
            " The `run` subcommand reports this check as an \
             UnrefutedPass, a verdict that rests on exactly the absence of proof this \
             divergence contradicts."
        }
    };

    format!(
        "{direction}{consequence} If neither reading fits, check the probe ontology this \
         question was built from: a probe that asks a different question than rustdl was \
         asked would show up here too."
    )
}

/// What an AGREEMENT means, where it means more than "both said the
/// same thing".
///
/// Only ruling 7's positives get a note. Two reasoners agreeing that a
/// positive assertion has no proof, one of them complete, is a proof
/// of absence: the `Fail` the `run` subcommand reported is real. That
/// is worth saying, because `oracle`'s own message for that Fail
/// explicitly defers the question to this job.
#[must_use]
pub fn explain_agreement(origin: Origin, answer: Answer) -> Option<String> {
    match origin {
        Origin::FailingPositive if !answer.rests_on_a_proof() => Some(
            "both reasoners found no proof, and HermiT is complete for OWL 2 DL, so this \
             is a proof of absence: the Fail the `run` subcommand reports for this check \
             is a genuine regression in the ontology, not rustdl incompleteness."
                .to_string(),
        ),
        // A failing positive on which HermiT ALSO found the proof
        // would not be an agreement (rustdl's answer here is always
        // the absence one), so this arm is unreachable and says
        // nothing rather than guessing.
        _ => None,
    }
}

/// Counts, for the summary line and the JSON payload.
#[must_use]
fn tally(asked: &[Asked]) -> (usize, usize, usize) {
    let mut agreed = 0;
    let mut diverged = 0;
    let mut indeterminate = 0;
    for a in asked {
        match a.comparison {
            Comparison::Agree { .. } => agreed += 1,
            Comparison::Divergence { .. } => diverged += 1,
            Comparison::Indeterminate { .. } => indeterminate += 1,
        }
    }
    (agreed, diverged, indeterminate)
}

/// The human-readable differential report.
///
/// Divergences first and in full, because they are the only reason
/// this job exists. Indeterminates next, because a question that was
/// never answered is not a question that agreed. Agreements last and
/// one line each: they are the bulk of the output and the least
/// informative part of it, but they are printed rather than counted so
/// that a reader can see WHICH questions were asked, and notice one
/// they expected and cannot find.
#[must_use]
pub fn render(asked: &[Asked], opts: &DifferentialOptions) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "differential: rustdl vs HermiT\n  suite:    {}\n  ontology: {}\n  robot:    {}\n  probes:   {}\n\n",
        opts.suite.display(),
        opts.ontology.display(),
        opts.robot.display(),
        opts.workdir.display()
    ));

    for a in asked {
        if let Comparison::Divergence {
            question,
            rustdl,
            hermit,
        } = &a.comparison
        {
            out.push_str(&format!(
                "DIVERGENCE  {} / {}\n    from:   {}\n    asked:  {}\n    rustdl: {}\n    HermiT: {}\n    {}\n\n",
                question.case_id,
                question.check,
                question.origin,
                question.asked,
                rustdl,
                hermit,
                explain_divergence(question.origin, *rustdl, *hermit)
            ));
        }
    }

    for a in asked {
        if let Comparison::Indeterminate { question, reason } = &a.comparison {
            out.push_str(&format!(
                "INDETERMINATE  {} / {}\n    asked:  {}\n    {}\n\n",
                question.case_id, question.check, question.asked, reason
            ));
        }
    }

    for a in asked {
        if let Comparison::Agree { answer } = &a.comparison {
            out.push_str(&format!(
                "agree  {} / {}: both answered {}\n",
                a.provenance.case_id, a.provenance.check, answer
            ));
            if let Some(note) = explain_agreement(a.provenance.origin, *answer) {
                out.push_str(&format!("    {note}\n"));
            }
        }
    }

    let (agreed, diverged, indeterminate) = tally(asked);
    out.push_str(&format!(
        "\n{} question(s): {agreed} agreed, {diverged} diverged, {indeterminate} \
         indeterminate\n",
        asked.len()
    ));
    out
}

/// The same report as JSON.
#[must_use]
pub fn render_json(asked: &[Asked], opts: &DifferentialOptions) -> String {
    let questions: Vec<serde_json::Value> = asked
        .iter()
        .map(|a| {
            let origin = match a.provenance.origin {
                Origin::Gate => "gate",
                Origin::Unrefuted => "unrefuted",
                Origin::FailingPositive => "failing_positive",
            };
            let mut row = serde_json::json!({
                "case": a.provenance.case_id,
                "check": a.provenance.check,
                "asked": a.provenance.asked,
                "origin": origin,
            });
            let map = row.as_object_mut().expect("json! built an object");
            match &a.comparison {
                Comparison::Agree { answer } => {
                    map.insert("outcome".into(), "agree".into());
                    map.insert("rustdl".into(), answer.to_string().into());
                    map.insert("hermit".into(), answer.to_string().into());
                    if let Some(note) = explain_agreement(a.provenance.origin, *answer) {
                        map.insert("note".into(), note.into());
                    }
                }
                Comparison::Divergence {
                    question,
                    rustdl,
                    hermit,
                } => {
                    map.insert("outcome".into(), "divergence".into());
                    map.insert("rustdl".into(), rustdl.to_string().into());
                    map.insert("hermit".into(), hermit.to_string().into());
                    map.insert(
                        "note".into(),
                        explain_divergence(question.origin, *rustdl, *hermit).into(),
                    );
                }
                Comparison::Indeterminate { reason, .. } => {
                    map.insert("outcome".into(), "indeterminate".into());
                    map.insert("reason".into(), reason.clone().into());
                }
            }
            row
        })
        .collect();

    let (agreed, diverged, indeterminate) = tally(asked);
    let payload = serde_json::json!({
        "suite": opts.suite.display().to_string(),
        "ontology": opts.ontology.display().to_string(),
        "robot": opts.robot.display().to_string(),
        "probes": opts.workdir.display().to_string(),
        "summary": {
            "questions": asked.len(),
            "agreed": agreed,
            "diverged": diverged,
            "indeterminate": indeterminate,
            "exit_code": differential_exit_code(asked),
        },
        "questions": questions,
    });
    serde_json::to_string_pretty(&payload).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}
