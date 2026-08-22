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
    tags: Vec<String>,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
}

/// A parsed case, with paths still relative to the manifest.
#[derive(Debug)]
pub struct Case {
    pub id: String,
    pub description: String,
    pub ontology: Option<PathBuf>,
    pub imports: Vec<PathBuf>,
    pub data: Vec<PathBuf>,
    pub prefixes: BTreeMap<String, String>,
    pub expect_inconsistent: bool,
    pub entails: Option<String>,
    pub not_entails: Option<String>,
    pub entails_manchester: Vec<SubsumptionExpr>,
    pub not_entails_manchester: Vec<SubsumptionExpr>,
    pub instance_of_expr: Vec<InstanceExpr>,
    pub satisfiable_expr: Vec<String>,
    pub unsatisfiable: Vec<String>,
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
        tags: raw.tags,
        timeout_ms: raw.timeout_ms,
        base_dir: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
    })
}
