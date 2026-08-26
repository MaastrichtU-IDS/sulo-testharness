//! YAML case manifests.
//!
//! `deny_unknown_fields` is load-bearing. A mistyped key like
//! `entials:` would otherwise parse as a case with nothing to check
//! and report a confident green, which is the single worst failure
//! mode available to a test harness.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid manifest {path}: {source}")]
    Yaml {
        path: PathBuf,
        source: serde_yaml::Error,
    },
    #[error("manifest {path} has an empty id")]
    EmptyId { path: PathBuf },
    #[error(
        "manifest {path} asserts nothing: set at least one of entails, not_entails, \
         entails_manchester, not_entails_manchester, instance_of_expr, satisfiable_expr, \
         unsatisfiable, cq, or expect_inconsistent: true"
    )]
    NoAssertions { path: PathBuf },
    #[error(
        "manifest {path}: cq entry for query {query} sets ordered: true with \
         exact: false, a combination spec 7.3 leaves undefined (it does not say \
         whether an unmatched actual row may appear before, between, or only \
         after the expected sequence). Use ordered: true, exact: true for an \
         exact sequence, or ordered: false, exact: false for an unordered subset."
    )]
    CqOrderedNotExact { path: PathBuf, query: PathBuf },
    #[error(
        "manifest {path}: cq entry for query {query} has an empty expect_rows \
         with exact: false, so it asserts nothing and passes whatever the query \
         returns. Use exact: true to assert that the query returns no rows, or \
         list the rows it must return."
    )]
    CqAssertsNothing { path: PathBuf, query: PathBuf },
}

/// One competency question: a SPARQL query plus the rows it must
/// return.
///
/// `expect_rows` is a list of rows, each a map from variable name to
/// an expected token. A YAML `null` means the variable must be
/// UNBOUND in that row, which is different from the key being absent.
///
/// An expected row must name EVERY variable the query projects.
/// `rows::compare` compares whole rows by `BTreeMap` equality, and
/// `cq::check_cq` builds each actual row with a key per projected
/// variable (bound or `None`), so a row that omits a projected
/// variable has a smaller key set and can never match anything. An
/// absent key is not "not compared"; it is a row that cannot match.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CqSpec {
    pub query: PathBuf,
    #[serde(default)]
    pub expect_rows: Vec<BTreeMap<String, Option<String>>>,
    /// `true` requires set equality with the actual rows. `false`
    /// requires only that every expected row is present.
    #[serde(default = "default_true")]
    pub exact: bool,
    /// `true` compares as a sequence. Only meaningful with an
    /// `ORDER BY` in the query.
    #[serde(default)]
    pub ordered: bool,
}

fn default_true() -> bool {
    true
}

/// One or many, so `data:` accepts a string or a list.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(PathBuf),
    Many(Vec<PathBuf>),
}

impl OneOrMany {
    fn into_vec(self) -> Vec<PathBuf> {
        match self {
            OneOrMany::One(p) => vec![p],
            OneOrMany::Many(v) => v,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SubsumptionExpr {
    pub sub_expr: String,
    pub sup_expr: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstanceExpr {
    pub individual: String,
    pub expr: String,
}

fn default_timeout() -> u64 {
    30_000
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCase {
    id: String,
    description: String,
    #[serde(default)]
    ontology: Option<PathBuf>,
    #[serde(default)]
    imports: Option<OneOrMany>,
    #[serde(default)]
    data: Option<OneOrMany>,
    #[serde(default)]
    prefixes: BTreeMap<String, String>,
    #[serde(default)]
    expect_inconsistent: bool,
    #[serde(default)]
    entails: Option<String>,
    #[serde(default)]
    not_entails: Option<String>,
    #[serde(default)]
    entails_manchester: Vec<SubsumptionExpr>,
    #[serde(default)]
    not_entails_manchester: Vec<SubsumptionExpr>,
    #[serde(default)]
    instance_of_expr: Vec<InstanceExpr>,
    #[serde(default)]
    satisfiable_expr: Vec<String>,
    #[serde(default)]
    unsatisfiable: Vec<String>,
    #[serde(default)]
    cq: Vec<CqSpec>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
}

/// A parsed case, with paths still relative to the manifest.
#[derive(Debug)]
pub struct Case {
    pub id: String,
    /// Prose for the reader. Parsed and carried, but nothing in the
    /// harness reads it; it is not part of any verdict or report line.
    pub description: String,
    pub ontology: Option<PathBuf>,
    pub imports: Vec<PathBuf>,
    pub data: Vec<PathBuf>,
    pub prefixes: BTreeMap<String, String>,
    pub expect_inconsistent: bool,
    /// A Turtle fragment whose every triple must be entailed.
    ///
    /// KNOWN LIMITATION, pinned reasoner v0.4.22: a language-tagged
    /// literal here can NEVER succeed. rustdl cannot positively
    /// confirm `rdf:langString` `DataHasValue` membership by any path
    /// (neither the materialised `inferred_data_property_values`,
    /// which drops the language tag entirely, nor the bounded
    /// satisfiability probe, nor the retired unbounded
    /// `class_expression_instances`), even for an individual with that
    /// exact literal asserted directly. So `ex:n sulo:hasValue
    /// "bonjour"@fr .` under `entails:` is a permanent Fail (or, with
    /// any axiom loss present, a permanent Indeterminate), not an
    /// ontology defect. Assert such a value under `not_entails:` if it
    /// must be mentioned at all, and expect `UnrefutedPass`.
    pub entails: Option<String>,
    /// A Turtle fragment whose every triple must NOT be entailed.
    /// Satisfying this yields `UnrefutedPass`, never `Pass`: the
    /// reasoner is incomplete, so it only failed to refute the
    /// negative. See `entails` for the `rdf:langString` limitation,
    /// which applies to this field too (a language-tagged literal here
    /// always "passes", unrefuted, for a reason unrelated to the
    /// ontology).
    pub not_entails: Option<String>,
    pub entails_manchester: Vec<SubsumptionExpr>,
    pub not_entails_manchester: Vec<SubsumptionExpr>,
    pub instance_of_expr: Vec<InstanceExpr>,
    pub satisfiable_expr: Vec<String>,
    pub unsatisfiable: Vec<String>,
    pub cq: Vec<CqSpec>,
    /// Free-form labels. Parsed and carried, but nothing reads them
    /// yet: the `--tag` case filter is deferred, so a tag today is
    /// documentation for a human reader, not a selector. Recorded here
    /// so the field does not read as working machinery.
    pub tags: Vec<String>,
    /// The per-case reasoner time budget, in milliseconds, used as
    /// the `deadline` for every check `run_case` makes on this case's
    /// behalf. Defaults to 30000 (`default_timeout`).
    ///
    /// A value of 0 means "expire immediately", not "no limit". It
    /// does NOT, however, force `Indeterminate(Timeout)` on every
    /// check it governs, and an earlier version of this comment said
    /// it did. Measured: a subsumption check over
    /// `tests/fixtures/clean.ttl` with `timeout_ms: 0` still returns
    /// `Pass`, because the reasoner settles it before reaching the
    /// tableau where the deadline is consulted. What is true is
    /// narrower, and is what `oracle::holds_with_deadline` claims: a
    /// zero deadline forces a Timeout on any arm that genuinely needs
    /// tableau work, and on `materialize`, whose very first act is to
    /// check the deadline (see `tests/materialize.rs`). Reach for an
    /// unbound prefix or a `cq:` block, not a zero budget, when a
    /// deterministic `Indeterminate` is what a fixture needs.
    pub timeout_ms: u64,
    /// Directory the manifest lives in; all paths resolve against it.
    pub base_dir: PathBuf,
}

pub fn load_case(path: &Path) -> Result<Case, ManifestError> {
    let text = std::fs::read_to_string(path).map_err(|source| ManifestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let raw: RawCase = serde_yaml::from_str(&text).map_err(|source| ManifestError::Yaml {
        path: path.to_path_buf(),
        source,
    })?;

    if raw.id.trim().is_empty() {
        return Err(ManifestError::EmptyId {
            path: path.to_path_buf(),
        });
    }

    // A case with `id` and `description` and nothing else parses
    // perfectly well and then reports Pass over an empty check set:
    // the same "confident green for a check that never ran" failure
    // `deny_unknown_fields` exists to stop, arrived at from the other
    // direction. `expect_inconsistent: true` counts as an assertion
    // (the consistency gate is then the whole test); the default
    // `false` does not.
    let asserts_something = raw.expect_inconsistent
        || raw.entails.is_some()
        || raw.not_entails.is_some()
        || !raw.entails_manchester.is_empty()
        || !raw.not_entails_manchester.is_empty()
        || !raw.instance_of_expr.is_empty()
        || !raw.satisfiable_expr.is_empty()
        || !raw.unsatisfiable.is_empty()
        || !raw.cq.is_empty();
    if !asserts_something {
        return Err(ManifestError::NoAssertions {
            path: path.to_path_buf(),
        });
    }

    // Two `cq` configurations cannot do their job, and both are
    // decidable from the manifest alone, so they are refused HERE
    // rather than at check time.
    //
    // Why load time and not `Indeterminate` from `cq::check_cq`:
    //
    // * Both are statically decidable, unlike the situations `cq.rs`
    //   does route to `Indeterminate` (unreadable query file, parse
    //   failure, execution failure, ASK, CONSTRUCT/DESCRIBE, a
    //   failure part-way through the result stream, `ordered: true`
    //   over a query with no `ORDER BY`, and a rejected `expect_rows`
    //   token), each of which needs the `.rq` file read or the query
    //   run. That list is enumerated rather than counted on purpose:
    //   it was written here as "four" while `cq.rs` had seven exits,
    //   and a stale count reads as a closed enumeration. `cq.rs`'s
    //   module doc holds the authoritative list.
    // * Exit code 2 is the documented meaning of "harness or
    //   configuration error"; `Indeterminate` (exit 3) means the
    //   reasoner could not answer, which is not what happened.
    // * Fail-fast beats fail-at-check: the mistake is caught even
    //   when the ontology itself fails to load, so the author is not
    //   told about the ontology when the manifest is the problem.
    // * One guard site instead of two. Doing both would make the
    //   check-time branch unreachable, which is exactly the "a check
    //   that cannot fail" shape this harness exists to refuse.
    //
    // `rows::compare` keeps its own `ordered && !exact` guard as
    // defence-in-depth for direct library callers that never pass
    // through a manifest; see that function's doc comment.
    for spec in &raw.cq {
        if spec.ordered && !spec.exact {
            return Err(ManifestError::CqOrderedNotExact {
                path: path.to_path_buf(),
                query: spec.query.clone(),
            });
        }
        // An empty `expect_rows` with `exact: false` makes
        // `rows::compare` run an empty expected loop, skip the
        // leftover check, and return `Ok(())` no matter what the
        // query returned: a green check that cannot fail, which is
        // the same mistake `NoAssertions` above exists to refuse, one
        // level in. Empty `expect_rows` with `exact: true` is a
        // legitimate "this query must return nothing" assertion and
        // is deliberately still accepted.
        if spec.expect_rows.is_empty() && !spec.exact {
            return Err(ManifestError::CqAssertsNothing {
                path: path.to_path_buf(),
                query: spec.query.clone(),
            });
        }
    }

    Ok(Case {
        id: raw.id,
        description: raw.description,
        ontology: raw.ontology,
        imports: raw.imports.map(OneOrMany::into_vec).unwrap_or_default(),
        data: raw.data.map(OneOrMany::into_vec).unwrap_or_default(),
        prefixes: raw.prefixes,
        expect_inconsistent: raw.expect_inconsistent,
        entails: raw.entails,
        not_entails: raw.not_entails,
        entails_manchester: raw.entails_manchester,
        not_entails_manchester: raw.not_entails_manchester,
        instance_of_expr: raw.instance_of_expr,
        satisfiable_expr: raw.satisfiable_expr,
        unsatisfiable: raw.unsatisfiable,
        cq: raw.cq,
        tags: raw.tags,
        timeout_ms: raw.timeout_ms,
        base_dir: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
    })
}
