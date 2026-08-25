//! Run one competency question against a materialised store.
//!
//! A CQ is a positive assertion about what the ontology answers, run
//! against the closure `materialize` built, not a proof search. So the
//! asymmetry the rest of this crate carries between `Pass` and
//! `UnrefutedPass` does not apply here: when the actual rows match
//! `expect_rows`, that is a real answer straight out of the store, not
//! the absence of a refutation, and gets a trustworthy `Pass`. Anything
//! that stops the comparison from happening at all (a missing query
//! file, a syntax error, a query that is not a `SELECT`, an
//! `expect_rows` token `rows::parse_expected` rejects) is the case
//! author's configuration error, not an ontology regression, so it is
//! `Indeterminate`, never a silent `Fail`.

use std::collections::BTreeMap;
use std::path::Path;

use curie::PrefixMapping;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use oxrdf::Term;

use crate::manifest::CqSpec;
use crate::rows::{self, Expected};
use crate::verdict::{CheckOutcome, IndeterminateReason, Verdict};

/// Build an `Indeterminate` outcome, the common shape every error exit
/// below shares.
fn indeterminate(name: String, msg: String) -> CheckOutcome {
    CheckOutcome {
        name,
        verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(msg)),
    }
}

/// Run `spec` against `store` and produce a verdict. `spec.query`
/// resolves against `base_dir` (a case's directory); `pm` resolves the
/// CURIEs in `spec.expect_rows`, exactly as `base_dir` and `pm` are
/// already threaded through the Manchester checks in `oracle.rs`.
#[must_use]
pub fn check_cq(store: &Store, spec: &CqSpec, base_dir: &Path, pm: &PrefixMapping) -> CheckOutcome {
    let name = format!("cq {}", spec.query.display());
    let path = base_dir.join(&spec.query);

    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            return indeterminate(name, format!("cannot read query {}: {e}", path.display()));
        }
    };

    let prepared = match SparqlEvaluator::new().parse_query(&text) {
        Ok(p) => p,
        Err(e) => {
            return indeterminate(name, format!("cannot parse query {}: {e}", path.display()));
        }
    };

    let results = match prepared.on_store(store).execute() {
        Ok(r) => r,
        Err(e) => {
            return indeterminate(
                name,
                format!("query {} failed to execute: {e}", path.display()),
            );
        }
    };

    let solutions = match results {
        QueryResults::Solutions(s) => s,
        QueryResults::Boolean(_) => {
            return indeterminate(
                name,
                format!(
                    "query {} is an ASK query; expect_rows only applies to SELECT",
                    path.display()
                ),
            );
        }
        QueryResults::Graph(_) => {
            return indeterminate(
                name,
                format!(
                    "query {} is a CONSTRUCT/DESCRIBE query; expect_rows only applies to SELECT",
                    path.display()
                ),
            );
        }
    };

    let variables: Vec<String> = solutions
        .variables()
        .iter()
        .map(|v| v.as_str().to_string())
        .collect();

    let mut actual: Vec<BTreeMap<String, Option<Term>>> = Vec::new();
    for solution in solutions {
        let solution = match solution {
            Ok(s) => s,
            Err(e) => {
                return indeterminate(
                    name,
                    format!("query {} failed while reading results: {e}", path.display()),
                );
            }
        };
        // Every query variable is a key in the row, bound or not, so
        // an unbound variable in a solution becomes `None` rather than
        // being absent from the map: `rows::compare` relies on that
        // distinction to tell "unbound" from "not compared".
        let row = variables
            .iter()
            .map(|v| (v.clone(), solution.get(v.as_str()).cloned()))
            .collect();
        actual.push(row);
    }

    let mut expected: Vec<BTreeMap<String, Option<Term>>> = Vec::new();
    for expect_row in &spec.expect_rows {
        let mut row = BTreeMap::new();
        for (var, token) in expect_row {
            let parsed = match rows::parse_expected(token.as_deref(), pm) {
                Ok(e) => e,
                Err(e) => {
                    return indeterminate(
                        name,
                        format!("cannot parse expect_rows for query {}: {e}", path.display()),
                    );
                }
            };
            let term = match parsed {
                Expected::Bound(t) => Some(t),
                Expected::Unbound => None,
            };
            row.insert(var.clone(), term);
        }
        expected.push(row);
    }

    let verdict = match rows::compare(&expected, &actual, spec.exact, spec.ordered) {
        Ok(()) => Verdict::Pass,
        Err(msg) => Verdict::Fail(msg),
    };

    CheckOutcome { name, verdict }
}
