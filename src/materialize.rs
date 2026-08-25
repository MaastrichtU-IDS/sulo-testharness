//! Build the oxigraph store that the competency-question path queries.
//!
//! Spec section 8 step 6 defines the store's contents exactly, in this
//! order, because leaving it vague would let two implementations build
//! different stores and have the same competency question pass on one
//! and fail on the other:
//!
//! 1. Every asserted triple from the ontology and data files.
//! 2. Every inferred class assertion, from `instances` over every named
//!    class (the full closure, not most-specific types).
//! 3. Every inferred object and data property assertion, from
//!    `property-values`.
//! 4. The reflexive self-loops `x isPartOf x` and `x hasPart x` for
//!    every named individual, which `property-values` omits.
//!
//! Named individuals only: blank nodes are outside
//! `inferred_object_property_values` coverage, so suite data uses
//! skolemised IRIs (spec section 8 step 6). This module never queries
//! or asserts anything about an anonymous individual.
//!
//! ## Boundedness
//!
//! Every reasoner call this crate makes on the per-case path is meant to
//! be bounded; the precedent is a real 24-minute-30-second hang (see
//! `oracle.rs`'s module doc). Three calls in this module were weighed
//! against that precedent:
//!
//! * Step 3 (`inferred_object_property_values`,
//!   `inferred_data_property_values`) is bounded: the former takes a
//!   `pair_deadline` directly, honoured below; the latter is a pure
//!   structural passthrough with no tableau search, the same reasoning
//!   `oracle.rs` already documents for why it carries no deadline
//!   parameter.
//! * Step 2 (`instances_of`, one call per named class) has NO deadline
//!   parameter, and is therefore this module's one unbounded reasoner
//!   call, alongside the consistency gate in `suite::run_case` and
//!   `classify` in `golden::closure`. It is kept anyway, deliberately,
//!   for a measured reason rather than an oversight:
//!
//!   Two approaches were measured head to head on real SULO merged with
//!   `tests/fixtures/parts.ttl` (7 named individuals, 17 named classes):
//!   `instances_of` once per named class took 11.9ms total; the bounded
//!   alternative (`entailed_via_satisfiability_probe`, one call per
//!   `(individual, class)` pair, cloning the ontology each time) took
//!   80.9ms for the same closure, roughly 7x slower, and its cost is
//!   O(individuals x classes) where `instances_of`'s is O(classes). SULO
//!   itself declares zero named individuals (it is pure TBox), so a
//!   case's individual count is entirely driven by its data file, which
//!   for every fixture in this repository is small; the O(individuals)
//!   factor the bounded alternative pays for is not a cost `instances_of`
//!   carries at all.
//!
//!   `instances_of` is not unconditionally unbounded in the way the
//!   24-minute hang was: on the fast path (real SULO's TBox fragment
//!   qualifies) it is a saturation-only closure computation with no
//!   tableau search at all; only off that fast path does it fall back to
//!   a per-individual tableau probe bounded by an env-var-configurable
//!   timeout (`RUSTDL_REALIZE_PAIR_TIMEOUT_MS`, default 750ms) that this
//!   module does not touch, matching `oracle.rs`'s documented refusal to
//!   mutate that variable process-wide. The 24-minute hang that motivates
//!   every OTHER bound in this crate came from a structurally different
//!   call: `class_expression_instances`, which probes a class expression
//!   extended with negation and a nominal, pushing the ontology out of
//!   the fast fragment entirely. A plain named-class `instances_of` call
//!   over the unmodified ontology, as used here, never does that.
//!
//!   If a future SULO revision or case fixture ever makes this slow, the
//!   fix is `instances_of_saturation_only` (a sound under-approximation
//!   with no tableau fallback at all) traded against completeness, not a
//!   silent timeout; that trade should be made deliberately when it is
//!   needed, not preemptively here.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use horned_owl::model::{Component, Individual, RcStr};
use horned_owl::ontology::component_mapped::RcComponentMappedOntology;
use horned_owl::ontology::set::SetOntology;
use oxigraph::io::RdfFormat;
use oxigraph::model::{GraphName, Literal, NamedNode, Quad};
use oxigraph::store::Store;

/// `sulo:isPartOf` and `sulo:hasPart`, the two properties whose
/// reflexive self-loop `property-values` omits. Spec section 8 step 6.
const IS_PART_OF: &str = "https://w3id.org/sulo/isPartOf";
const HAS_PART: &str = "https://w3id.org/sulo/hasPart";

/// Why `materialize` could not build the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterializeError {
    /// A reasoner call failed outright (not a timeout).
    Reasoner(String),
    /// The oxigraph store rejected a load or an insert.
    Store(String),
    /// `deadline` was exceeded before the closure finished.
    Timeout,
}

impl std::fmt::Display for MaterializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaterializeError::Reasoner(msg) => write!(f, "{msg}"),
            MaterializeError::Store(msg) => write!(f, "{msg}"),
            MaterializeError::Timeout => write!(f, "materialisation exceeded its time budget"),
        }
    }
}

/// Every named class the ontology declares. "Named class" means an
/// explicit `Declaration(Class(...))`, not `owl:Thing` / `owl:Nothing`:
/// spec section 8 step 6 says "instances over all 17 named classes",
/// and querying the two OWL builtins would be wasted, off-spec work.
fn named_classes(onto: &SetOntology<RcStr>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for ac in onto.iter() {
        if let Component::DeclareClass(horned_owl::model::DeclareClass(c)) = &ac.component {
            out.insert(c.0.to_string());
        }
    }
    out
}

fn named_iri(i: &Individual<RcStr>) -> Option<String> {
    match i {
        Individual::Named(ni) => Some(ni.0.to_string()),
        Individual::Anonymous(_) => None,
    }
}

/// Every named individual the ontology mentions, gathered the same way
/// `oracle::Declared` gathers them: an individual counts whether or not
/// it carries an explicit `DeclareNamedIndividual`, because RDF-serialised
/// data routinely omits `a owl:NamedIndividual` (`tests/fixtures/parts.ttl`
/// does). Blank nodes are never collected: spec section 8 step 6 restricts
/// this whole store to named individuals.
fn named_individuals(onto: &SetOntology<RcStr>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for ac in onto.iter() {
        match &ac.component {
            Component::DeclareNamedIndividual(horned_owl::model::DeclareNamedIndividual(ni)) => {
                out.insert(ni.0.to_string());
            }
            Component::ClassAssertion(horned_owl::model::ClassAssertion { i, .. }) => {
                out.extend(named_iri(i));
            }
            Component::ObjectPropertyAssertion(horned_owl::model::ObjectPropertyAssertion {
                from,
                to,
                ..
            })
            | Component::NegativeObjectPropertyAssertion(
                horned_owl::model::NegativeObjectPropertyAssertion { from, to, .. },
            ) => {
                out.extend(named_iri(from));
                out.extend(named_iri(to));
            }
            Component::DataPropertyAssertion(horned_owl::model::DataPropertyAssertion {
                from,
                ..
            })
            | Component::NegativeDataPropertyAssertion(
                horned_owl::model::NegativeDataPropertyAssertion { from, .. },
            ) => {
                out.extend(named_iri(from));
            }
            Component::SameIndividual(horned_owl::model::SameIndividual(is))
            | Component::DifferentIndividuals(horned_owl::model::DifferentIndividuals(is)) => {
                out.extend(is.iter().filter_map(named_iri));
            }
            _ => {}
        }
    }
    out
}

/// Load every asserted triple from `onto` into `store`. Serialises the
/// ontology through horned-owl's own RDF writer (`write_to_rdf_format`
/// with `"ttl"`, which despite the name emits N-Triples: see that
/// function's match arm) rather than walking axiom variants by hand, so
/// this cannot silently miss an axiom kind horned-owl itself knows how
/// to render as RDF.
fn load_asserted(onto: &SetOntology<RcStr>, store: &Store) -> Result<(), MaterializeError> {
    let mapped: RcComponentMappedOntology = onto.clone().into();
    let bytes = horned_owl::io::rdf::writer::write_to_rdf_format(Vec::new(), &mapped, "ttl")
        .map_err(|e| MaterializeError::Reasoner(e.to_string()))?;
    store
        .load_from_slice(RdfFormat::NTriples, bytes.as_slice())
        .map_err(|e| MaterializeError::Store(e.to_string()))
}

/// `deadline` minus however long has elapsed since `start`, or
/// `Err(Timeout)` if that is zero or negative. Checked before every
/// reasoner call below so a slow step is caught at the next boundary
/// rather than let run unbounded.
fn remaining(start: Instant, deadline: Duration) -> Result<Duration, MaterializeError> {
    let elapsed = start.elapsed();
    if elapsed >= deadline {
        return Err(MaterializeError::Timeout);
    }
    Ok(deadline - elapsed)
}

fn insert_object_triple(store: &Store, s: &str, p: &str, o: &str) -> Result<(), MaterializeError> {
    let quad = Quad::new(
        NamedNode::new(s).map_err(|e| MaterializeError::Store(e.to_string()))?,
        NamedNode::new(p).map_err(|e| MaterializeError::Store(e.to_string()))?,
        NamedNode::new(o).map_err(|e| MaterializeError::Store(e.to_string()))?,
        GraphName::DefaultGraph,
    );
    store
        .insert(&quad)
        .map_err(|e| MaterializeError::Store(e.to_string()))?;
    Ok(())
}

/// Build the store spec section 8 step 6 describes: every asserted
/// triple, plus the reasoner's inferred closure, plus the reflexive
/// self-loops the closure omits. `deadline` bounds the whole call: it
/// is checked before every reasoner call, and an exhausted budget
/// returns `Err(MaterializeError::Timeout)` rather than letting the
/// next step run.
pub fn materialize(
    onto: &SetOntology<RcStr>,
    deadline: Duration,
) -> Result<Store, MaterializeError> {
    let start = Instant::now();
    let store = Store::new().map_err(|e| MaterializeError::Store(e.to_string()))?;

    // Step 1: every asserted triple. No reasoner call, but still
    // subject to the overall budget: an empty deadline must not
    // silently succeed just because this step alone is cheap.
    remaining(start, deadline)?;
    load_asserted(onto, &store)?;

    let classes = named_classes(onto);
    let individuals = named_individuals(onto);

    // Step 2: every inferred class assertion, full closure. See the
    // module doc for why `instances_of` (unbounded; no deadline
    // parameter exists) was chosen over the bounded per-pair
    // alternative after measuring both.
    for class in &classes {
        remaining(start, deadline)?;
        let instances = owl_dl_reasoner::instances_of(onto, class)
            .map_err(|e| MaterializeError::Reasoner(e.to_string()))?;
        for individual in &instances {
            insert_object_triple(
                &store,
                individual,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                class,
            )?;
        }
    }

    // Step 3: every inferred object and data property assertion.
    let object_deadline = remaining(start, deadline)?;
    let object_values =
        owl_dl_reasoner::inferred_object_property_values(onto, Some(object_deadline))
            .map_err(|e| MaterializeError::Reasoner(e.to_string()))?;
    // `inferred_object_property_values` does not itself signal a
    // timeout via `Err`: an individual candidate probe that times out
    // is silently treated as "not entailed" (see `property_values.rs`).
    // The wall-clock check below is what catches that here, exactly
    // the same pattern `oracle::holds_with_deadline` already uses for
    // this same call.
    if start.elapsed() >= deadline {
        return Err(MaterializeError::Timeout);
    }
    for (s, p, o) in object_values.triples() {
        insert_object_triple(&store, s, p, o)?;
    }

    remaining(start, deadline)?;
    // No deadline parameter here: `inferred_data_property_values` is a
    // pure structural passthrough over asserted data-property triples,
    // not a tableau search, so it carries no comparable blow-up risk;
    // see `oracle.rs`'s identical call for the same reasoning.
    let data_values = owl_dl_reasoner::inferred_data_property_values(onto)
        .map_err(|e| MaterializeError::Reasoner(e.to_string()))?;
    for (s, p, lexical, datatype) in data_values.quads() {
        let subject = NamedNode::new(s).map_err(|e| MaterializeError::Store(e.to_string()))?;
        let predicate = NamedNode::new(p).map_err(|e| MaterializeError::Store(e.to_string()))?;
        let dt = NamedNode::new(datatype).map_err(|e| MaterializeError::Store(e.to_string()))?;
        let quad = Quad::new(
            subject,
            predicate,
            Literal::new_typed_literal(lexical, dt),
            GraphName::DefaultGraph,
        );
        store
            .insert(&quad)
            .map_err(|e| MaterializeError::Store(e.to_string()))?;
    }

    // Step 4: the reflexive self-loops `property-values` omits, for
    // every named individual. Spec section 8 step 6; see the module
    // doc's opening paragraph.
    remaining(start, deadline)?;
    for individual in &individuals {
        insert_object_triple(&store, individual, IS_PART_OF, individual)?;
        insert_object_triple(&store, individual, HAS_PART, individual)?;
    }

    Ok(store)
}
