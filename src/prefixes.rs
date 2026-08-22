//! The single prefix map every CURIE in the suite resolves against.
//!
//! One map serves three consumers: Turtle fragments (prepended as
//! `@prefix` lines), Manchester expressions (passed to
//! `parse_class_expression`, which resolves CURIEs natively), and
//! `expect_rows` values. Keeping one map means an author learns one
//! set of bindings.

use std::collections::BTreeMap;

use curie::PrefixMapping;

#[derive(Debug, thiserror::Error)]
pub enum PrefixError {
    #[error("prefix '{prefix}' is not bound; declare it in the suite or case prefixes")]
    Unbound { prefix: String },
    #[error("'{0}' is neither a CURIE nor a full <IRI>")]
    Malformed(String),
}

/// Always-present bindings: `sulo:` plus the standard vocabularies.
#[must_use]
pub fn base_mapping() -> PrefixMapping {
    let mut pm = PrefixMapping::default();
    // add_prefix only errors on a reserved prefix name; none of these are.
    let pairs = [
        ("sulo", "https://w3id.org/sulo/"),
        ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
        ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
        ("owl", "http://www.w3.org/2002/07/owl#"),
        ("xsd", "http://www.w3.org/2001/XMLSchema#"),
        ("skos", "http://www.w3.org/2004/02/skos/core#"),
    ];
    for (p, iri) in pairs {
        let _ = pm.add_prefix(p, iri);
    }
    pm
}

/// Layer `overrides` on top of `base`. Later bindings win.
#[must_use]
pub fn with_overrides(
    base: &PrefixMapping,
    overrides: &BTreeMap<String, String>,
) -> PrefixMapping {
    let mut pm = base.clone();
    for (prefix, iri) in overrides {
        let _ = pm.add_prefix(prefix, iri);
    }
    pm
}

/// Expand a CURIE or unwrap a full `<IRI>`.
pub fn expand(pm: &PrefixMapping, token: &str) -> Result<String, PrefixError> {
    let t = token.trim();

    if let Some(inner) = t.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        return Ok(inner.to_string());
    }

    match pm.expand_curie_string(t) {
        Ok(iri) => Ok(iri),
        Err(_) => {
            let prefix = t.split(':').next().unwrap_or(t).to_string();
            if t.contains(':') {
                Err(PrefixError::Unbound { prefix })
            } else {
                Err(PrefixError::Malformed(t.to_string()))
            }
        }
    }
}
