//! Hermetic ingest of OWL ontologies, with axiom-loss detection on
//! both channels.
//!
//! Two independent things can silently discard axioms:
//!
//! 1. horned-owl's RDF reader, which has no vocabulary entry for
//!    `owl:AllDisjointClasses` (nor for `owl:AllDisjointProperties`;
//!    contrast `owl:AllDifferent`, which it does handle) and puts the
//!    unconsumed triples in `IncompleteParse`. `owl:AllDisjointClasses`
//!    is recovered from those leftovers below, by `recover_all_disjoint_classes`;
//!    `owl:AllDisjointProperties` is not, and stays reported as loss.
//! 2. rustdl's conversion to its internal IR, which reports
//!    `DroppedAxioms` for constructs it cannot represent (for
//!    example, datatype facet restrictions and data-range unions).
//!    Real SULO carries exactly two such axioms, permanently, at the
//!    pinned rustdl tag: a reasoner expressivity gap, not a defect in
//!    SULO, and not something this loader can recover (the axioms
//!    reach rustdl intact; its own internal conversion is what cannot
//!    represent them). Loss matching that exact, known shape is
//!    tracked separately as `Loaded::baseline_loss`: expected, always
//!    surfaced, and never a reason to distrust a verdict. Anything
//!    else, on either channel, is `Loaded::loss` and downgrades
//!    exactly as before.
//!
//! Both must be surfaced. An unreported loss means the harness
//! reasons over a weaker ontology than the one under test and says a
//! non-entailment holds when it may not.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use horned_owl::io::ParserConfiguration;
use horned_owl::io::rdf::reader::{IncompleteParse, Term, read as read_rdf};
use horned_owl::model::{
    Build, ClassExpression, Component, DisjointClasses, DisjointUnion, EquivalentClasses,
    MutableOntology, RcStr,
};
use horned_owl::ontology::set::SetOntology;
use horned_owl::vocab::{OWL as VOwl, RDF as VRdf};

/// The literal `owl:AllDisjointClasses` IRI. horned-owl's vocabulary
/// has no enum variant for it (see `vocab.rs`: `AllDisjointProperties`
/// is listed, `AllDisjointClasses` is not), so the RDF reader parses
/// its `rdf:type` object as a plain `Term::Iri`, never as a
/// recognised vocabulary term.
const ALL_DISJOINT_CLASSES_IRI: &str = "http://www.w3.org/2002/07/owl#AllDisjointClasses";

/// The `owl_dl_reasoner::DroppedAxioms` kind string for the one known,
/// permanent, conversion-channel gap in real SULO at the pinned
/// rustdl tag: `sulo:TimeInstant`'s `owl:allValuesFrom [ owl:unionOf
/// (xsd:dateTime xsd:dateTimeStamp) ]` restriction, and
/// `sulo:InformationObject`'s `owl:allValuesFrom rdfs:Literal`
/// restriction on `sulo:hasValue`, both `SubClassOf` axioms whose
/// data range rustdl's IR cannot represent. This is a limitation of
/// the pinned reasoner's datatype support (`owl-dl-core` calls its
/// own data-range handling "Phase 3 minimal ... Phase 7 full concrete
/// domains", i.e. not yet built), not a defect in SULO's
/// axiomatisation.
const KNOWN_BASELINE_KIND: &str = "SubClassOf: unsupported data range";
/// How many axioms of `KNOWN_BASELINE_KIND` real SULO carries.
/// Anything other than exactly this many, of exactly this one kind,
/// is treated as loss beyond the baseline, not folded in: an
/// allowlist, not a threshold, so a *new* drop of the same kind is
/// just as loud as a drop of a different kind.
const KNOWN_BASELINE_COUNT: u64 = 2;

/// A loaded ontology plus anything lost on the way in.
pub struct Loaded {
    pub ontology: SetOntology<RcStr>,
    /// Human-readable descriptions of dropped content beyond the
    /// known baseline (see `KNOWN_BASELINE_KIND`). Empty is good:
    /// `suite::downgrade_for_loss` treats non-empty here as a reason
    /// to distrust "no proof was found". Never includes the known
    /// baseline drop; see `baseline_loss` for that.
    pub loss: Vec<String>,
    /// Human-readable descriptions of loss matching the known,
    /// permanent, pinned-reasoner baseline exactly. Non-empty here is
    /// EXPECTED for real, unmodified SULO and must never downgrade a
    /// verdict (`downgrade_for_loss` is never given this): the point
    /// is that it is still surfaced, not silently dropped, so the
    /// ontology's fidelity gap stays visible even though it is
    /// trusted not to matter.
    pub baseline_loss: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("cannot open {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot parse {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("unsupported extension on {path}, expected .ttl")]
    UnsupportedFormat { path: PathBuf },
}

/// Parse one Turtle file. Never touches the network.
pub fn load_file(path: &Path) -> Result<Loaded, LoadError> {
    if path.extension().and_then(|s| s.to_str()) != Some("ttl") {
        return Err(LoadError::UnsupportedFormat {
            path: path.to_path_buf(),
        });
    }

    let file = File::open(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::new(file);

    let mut config = ParserConfiguration::default();
    config.rdf.format = Some(oxrdfio::RdfFormat::Turtle);
    // Hermetic by construction: at this pinned horned-owl rev, the RDF
    // reader's resolve_imports only records each owl:imports triple as an
    // Import axiom. It never dereferences the IRI or touches the network,
    // so there is no `local_only`-style flag to set (that field does not
    // exist on this ParserConfiguration; a newer horned-owl adds one).

    let (concrete, mut incomplete) =
        read_rdf(&mut reader, config).map_err(|e| LoadError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

    let recovered_disjoint = recover_all_disjoint_classes(&mut incomplete);

    let mut loss = Vec::new();
    if !incomplete.is_complete() {
        loss.push(format!(
            "parse: {} simple triples, {} bnode triples, {} bnode sequences, \
             {} orphan class expressions were not consumed after recovering \
             {} owl:AllDisjointClasses axiom(s) (horned-owl has no vocabulary \
             entry for owl:AllDisjointClasses or owl:AllDisjointProperties; \
             remaining leftovers are owl:AllDisjointProperties, an ambiguous \
             owl:AllDisjointClasses shape recovery declined to guess at, or \
             another unhandled construct)",
            incomplete.simple.len(),
            incomplete.bnode.len(),
            incomplete.bnode_seq.len(),
            incomplete.class_expression.len(),
            recovered_disjoint.len(),
        ));
    }

    let mut ontology: SetOntology<RcStr> = concrete.into();
    for dc in recovered_disjoint {
        ontology.insert(dc);
    }
    lower_disjoint_unions(&mut ontology);

    let mut baseline_loss = Vec::new();

    // Second channel: what the reasoner's IR cannot represent. Split
    // against the known baseline (see `KNOWN_BASELINE_KIND`): an
    // EXACT match (this one kind, this one count, nothing else) is
    // expected and does not downgrade; anything else, downgrades
    // exactly as before.
    match owl_dl_reasoner::dropped_axioms(&ontology) {
        Ok(dropped) if !dropped.is_empty() => {
            let kinds_map = dropped.by_kind();
            let kinds: Vec<String> = kinds_map.iter().map(|(k, n)| format!("{k} x{n}")).collect();
            let message = format!(
                "conversion: {} dropped ({})",
                dropped.total(),
                kinds.join(", ")
            );

            let is_known_baseline = kinds_map.len() == 1
                && kinds_map.get(KNOWN_BASELINE_KIND) == Some(&KNOWN_BASELINE_COUNT);

            if is_known_baseline {
                eprintln!(
                    "sulo-testharness: warning: known baseline loss in {}: {message} \
                     (pinned-reasoner limitation, not a SULO defect; does not affect verdicts)",
                    path.display()
                );
                baseline_loss.push(message);
            } else {
                loss.push(message);
            }
        }
        Ok(_) => {}
        Err(e) => loss.push(format!("conversion: could not be checked: {e}")),
    }

    Ok(Loaded {
        ontology,
        loss,
        baseline_loss,
    })
}

/// True if a bnode-triple group is `<subj> a owl:AllDisjointClasses ;
/// owl:members <listHead>`, in either triple order (IncompleteParse's
/// bnode groups have no guaranteed triple order).
fn is_all_disjoint_classes_group(triples: &[[Term<RcStr>; 3]]) -> bool {
    let has_type = triples.iter().any(|t| {
        matches!(&t[1], Term::RDF(VRdf::Type))
            && matches!(&t[2], Term::Iri(iri) if iri.as_ref() == ALL_DISJOINT_CLASSES_IRI)
    });
    let has_members = triples
        .iter()
        .any(|t| matches!(&t[1], Term::OWL(VOwl::Members)) && matches!(&t[2], Term::BNode(_)));
    has_type && has_members
}

/// `Some(iris)` if every term in an RDF-list leftover is a plain IRI
/// (so it reads as a plausible class list), and there are at least
/// two of them (an `AllDisjointClasses` of fewer than two members
/// asserts nothing).
fn as_class_iri_list(terms: &[Term<RcStr>]) -> Option<Vec<String>> {
    if terms.len() < 2 {
        return None;
    }
    terms
        .iter()
        .map(|t| match t {
            Term::Iri(iri) => Some(iri.as_ref().to_string()),
            _ => None,
        })
        .collect()
}

/// Recover `owl:AllDisjointClasses` axioms that horned-owl's RDF
/// reader could not parse, from the leftovers it reports in
/// `IncompleteParse`.
///
/// horned-owl's vocabulary has no entry for `owl:AllDisjointClasses`
/// (unlike `owl:AllDifferent`, which its reader handles natively), so
/// `[] a owl:AllDisjointClasses ; owl:members ( C1 .. Cn )` never
/// reaches the parsed ontology: the type-and-members triples land in
/// `IncompleteParse::bnode`, grouped by their blank-node subject, and
/// the member list lands in `IncompleteParse::bnode_seq`. This walks
/// both, recognises that exact shape, and reinserts the axiom as
/// `DisjointClasses`, which is what `owl:AllDisjointClasses` means.
///
/// Matching a members list back to *its* declaration is not possible
/// from this public API: `IncompleteParse` groups bnode triples by
/// subject internally, but discards the subject id at the boundary
/// (`bnode: Vec<VPosTriple<A>>`), and does the same for the list
/// head's id (`bnode_seq: Vec<Vec<Term<A>>>`). So this only recovers
/// when the count of qualifying `AllDisjointClasses`-shaped groups
/// exactly equals the count of candidate (all-IRI, length >= 2)
/// leftover lists. Under that condition the ambiguity is harmless:
/// `AllDisjointClasses` is an anonymous n-ary axiom with no identity
/// beyond its member list, so however the groups and lists are
/// paired, the resulting axiom SET is the same. If the counts
/// disagree, this declines to guess and changes nothing, leaving the
/// leftovers to be reported as loss exactly as before.
fn recover_all_disjoint_classes(
    incomplete: &mut IncompleteParse<RcStr>,
) -> Vec<DisjointClasses<RcStr>> {
    let group_count = incomplete
        .bnode
        .iter()
        .filter(|g| is_all_disjoint_classes_group(g.vec_triple()))
        .count();

    let candidate_indices: Vec<usize> = incomplete
        .bnode_seq
        .iter()
        .enumerate()
        .filter(|(_, terms)| as_class_iri_list(terms).is_some())
        .map(|(i, _)| i)
        .collect();

    if group_count == 0 || group_count != candidate_indices.len() {
        return Vec::new();
    }

    incomplete
        .bnode
        .retain(|g| !is_all_disjoint_classes_group(g.vec_triple()));

    let build: Build<RcStr> = Build::new();
    let mut recovered = Vec::new();
    for &idx in candidate_indices.iter().rev() {
        let terms = incomplete.bnode_seq.remove(idx);
        let iris = as_class_iri_list(&terms).expect("filtered above");
        let members = iris
            .into_iter()
            .map(|iri| ClassExpression::Class(build.class(iri)))
            .collect();
        recovered.push(DisjointClasses(members));
    }

    recovered
}

/// Rewrite every `DisjointUnion(C, D1..Dn)` into the two axioms it
/// abbreviates: `EquivalentClasses(C, ObjectUnionOf(D1..Dn))` and
/// `DisjointClasses(D1..Dn)`. Returns how many were rewritten.
///
/// This is semantics-preserving, not a correction: `DisjointUnion(C,
/// D1..Dn)` means exactly `EquivalentClasses(C, ObjectUnionOf(D1..Dn))`
/// plus `DisjointClasses(D1..Dn)` by the OWL spec, so spelling it out
/// cannot assert anything the original axiom did not already mean. It
/// is kept as defense-in-depth so the harness does not depend on the
/// reasoner implementing `DisjointUnion`'s covering half natively.
///
/// That dependency is not hypothetical. Measured: at the pinned
/// `owl-dl-reasoner` tag v0.4.22, an individual typed `F` and
/// explicitly neither `A` nor `B` under `DisjointUnion(F, A, B)` is
/// correctly reported inconsistent even without this expansion; a
/// later rustdl working-tree build (14 commits past v0.4.22, in
/// commits about the pseudo-model prune silently losing entailments)
/// reported the same case consistent, i.e. it lost the covering half.
/// The regression is upstream of the version we pin, not present in
/// it, but this expansion stays so a future rustdl bump landing that
/// regression (or a similar one) does not silently weaken the
/// harness.
///
/// The original `DisjointUnion` is left in place: it is harmless and
/// keeps the ontology faithful to its source.
pub fn lower_disjoint_unions(onto: &mut SetOntology<RcStr>) -> usize {
    // Collect first: we cannot mutate while iterating.
    let unions: Vec<DisjointUnion<RcStr>> = onto
        .iter()
        .filter_map(|ac| match &ac.component {
            Component::DisjointUnion(du) => Some(du.clone()),
            _ => None,
        })
        .collect();

    let count = unions.len();

    for DisjointUnion(class, members) in unions {
        let union_of = ClassExpression::ObjectUnionOf(members.clone());
        onto.insert(EquivalentClasses(vec![
            ClassExpression::Class(class),
            union_of,
        ]));

        // A one-member union carries no disjointness; its covering
        // half is a plain equivalence, which is handled above
        // regardless of member count.
        if members.len() >= 2 {
            onto.insert(DisjointClasses(members));
        }
    }

    count
}

/// Fold `other`'s components into `base`.
pub fn merge(base: &mut SetOntology<RcStr>, other: SetOntology<RcStr>) {
    for component in other {
        base.insert(component);
    }
}
