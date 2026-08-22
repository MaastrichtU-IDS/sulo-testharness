//! Hermetic ingest of OWL ontologies, with axiom-loss detection on
//! both channels.
//!
//! Two independent things can silently discard axioms:
//!
//! 1. horned-owl's RDF reader, which has no `AllDisjointClasses`
//!    handling and puts unconsumed triples in `IncompleteParse`.
//! 2. rustdl's conversion to its internal IR, which reports
//!    `DroppedAxioms` for constructs it cannot represent.
//!
//! Both must be surfaced. An unreported loss means the harness
//! reasons over a weaker ontology than the one under test and says a
//! non-entailment holds when it may not.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use horned_owl::io::ParserConfiguration;
use horned_owl::io::rdf::reader::read as read_rdf;
use horned_owl::model::{
    ClassExpression, Component, DisjointClasses, DisjointUnion, EquivalentClasses, MutableOntology,
    RcStr,
};
use horned_owl::ontology::set::SetOntology;

/// A loaded ontology plus anything lost on the way in.
pub struct Loaded {
    pub ontology: SetOntology<RcStr>,
    /// Human-readable descriptions of dropped content. Empty is good.
    pub loss: Vec<String>,
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

    let (concrete, incomplete) = read_rdf(&mut reader, config).map_err(|e| LoadError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    let mut loss = Vec::new();
    if !incomplete.is_complete() {
        loss.push(format!(
            "parse: {} simple triples, {} bnode triples, {} bnode sequences, \
             {} orphan class expressions were not consumed \
             (horned-owl does not handle owl:AllDisjointClasses)",
            incomplete.simple.len(),
            incomplete.bnode.len(),
            incomplete.bnode_seq.len(),
            incomplete.class_expression.len(),
        ));
    }

    let mut ontology: SetOntology<RcStr> = concrete.into();
    lower_disjoint_unions(&mut ontology);

    // Second channel: what the reasoner's IR cannot represent.
    match owl_dl_reasoner::dropped_axioms(&ontology) {
        Ok(dropped) if !dropped.is_empty() => {
            let kinds: Vec<String> = dropped
                .by_kind()
                .iter()
                .map(|(k, n)| format!("{k} x{n}"))
                .collect();
            loss.push(format!(
                "conversion: {} dropped ({})",
                dropped.total(),
                kinds.join(", ")
            ));
        }
        Ok(_) => {}
        Err(e) => loss.push(format!("conversion: could not be checked: {e}")),
    }

    Ok(Loaded { ontology, loss })
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
