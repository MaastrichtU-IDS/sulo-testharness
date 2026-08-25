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
}

/// One or many, so `data:` accepts a string or a list.
/// One competency question: a SPARQL query plus the rows it must
/// return.
///
/// `expect_rows` is a list of rows, each a map from variable name to
/// an expected token. A YAML `null` means the variable must be
/// UNBOUND in that row, which is different from the key being absent
/// (an absent key is not compared at all).
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
    /// behalf. Defaults to 30000 (`default_timeout`). A value of 0
    /// means "expire immediately", not "no limit": it forces a
    /// deterministic `Indeterminate(Timeout)` on every check it
    /// governs, matching the zero-deadline seam already used
    /// elsewhere in this crate (`oracle::holds_with_deadline`) to
    /// force a Timeout without depending on a real reasoner call
    /// being slow.
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
