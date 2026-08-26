//! Run one competency question against a materialised store.
//!
//! A CQ is a positive assertion about what the ontology answers, run
//! against the closure `materialize` built, not a proof search. So the
//! asymmetry the rest of this crate carries between `Pass` and
//! `UnrefutedPass` does not apply here: when the actual rows match
//! `expect_rows`, that is a real answer straight out of the store, not
//! the absence of a refutation, and gets a trustworthy `Pass`.
//!
//! A configuration error is never a silent `Fail`, but the two kinds
//! of configuration error are routed differently, and the difference
//! is whether the mistake is decidable from the manifest alone:
//!
//! * Decidable STATICALLY, so refused by `manifest::load_case` before
//!   any ontology is loaded, as a `ManifestError` (exit code 2, the
//!   documented "harness or configuration error"): a `cq` entry with
//!   `ordered: true, exact: false` (undefined under spec 7.3), and a
//!   `cq` entry with an empty `expect_rows` and `exact: false` (which
//!   asserts nothing at all, whatever the query returns). Both are
//!   fail-fast: they are caught even when the ontology itself fails
//!   to load.
//! * Discovered only AT CHECK TIME, because seeing them needs the
//!   `.rq` file read or the query run. These are `Indeterminate`
//!   (exit code 3): the comparison never happened, so the harness has
//!   no answer either way. This function has EIGHT such exits, and
//!   the list below is the whole of it, not a representative sample
//!   (an earlier revision said "four" and was wrong; see the
//!   `indeterminate` call sites, which are the ground truth):
//!   1. the query file cannot be read;
//!   2. the query text does not parse;
//!   3. the parsed query fails to execute against the store;
//!   4. the query is an `ASK`, so it has no rows to compare;
//!   5. the query is a `CONSTRUCT`/`DESCRIBE`, likewise;
//!   6. reading a solution fails part-way through the result stream;
//!   7. the spec sets `ordered: true` over a query with no `ORDER
//!      BY`, so the row order it compares is arbitrary;
//!   8. an `expect_rows` token `rows::parse_expected` rejects.
//!
//!   One more `Indeterminate` for a `cq` entry is raised OUTSIDE this
//!   module, in `suite::run_case`: a `MaterializeError` means the
//!   store was never built, so every `cq` entry of that case is
//!   reported `Indeterminate` without `check_cq` ever being called.
//!
//! The two lists are disjoint on purpose. Routing a statically
//! decidable mistake to BOTH would make the check-time branch
//! unreachable, which is the defect shape this project keeps finding.
//! Exit 7 above shows the split is not merely a convention to keep
//! them disjoint: `check_cq` validates precisely what `load_case`
//! cannot see. `ordered: true` needs an `ORDER BY` (spec 7.3), and
//! whether the query has one is not in the manifest at all, so that
//! guard could not live at load time however much one wanted it
//! there; `ordered: true, exact: false` is fully decided by the two
//! booleans in the manifest, so it could not honestly live here.
//!
//! **Read this before writing a query over `hasPart` or `isPartOf`.**
//! Both are `owl:ReflexiveProperty`, and `materialize` injects the
//! self-loop `x hasPart x` / `x isPartOf x` for every named
//! individual (spec section 8 step 6, `materialize.rs`'s own module
//! doc). This is deliberate and required by the reflexivity cases, not
//! a defect: without it, a CQ pattern `?x sulo:isPartOf ?x` would
//! silently return nothing despite the axiom. But it also means a
//! query like `?whole sulo:hasPart ?part` binds `?part` to `?whole`
//! itself alongside every genuine part, which is usually not what the
//! query wants. Add `FILTER (?part != ?whole)` (or the equivalent
//! `NamedIndividual` version) whenever the self-loop is not the answer
//! being asked for; hit and fixed once already in
//! `suites/sulo/patterns/solid/queries/value-quality-unit.rq`.
//!
//! The same step has a mirror-image hazard, which bites the other
//! way: `isPartOf` is a subproperty of `isIn` and `hasPart` of
//! `contains`, so `x isIn x` and `x contains x` are ENTAILED but
//! ABSENT from the store, because `inferred_object_property_values`
//! never emits the reflexive base fact for the subproperty rule to
//! propagate, and step 4 injects only the two parthood self-loops. A
//! query resting on `isIn`/`contains` reflexivity is therefore
//! correct and still returns nothing. Likewise there are no inferred
//! `rdfs:subClassOf` triples in the store at all (step 2 adds only
//! `rdf:type`), so a query that walks class subsumption sees only
//! what the source files assert. Both are spec-conformant (spec
//! section 8 step 6 defines exactly four components and `materialize`
//! implements exactly those), not deviations to be fixed in the
//! materialiser: write the query around them.
//!
//! Also read the `not_entails` limitation on `manifest::Case::entails`
//! before writing `expect_rows` for a language-tagged literal
//! (`"..."@lang`): rustdl cannot positively confirm one by any path
//! this crate uses, so it can never appear as a bound, matched value
//! in a passing CQ row either.

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
        // The comparison never ran, so this outcome rests on nothing
        // the store did or did not contain.
        rests_on_absence: false,
    }
}

/// SPARQL keywords whose presence makes a query possibly NON-monotone
/// in the store, i.e. able to return MORE rows, or different rows,
/// when the store SHRINKS.
///
/// The list is the union of the three mechanisms that can do that:
/// solution-modifier windows (`LIMIT`/`OFFSET`, where losing a row
/// promotes another into the window), aggregation (`GROUP` and every
/// aggregate function, where fewer input rows change the aggregated
/// value rather than removing an output row, and `HAVING` filters on
/// that value), and negation as failure (`MINUS`, `NOT EXISTS`,
/// `OPTIONAL` with its unbound-on-no-match cell, and `BOUND`, which
/// tests for exactly that cell).
///
/// Deliberately OVER-broad in three ways, because a keyword this list
/// misses is unsound while one it over-matches merely costs an
/// `Indeterminate` under loss:
///
/// * `EXISTS` rather than `NOT EXISTS`. Plain `EXISTS` is monotone,
///   but matching the two-word form would need whitespace and comment
///   handling between the words, and getting that wrong would MISS a
///   real `NOT EXISTS`.
/// * `GROUP` covers both `GROUP BY` and `GROUP_CONCAT`, which is
///   listed separately anyway so this list can be read against spec
///   7.3 without decoding which entry subsumes which.
/// * `ORDER BY` is absent on purpose: on its own it permutes rows
///   without changing the multiset, and the only way it changes an
///   answer is with `LIMIT`/`OFFSET`, which are on the list. A spec
///   that does care about order is `ordered: true`, which `load_case`
///   already forces to `exact: true`, and `exact` flags the outcome
///   regardless.
const NON_MONOTONE_KEYWORDS: &[&str] = &[
    "LIMIT",
    "OFFSET",
    "MINUS",
    "EXISTS",
    "GROUP",
    "GROUP_CONCAT",
    "HAVING",
    "COUNT",
    "SUM",
    "AVG",
    "MIN",
    "MAX",
    "SAMPLE",
    "BOUND",
    "OPTIONAL",
];

/// True when `keyword` occurs in `text` case-insensitively, not
/// flanked by an ASCII alphanumeric on either side.
///
/// A TOKEN SCAN OVER RAW TEXT, chosen over inspecting the parsed
/// query and not by oversight. `oxigraph` re-exports only
/// `spargebra::SparqlSyntaxError`, not `spargebra::Query`, and
/// `PreparedSparqlQuery` exposes only `dataset` and
/// `substitute_variable`, so the algebra is reachable only by adding
/// `spargebra` as a second direct dependency and re-parsing. That
/// would be exact, but a recursive walk over `GraphPattern` AND
/// `Expression` is only as sound as its least-complete arm, and one
/// forgotten arm is an UNDER-approximation, the unsafe direction
/// here. A scan of the raw text has the opposite failure mode by
/// construction.
///
/// So this deliberately over-matches: it sees keywords inside
/// comments, string literals, and IRIs, and it treats `_` as a
/// boundary so `GROUP` matches inside `GROUP_CONCAT`. What it cannot
/// do is MISS one. A genuine SPARQL keyword is a token, so it is
/// never immediately preceded or followed by an alphanumeric (that
/// would lex as one longer name), which is exactly the only condition
/// under which this returns `false` for an occurrence it found.
fn mentions_keyword(text: &str, keyword: &str) -> bool {
    let hay = text.to_ascii_lowercase();
    let needle = keyword.to_ascii_lowercase();
    let bytes = hay.as_bytes();
    let mut from = 0;
    while let Some(rel) = hay[from..].find(&needle) {
        let start = from + rel;
        let end = start + needle.len();
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let after_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

/// Conservatively decide whether `text` is a monotone query: one that
/// can only ever return FEWER rows when the store shrinks.
///
/// Answers `false` whenever any [`NON_MONOTONE_KEYWORDS`] entry
/// appears, so "monotone" here means "no construct that could make it
/// otherwise was found", never "proved monotone". Over-approximating
/// non-monotonicity is the safe direction: see the `rests_on_absence`
/// computation in [`check_cq`], the only caller.
fn query_is_monotone(text: &str) -> bool {
    !NON_MONOTONE_KEYWORDS
        .iter()
        .any(|kw| mentions_keyword(text, kw))
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

    // Spec 7.3: `ordered: true` "is only valid with an `ORDER BY` in
    // the query". Without one, SPARQL leaves the row order arbitrary,
    // so an ordered comparison is a coin flip the harness would
    // report as a verdict; refuse to report one instead.
    //
    // This is the check-time half of the split described in this
    // module's doc: `load_case` sees the manifest, and the manifest
    // does not say whether the `.rq` has an `ORDER BY`.
    //
    // The scan errs the OTHER way from `query_is_monotone`'s, and
    // that is deliberate. `mentions_keyword` over raw text can be
    // fooled by the word `ORDER` in a comment or an IRI, which lets a
    // genuinely unordered query through and leaves the status quo (no
    // guard at all). It can never fire on a query that really does
    // have an `ORDER BY`, so it cannot turn a working case red. A
    // guard that misfires on the suite would be reverted, not fixed.
    if spec.ordered && !mentions_keyword(&text, "ORDER") {
        return indeterminate(
            name,
            format!(
                "query {} has no ORDER BY, so its row order is arbitrary and \
                 ordered: true cannot be compared (spec 7.3). Add an ORDER BY \
                 to the query, or set ordered: false.",
                path.display()
            ),
        );
    }

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

    // `rests_on_absence` records whether this outcome's meaning
    // depends on a row NOT being in the materialised store, which is
    // exactly what axiom loss can fake (loss can only SHRINK the
    // closure, and what a smaller closure does to the answer depends
    // on the query, see the third bullet).
    // `suite::downgrade_for_loss` cannot infer it:
    // a `Fail` here is built by `rows::compare` and so never carries
    // `oracle::NO_PROOF_MARKER`, and the check name is not a `GATE_*`
    // constant.
    //
    // * A `Fail` is flagged unconditionally. Every row-level failure
    //   `compare` reports is "this expected row was not there" or
    //   "this actual row was not the one expected", and a row
    //   suppressed by loss is indistinguishable from an ontology
    //   regression. `compare` has exactly one Err that is NOT of that
    //   shape, its `ordered && !exact` configuration refusal, which a
    //   direct library caller hand-building a `CqSpec` can still
    //   reach. It gets the flag set anyway, which errs toward
    //   `Indeterminate` under loss, the safe direction for a
    //   configuration error that is not an ontology answer at all.
    //   (`check_cq`'s contract is a verdict per spec for everything
    //   `load_case` cannot see; that combination is precisely what
    //   `load_case` CAN see, and every manifest-driven run is already
    //   covered by the load guard.)
    // * A `Pass` rests on absence only when the spec makes an absence
    //   claim: `exact: true` asserts "and no other rows", and a
    //   `null` cell asserts "this variable is unbound". Under loss a
    //   dropped extra row, or a binding that never got inferred,
    //   makes either claim pass unearned.
    // * A `Pass` with `exact: false`, every cell bound, AND a
    //   monotone query asserts only that certain rows are PRESENT,
    //   and over a monotone query loss can only remove rows, never
    //   add one. Downgrading it would overstate the harness's
    //   uncertainty in the other direction, so it stays a
    //   trustworthy `Pass`.
    //
    //   The monotonicity condition is load-bearing and was missing
    //   here until this round; the carve-out was stated as an
    //   unconditional "loss can never add a row", which is false for
    //   SPARQL in general. Loss shrinks the materialised store, and a
    //   SMALLER store yields more or different rows under aggregates
    //   (`COUNT` over fewer rows still emits a row, with a different
    //   value), under `LIMIT`/`OFFSET` (a lost row promotes the next
    //   one into the window), and under negation as failure (`MINUS`,
    //   `NOT EXISTS`, and an `OPTIONAL` cell that goes unbound). In
    //   each of those an `exact: false` all-bound `Pass` can be
    //   MANUFACTURED by loss, so it is exactly as absence-resting as
    //   an `exact: true` one. `query_is_monotone` decides this
    //   conservatively, over-approximating non-monotonicity.
    //
    //   Honest scope, measured over every `.rq` in the repository
    //   rather than assumed: both suite queries
    //   (`who-participated.rq`, which uses `ORDER BY` without a
    //   `LIMIT`, and `value-quality-unit.rq`) are monotone, so no
    //   manifest-driven check changes verdict because of this. The
    //   only queries the scan flags are test fixtures:
    //   `parts_of_a_optional_label.rq`, which was already here and
    //   uses `OPTIONAL`, plus the two written for the tests of this
    //   condition. On top of that the flag only matters when there is
    //   loss to downgrade for, which `suite::downgrade_for_loss`
    //   documents as not happening on this suite either. This closes
    //   a latent hole; it does not fix an observable wrong verdict.
    let (verdict, rests_on_absence) =
        match rows::compare(&expected, &actual, spec.exact, spec.ordered) {
            Ok(()) => (
                Verdict::Pass,
                spec.exact
                    || spec
                        .expect_rows
                        .iter()
                        .any(|r| r.values().any(Option::is_none))
                    || !query_is_monotone(&text),
            ),
            Err(msg) => (Verdict::Fail(msg), true),
        };

    CheckOutcome {
        name,
        verdict,
        rests_on_absence,
    }
}
