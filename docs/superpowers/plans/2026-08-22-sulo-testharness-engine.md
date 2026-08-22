# sulo-testharness Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `sulo-testharness` engine: a Rust CLI that loads an OWL ontology plus test data, checks declared entailments and non-entailments against the rustdl reasoner, proves its own cases load-bearing by mutation, and diffs a golden inference closure.

**Architecture:** One Rust crate, library plus thin CLI. Turtle is parsed in-process by `horned-owl` (no subprocess, no OFN conversion), reasoning goes through `owl-dl-reasoner` as a library. Test cases are YAML manifests with Turtle and Manchester sidecars, translated into typed claims and dispatched to specific reasoner queries. Untrusted-direction answers are backstopped by a golden closure diff rather than by a completeness flag.

**Tech Stack:** Rust (edition 2024, toolchain 1.95.0), `horned-owl` (OWL model + Turtle reader), `owl-dl-reasoner` (rustdl, SROIQ reasoning), `oxrdfio`/`oxrdf` (RDF term parsing), `curie` (prefix mapping), `serde`/`serde_yaml`, `clap`.

**Spec:** `docs/superpowers/specs/2026-08-21-sulo-testharness-design.md`

## Global Constraints

- **Rust toolchain 1.95.0**, edition 2024. Matches rustdl's `rust-toolchain.toml` pin; a lower toolchain will not build the reasoner.
- **`horned-owl` MUST be pinned to git rev `b188edaf7c92600918f0524962d928097ecd6b4d`** from `https://github.com/micheldumontier/horned-owl`. This is the rev rustdl itself pins. Cargo unifies a git dependency only on exact source plus rev match, so depending on crates.io `horned-owl = "2.0.0"` instead compiles a second copy of the crate and the `SetOntology` you parse becomes a different type from the one `owl_dl_reasoner::is_consistent` accepts. Symptom: a type-mismatch error naming two identical-looking types.
- **`owl-dl-reasoner` pinned to rustdl tag `v0.4.22`.** During local development a `[patch]` to the sibling checkout at `../rustdl` is acceptable; the committed `Cargo.toml` must name the tag.
- **All parsing uses `local_only: true`.** The harness must never touch the network. `sulo-dev.ttl` carries `owl:imports <https://w3id.org/sulo/sulo.ttl>`, and the non-closure reader plus `local_only` guarantees a hermetic run. Manifest `imports:` is the only supported way to merge another ontology.
- **Exit codes are contract:** `0` all pass, `1` any Fail, `2` harness or configuration error, `3` any Indeterminate, `4` golden drift or re-baseline required, `5` oracle divergence.
- **No JVM anywhere in this plan.** The HermiT differential is a later phase.
- **No em-dashes in any output, comment, or documentation text.** Use commas, colons, parentheses, or sentence breaks.
- **Never collapse an Indeterminate into a Pass or a Fail.** Verdict precedence is Fail > Indeterminate > UnrefutedPass > Pass.

## File Structure

| File | Responsibility |
| --- | --- |
| `Cargo.toml` | Pinned dependencies (see constraints) |
| `src/lib.rs` | Public API surface, module wiring |
| `src/verdict.rs` | `Verdict`, `CheckOutcome`, aggregation and precedence |
| `src/prefixes.rs` | Assembling the layered `curie::PrefixMapping` |
| `src/load.rs` | Turtle/OWL ingest, `IncompleteParse` detection, `DisjointUnion` pre-lowering, `dropped_axioms` |
| `src/manifest.rs` | YAML case parsing and validation |
| `src/claim.rs` | Turtle fragment and Manchester string to typed `Claim` |
| `src/oracle.rs` | `Claim` to reasoner query, producing a `CheckOutcome` |
| `src/suite.rs` | Discovery, consistency gate, per-case orchestration |
| `src/golden.rs` | Canonical closure serialisation and diff |
| `src/report.rs` | Human output, `--json` |
| `src/main.rs` | CLI, exit-code mapping |
| `mutants/*.ttl` | Deliberately broken SULO variants |
| `tests/*.rs` | Integration tests, including the mutation self-test |

Tasks 1 to 4 build the loading and verdict foundation, 5 to 8 the claim and oracle pipeline, 9 to 11 the two trust mechanisms.

---

### Task 1: Crate skeleton and the verdict lattice

**Files:**
- Create: `Cargo.toml`, `src/lib.rs`, `src/verdict.rs`, `rust-toolchain.toml`
- Test: `tests/verdict.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `Verdict` enum (`Pass`, `UnrefutedPass`, `Indeterminate(IndeterminateReason)`, `Fail(String)`), `IndeterminateReason` enum (`Timeout`, `AxiomLoss(String)`, `OracleError(String)`), `CheckOutcome { name: String, verdict: Verdict }`, `fn aggregate(outcomes: &[CheckOutcome]) -> Verdict`, `fn exit_code(v: &Verdict) -> i32`.

- [ ] **Step 1: Write the failing test**

```rust
// tests/verdict.rs
use sulo_testharness::verdict::{
    CheckOutcome, IndeterminateReason, Verdict, aggregate, exit_code,
};

fn outcome(name: &str, v: Verdict) -> CheckOutcome {
    CheckOutcome { name: name.to_string(), verdict: v }
}

#[test]
fn empty_suite_passes() {
    assert_eq!(aggregate(&[]), Verdict::Pass);
}

#[test]
fn fail_beats_everything() {
    let out = vec![
        outcome("a", Verdict::Pass),
        outcome("b", Verdict::UnrefutedPass),
        outcome("c", Verdict::Indeterminate(IndeterminateReason::Timeout)),
        outcome("d", Verdict::Fail("boom".into())),
    ];
    assert!(matches!(aggregate(&out), Verdict::Fail(_)));
}

#[test]
fn indeterminate_beats_unrefuted_pass() {
    let out = vec![
        outcome("a", Verdict::UnrefutedPass),
        outcome("b", Verdict::Indeterminate(IndeterminateReason::Timeout)),
    ];
    assert!(matches!(aggregate(&out), Verdict::Indeterminate(_)));
}

#[test]
fn unrefuted_pass_beats_pass() {
    let out = vec![outcome("a", Verdict::Pass), outcome("b", Verdict::UnrefutedPass)];
    assert_eq!(aggregate(&out), Verdict::UnrefutedPass);
}

#[test]
fn exit_codes_match_the_contract() {
    assert_eq!(exit_code(&Verdict::Pass), 0);
    assert_eq!(exit_code(&Verdict::UnrefutedPass), 0);
    assert_eq!(exit_code(&Verdict::Fail("x".into())), 1);
    assert_eq!(exit_code(&Verdict::Indeterminate(IndeterminateReason::Timeout)), 3);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test verdict`
Expected: FAIL, the crate does not exist yet (`error: could not find Cargo.toml` or unresolved import `sulo_testharness`).

- [ ] **Step 3: Write minimal implementation**

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.95.0"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

`Cargo.toml`:

```toml
[package]
name = "sulo-testharness"
version = "0.1.0"
edition = "2024"
rust-version = "1.95"
license = "MIT OR Apache-2.0"
description = "Regression and competency-question test harness for the SULO ontology"

[dependencies]
# MUST match the rev rustdl pins, or SetOntology becomes two distinct types.
horned-owl = { git = "https://github.com/micheldumontier/horned-owl", rev = "b188edaf7c92600918f0524962d928097ecd6b4d", default-features = false }
owl-dl-reasoner = { git = "https://github.com/MaastrichtU-IDS/rustdl", tag = "v0.4.22" }
oxrdfio = "0.2.0"
oxrdf = "0.3.0"
curie = "0.1"
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
clap = { version = "4", features = ["derive"] }
anyhow = "1"
thiserror = "2"

[lib]
name = "sulo_testharness"
path = "src/lib.rs"

[[bin]]
name = "sulo-testharness"
path = "src/main.rs"
```

`src/verdict.rs`:

```rust
//! Verdicts and their precedence.
//!
//! Four outcomes, not two. `UnrefutedPass` exists because a
//! sound-but-incomplete reasoner reporting "not entailed" for a
//! negative test has not proved the non-entailment, only failed to
//! refute it. Reporting that as an ordinary Pass would overstate what
//! the harness knows.

/// Why a check could not be decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndeterminateReason {
    /// The reasoner exceeded the case's time budget.
    Timeout,
    /// An axiom was lost on the way in, so a "not entailed" answer is
    /// not meaningful. Carries a human-readable description.
    AxiomLoss(String),
    /// The reasoner returned an error for this query.
    OracleError(String),
}

/// The outcome of a single check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Trustworthy pass, guaranteed by the reasoner's soundness.
    Pass,
    /// A negative expectation the reasoner failed to refute. Not a
    /// proof of non-entailment. Does not fail the build.
    UnrefutedPass,
    /// Undecided. Never silently promoted or demoted.
    Indeterminate(IndeterminateReason),
    /// Trustworthy failure, carrying an explanation.
    Fail(String),
}

impl Verdict {
    /// Higher rank wins when aggregating.
    fn rank(&self) -> u8 {
        match self {
            Verdict::Pass => 0,
            Verdict::UnrefutedPass => 1,
            Verdict::Indeterminate(_) => 2,
            Verdict::Fail(_) => 3,
        }
    }
}

/// One named check and how it came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOutcome {
    pub name: String,
    pub verdict: Verdict,
}

/// Combine outcomes worst-first. An empty set passes.
#[must_use]
pub fn aggregate(outcomes: &[CheckOutcome]) -> Verdict {
    outcomes
        .iter()
        .map(|o| o.verdict.clone())
        .max_by_key(Verdict::rank)
        .unwrap_or(Verdict::Pass)
}

/// Map a verdict to its process exit code. Codes 2, 4, and 5 are
/// raised by the caller, not derived from a verdict.
#[must_use]
pub fn exit_code(v: &Verdict) -> i32 {
    match v {
        Verdict::Pass | Verdict::UnrefutedPass => 0,
        Verdict::Fail(_) => 1,
        Verdict::Indeterminate(_) => 3,
    }
}
```

`src/lib.rs`:

```rust
//! Regression and competency-question test harness for the SULO ontology.

pub mod verdict;
```

`src/main.rs`:

```rust
fn main() {
    println!("sulo-testharness: not wired up yet");
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test verdict`
Expected: PASS, 5 tests. Note the first build fetches and compiles rustdl and horned-owl, which takes several minutes.

- [ ] **Step 5: Verify the dependency pin actually unified**

Run: `cargo tree -p horned-owl | head -20`
Expected: exactly one `horned-owl` entry. If two appear, the pin is wrong; fix `Cargo.toml` before proceeding, because every later task depends on the types matching.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock rust-toolchain.toml src/lib.rs src/verdict.rs src/main.rs tests/verdict.rs
git commit -m "feat: crate skeleton and the four-way verdict lattice"
```

---

### Task 2: Hermetic ontology loading with axiom-loss detection

**Files:**
- Create: `src/load.rs`, `tests/load.rs`, `tests/fixtures/all-disjoint.ttl`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `struct Loaded { pub ontology: SetOntology<RcStr>, pub loss: Vec<String> }`, `fn load_file(path: &Path) -> Result<Loaded, LoadError>`, `fn merge(base: &mut SetOntology<RcStr>, other: SetOntology<RcStr>)`, `enum LoadError`.

`loss` is a list of human-readable descriptions of anything dropped, from either the parser or the reasoner conversion. Empty means nothing was lost.

- [ ] **Step 1: Write the failing test**

`tests/fixtures/all-disjoint.ttl`:

```turtle
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex:  <http://example.org/> .

<http://example.org/onto> a owl:Ontology .

ex:A a owl:Class .
ex:B a owl:Class .
ex:C a owl:Class .

[] a owl:AllDisjointClasses ;
   owl:members ( ex:A ex:B ex:C ) .
```

```rust
// tests/load.rs
use std::path::Path;
use sulo_testharness::load::load_file;

#[test]
fn loads_turtle_and_reports_alldisjointclasses_as_loss() {
    let loaded = load_file(Path::new("tests/fixtures/all-disjoint.ttl"))
        .expect("fixture should parse");

    // horned-owl has no AllDisjointClasses handling: the triples land in
    // IncompleteParse. The harness must surface that, not swallow it.
    assert!(
        !loaded.loss.is_empty(),
        "AllDisjointClasses must be reported as loss, got none"
    );
    assert!(
        loaded.loss.iter().any(|d| d.contains("parse")),
        "loss should name the parse channel, got {:?}",
        loaded.loss
    );
}

#[test]
fn a_clean_ontology_reports_no_loss() {
    let loaded = load_file(Path::new("tests/fixtures/clean.ttl"))
        .expect("fixture should parse");
    assert!(loaded.loss.is_empty(), "unexpected loss: {:?}", loaded.loss);
}

#[test]
fn missing_file_is_an_error_not_a_panic() {
    assert!(load_file(Path::new("tests/fixtures/nope.ttl")).is_err());
}
```

Also create `tests/fixtures/clean.ttl`:

```turtle
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:   <http://example.org/> .

<http://example.org/onto> a owl:Ontology .

ex:A a owl:Class .
ex:B a owl:Class ;
     rdfs:subClassOf ex:A .
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test load`
Expected: FAIL, unresolved import `sulo_testharness::load`.

- [ ] **Step 3: Write minimal implementation**

`src/load.rs`:

```rust
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
use horned_owl::model::{MutableOntology, RcStr};
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
    Io { path: PathBuf, source: std::io::Error },
    #[error("cannot parse {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("unsupported extension on {path}, expected .ttl")]
    UnsupportedFormat { path: PathBuf },
}

/// Parse one Turtle file. Never touches the network.
pub fn load_file(path: &Path) -> Result<Loaded, LoadError> {
    if path.extension().and_then(|s| s.to_str()) != Some("ttl") {
        return Err(LoadError::UnsupportedFormat { path: path.to_path_buf() });
    }

    let file = File::open(path)
        .map_err(|source| LoadError::Io { path: path.to_path_buf(), source })?;
    let mut reader = BufReader::new(file);

    let mut config = ParserConfiguration::default();
    config.rdf.format = Some(oxrdfio::RdfFormat::Turtle);
    // Hermetic: never resolve an owl:imports over the network.
    config.local_only = true;

    let (concrete, incomplete) = read_rdf(&mut reader, config)
        .map_err(|e| LoadError::Parse { path: path.to_path_buf(), message: e.to_string() })?;

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

    let ontology: SetOntology<RcStr> = concrete.into();

    // Second channel: what the reasoner's IR cannot represent.
    match owl_dl_reasoner::dropped_axioms(&ontology) {
        Ok(dropped) if !dropped.is_empty() => {
            let kinds: Vec<String> = dropped
                .by_kind()
                .iter()
                .map(|(k, n)| format!("{k} x{n}"))
                .collect();
            loss.push(format!("conversion: {} dropped ({})", dropped.total(), kinds.join(", ")));
        }
        Ok(_) => {}
        Err(e) => loss.push(format!("conversion: could not be checked: {e}")),
    }

    Ok(Loaded { ontology, loss })
}

/// Fold `other`'s components into `base`.
pub fn merge(base: &mut SetOntology<RcStr>, other: SetOntology<RcStr>) {
    for component in other {
        base.insert(component);
    }
}
```

Add to `src/lib.rs`:

```rust
pub mod load;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test load`
Expected: PASS, 3 tests. If `loads_turtle_and_reports_alldisjointclasses_as_loss` fails with an empty `loss`, inspect what `IncompleteParse` actually holds for that fixture and widen the fixture (for example add a second `AllDisjointClasses`) rather than weakening the assertion. The loss channel is the point of the task.

- [ ] **Step 5: Confirm hermeticism**

Run: `cargo test --test load 2>&1 | grep -i "http\|network\|resolve" || echo "no network activity"`
Expected: `no network activity`.

- [ ] **Step 6: Commit**

```bash
git add src/load.rs src/lib.rs tests/load.rs tests/fixtures/
git commit -m "feat(load): hermetic Turtle ingest with two-channel axiom-loss detection"
```

---

### Task 3: DisjointUnion pre-lowering

The covering half of `DisjointUnion` is not enforced by rustdl in the ABox path. This task restores it by rewriting each `DisjointUnion` into the two axioms it abbreviates. It is a workaround for a reasoner bug and must be labelled as one.

**Files:**
- Modify: `src/load.rs`
- Create: `tests/lowering.rs`, `tests/fixtures/covering.ttl`

**Interfaces:**
- Consumes: `Loaded`, `load_file` from Task 2.
- Produces: `fn lower_disjoint_unions(onto: &mut SetOntology<RcStr>) -> usize` returning how many were rewritten. `load_file` calls it before returning.

- [ ] **Step 1: Write the failing test**

`tests/fixtures/covering.ttl`:

```turtle
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex:  <http://example.org/> .

<http://example.org/onto> a owl:Ontology .

ex:F a owl:Class ;
     owl:disjointUnionOf ( ex:A ex:B ) .
ex:A a owl:Class .
ex:B a owl:Class .

# An F that is neither an A nor a B. The covering half of the
# disjoint union makes this unsatisfiable.
ex:x a ex:F,
       [ a owl:Class ; owl:complementOf ex:A ],
       [ a owl:Class ; owl:complementOf ex:B ] .
```

```rust
// tests/lowering.rs
use std::path::Path;
use sulo_testharness::load::load_file;

#[test]
fn covering_violation_is_inconsistent_after_lowering() {
    let loaded = load_file(Path::new("tests/fixtures/covering.ttl"))
        .expect("fixture should parse");

    let consistent = owl_dl_reasoner::is_consistent(&loaded.ontology)
        .expect("consistency check should not error");

    // Without pre-lowering rustdl reports this consistent, which is
    // wrong: the disjoint union says A and B exhaust F.
    assert!(
        !consistent,
        "an F that is neither A nor B must be inconsistent once the \
         disjoint union is lowered"
    );
}

#[test]
fn lowering_preserves_the_disjointness_half() {
    // An individual in both A and B must still clash.
    let loaded = load_file(Path::new("tests/fixtures/covering-both.ttl"))
        .expect("fixture should parse");
    let consistent = owl_dl_reasoner::is_consistent(&loaded.ontology).unwrap();
    assert!(!consistent, "A and B are disjoint, so being both must clash");
}
```

Also create `tests/fixtures/covering-both.ttl`:

```turtle
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex:  <http://example.org/> .

<http://example.org/onto> a owl:Ontology .

ex:F a owl:Class ;
     owl:disjointUnionOf ( ex:A ex:B ) .
ex:A a owl:Class .
ex:B a owl:Class .

ex:y a ex:A, ex:B .
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test lowering`
Expected: `covering_violation_is_inconsistent_after_lowering` FAILS with "must be inconsistent", because no lowering happens yet. `lowering_preserves_the_disjointness_half` should already PASS, since rustdl handles that half correctly. Confirming both is the point: it isolates exactly what is broken.

- [ ] **Step 3: Write minimal implementation**

Add to `src/load.rs`:

```rust
use horned_owl::model::{
    Component, ClassExpression, DisjointClasses, DisjointUnion, EquivalentClasses,
};

/// Rewrite every `DisjointUnion(C, D1..Dn)` into the two axioms it
/// abbreviates: `EquivalentClasses(C, ObjectUnionOf(D1..Dn))` and
/// `DisjointClasses(D1..Dn)`. Returns how many were rewritten.
///
/// WORKAROUND for a rustdl bug: the reasoner enforces the
/// disjointness half of a `DisjointUnion` but silently loses the
/// covering half in the ABox path, with no dropped-axiom diagnostic
/// and no incomplete flag. Verified: an individual typed `F` and
/// explicitly neither `A` nor `B` under `DisjointUnion(F, A, B)` is
/// reported consistent. Spelling the axiom out restores the covering
/// behaviour.
///
/// The original `DisjointUnion` is left in place: it is harmless and
/// keeps the ontology faithful to its source. Remove this function
/// when the upstream bug is fixed.
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
        if members.len() < 2 {
            // A one-member union carries no disjointness and its
            // covering half is a plain equivalence; still lower it.
        }

        let union_of = ClassExpression::ObjectUnionOf(members.clone());
        onto.insert(EquivalentClasses(vec![
            ClassExpression::Class(class),
            union_of,
        ]));

        if members.len() >= 2 {
            onto.insert(DisjointClasses(members));
        }
    }

    count
}
```

Then call it from `load_file`, immediately before the dropped-axiom check so the check sees the final ontology. Replace:

```rust
    let ontology: SetOntology<RcStr> = concrete.into();
```

with:

```rust
    let mut ontology: SetOntology<RcStr> = concrete.into();
    lower_disjoint_unions(&mut ontology);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test lowering --test load`
Expected: PASS, all 5 tests. Task 2's tests must still pass; lowering adds axioms and must not introduce loss.

- [ ] **Step 5: Commit**

```bash
git add src/load.rs tests/lowering.rs tests/fixtures/
git commit -m "fix(load): lower DisjointUnion to restore the covering half

Workaround for a rustdl bug: the covering half of a DisjointUnion is
silently lost in the ABox path, so an individual asserted to be a
Feature and none of its four members is reported consistent. Spelling
the axiom out as EquivalentClasses + DisjointClasses restores it."
```

---

### Task 4: The layered prefix map

**Files:**
- Create: `src/prefixes.rs`, `tests/prefixes.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `fn base_mapping() -> PrefixMapping`, `fn with_overrides(base: &PrefixMapping, overrides: &BTreeMap<String, String>) -> PrefixMapping`, `fn expand(pm: &PrefixMapping, curie: &str) -> Result<String, PrefixError>`, `enum PrefixError`.

- [ ] **Step 1: Write the failing test**

```rust
// tests/prefixes.rs
use std::collections::BTreeMap;
use sulo_testharness::prefixes::{base_mapping, expand, with_overrides};

#[test]
fn sulo_and_the_standard_prefixes_are_always_bound() {
    let pm = base_mapping();
    assert_eq!(expand(&pm, "sulo:Process").unwrap(), "https://w3id.org/sulo/Process");
    assert_eq!(
        expand(&pm, "owl:Thing").unwrap(),
        "http://www.w3.org/2002/07/owl#Thing"
    );
    assert_eq!(
        expand(&pm, "rdfs:subClassOf").unwrap(),
        "http://www.w3.org/2000/01/rdf-schema#subClassOf"
    );
}

#[test]
fn case_overrides_win() {
    let mut over = BTreeMap::new();
    over.insert("ex".to_string(), "http://example.org/".to_string());
    let pm = with_overrides(&base_mapping(), &over);
    assert_eq!(expand(&pm, "ex:alice").unwrap(), "http://example.org/alice");
}

#[test]
fn a_full_iri_passes_through() {
    let pm = base_mapping();
    assert_eq!(
        expand(&pm, "<http://example.org/x>").unwrap(),
        "http://example.org/x"
    );
}

#[test]
fn an_unbound_prefix_is_an_error_not_a_guess() {
    let pm = base_mapping();
    assert!(expand(&pm, "nope:thing").is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test prefixes`
Expected: FAIL, unresolved import `sulo_testharness::prefixes`.

- [ ] **Step 3: Write minimal implementation**

`src/prefixes.rs`:

```rust
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
```

Add to `src/lib.rs`:

```rust
pub mod prefixes;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test prefixes`
Expected: PASS, 4 tests. If `expand_curie_string` is not the method name on this `curie` version, run `cargo doc -p curie --open` and use the equivalent; the contract of `expand` is what matters, not the call.

- [ ] **Step 5: Commit**

```bash
git add src/prefixes.rs src/lib.rs tests/prefixes.rs
git commit -m "feat(prefixes): one layered prefix map for fragments, Manchester, and rows"
```

---

### Task 5: Manifest parsing

**Files:**
- Create: `src/manifest.rs`, `tests/manifest.rs`, `tests/fixtures/case-ok.yaml`, `tests/fixtures/case-bad-key.yaml`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:

```rust
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
    pub timeout_ms: u64,
}
pub struct SubsumptionExpr { pub sub_expr: String, pub sup_expr: String }
pub struct InstanceExpr { pub individual: String, pub expr: String }
pub fn load_case(path: &Path) -> Result<Case, ManifestError>;
```

- [ ] **Step 1: Write the failing test**

`tests/fixtures/case-ok.yaml`:

```yaml
id: pro-role-chain
description: Participation of the role holder is recovered via the PRO role chain.
data: data/pro-encounter.ttl
prefixes:
  ex: http://example.org/
entails: |
  ex:encounter sulo:hasParticipant ex:alice .
not_entails: |
  ex:alice sulo:hasParticipant ex:encounter .
entails_manchester:
  - sub_expr: "sulo:Feature"
    sup_expr: "sulo:Capability or sulo:InformationObject"
tags: [pattern, pro]
```

`tests/fixtures/case-bad-key.yaml`:

```yaml
id: typo-case
description: Has a key the schema does not define.
entials: |
  ex:a sulo:isPartOf ex:b .
```

```rust
// tests/manifest.rs
use std::path::Path;
use sulo_testharness::manifest::load_case;

#[test]
fn parses_a_well_formed_case() {
    let c = load_case(Path::new("tests/fixtures/case-ok.yaml")).unwrap();
    assert_eq!(c.id, "pro-role-chain");
    assert_eq!(c.data.len(), 1);
    assert_eq!(c.prefixes.get("ex").unwrap(), "http://example.org/");
    assert!(c.entails.is_some());
    assert_eq!(c.entails_manchester.len(), 1);
    assert_eq!(c.entails_manchester[0].sub_expr, "sulo:Feature");
    assert_eq!(c.tags, vec!["pattern", "pro"]);
    assert_eq!(c.timeout_ms, 30_000, "default timeout");
    assert!(!c.expect_inconsistent, "default is expecting consistency");
}

#[test]
fn a_single_data_path_is_accepted_as_a_string() {
    let c = load_case(Path::new("tests/fixtures/case-ok.yaml")).unwrap();
    assert_eq!(c.data[0].to_str().unwrap(), "data/pro-encounter.ttl");
}

#[test]
fn an_unknown_key_is_rejected_loudly() {
    // A typo like `entials:` must not silently mean "no entailments to check".
    let err = load_case(Path::new("tests/fixtures/case-bad-key.yaml"))
        .expect_err("unknown key must be an error");
    assert!(
        err.to_string().contains("entials"),
        "the error should name the offending key, got: {err}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test manifest`
Expected: FAIL, unresolved import `sulo_testharness::manifest`.

- [ ] **Step 3: Write minimal implementation**

`src/manifest.rs`:

```rust
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
    Io { path: PathBuf, source: std::io::Error },
    #[error("invalid manifest {path}: {source}")]
    Yaml { path: PathBuf, source: serde_yaml::Error },
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
    pub timeout_ms: u64,
    /// Directory the manifest lives in; all paths resolve against it.
    pub base_dir: PathBuf,
}

pub fn load_case(path: &Path) -> Result<Case, ManifestError> {
    let text = std::fs::read_to_string(path)
        .map_err(|source| ManifestError::Io { path: path.to_path_buf(), source })?;
    let raw: RawCase = serde_yaml::from_str(&text)
        .map_err(|source| ManifestError::Yaml { path: path.to_path_buf(), source })?;

    if raw.id.trim().is_empty() {
        return Err(ManifestError::EmptyId { path: path.to_path_buf() });
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
```

Add to `src/lib.rs`:

```rust
pub mod manifest;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test manifest`
Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git add src/manifest.rs src/lib.rs tests/manifest.rs tests/fixtures/
git commit -m "feat(manifest): YAML case parsing with deny_unknown_fields"
```

---

### Task 6: Typed claims from Turtle fragments

**Files:**
- Create: `src/claim.rs`, `tests/claim.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `prefixes::{base_mapping, with_overrides}` from Task 4.
- Produces:

```rust
pub enum Claim {
    Subsumption { sub: String, sup: String },
    Equivalence { left: String, right: String },
    Unsatisfiable { class: String },
    ClassAssertion { individual: String, class: String },
    ObjectPropertyAssertion { subject: String, property: String, object: String },
    DataPropertyAssertion { subject: String, property: String, literal: Literal },
}
pub struct Literal { pub lexical: String, pub datatype: String, pub language: Option<String> }
pub fn parse_fragment(fragment: &str, pm: &PrefixMapping) -> Result<Vec<Claim>, ClaimError>;
```

- [ ] **Step 1: Write the failing test**

```rust
// tests/claim.rs
use std::collections::BTreeMap;
use sulo_testharness::claim::{Claim, parse_fragment};
use sulo_testharness::prefixes::{base_mapping, with_overrides};

fn pm() -> curie::PrefixMapping {
    let mut over = BTreeMap::new();
    over.insert("ex".to_string(), "http://example.org/".to_string());
    with_overrides(&base_mapping(), &over)
}

#[test]
fn classifies_a_subsumption() {
    let claims = parse_fragment("sulo:Role rdfs:subClassOf sulo:Feature .", &pm()).unwrap();
    assert_eq!(claims.len(), 1);
    match &claims[0] {
        Claim::Subsumption { sub, sup } => {
            assert_eq!(sub, "https://w3id.org/sulo/Role");
            assert_eq!(sup, "https://w3id.org/sulo/Feature");
        }
        other => panic!("expected Subsumption, got {other:?}"),
    }
}

#[test]
fn subclassof_nothing_becomes_unsatisfiable() {
    let claims = parse_fragment("sulo:Role rdfs:subClassOf owl:Nothing .", &pm()).unwrap();
    assert!(matches!(&claims[0], Claim::Unsatisfiable { .. }));
}

#[test]
fn classifies_a_class_assertion() {
    let claims = parse_fragment("ex:alice a sulo:SpatialObject .", &pm()).unwrap();
    match &claims[0] {
        Claim::ClassAssertion { individual, class } => {
            assert_eq!(individual, "http://example.org/alice");
            assert_eq!(class, "https://w3id.org/sulo/SpatialObject");
        }
        other => panic!("expected ClassAssertion, got {other:?}"),
    }
}

#[test]
fn classifies_an_object_property_assertion() {
    let claims =
        parse_fragment("ex:encounter sulo:hasParticipant ex:alice .", &pm()).unwrap();
    assert!(matches!(&claims[0], Claim::ObjectPropertyAssertion { .. }));
}

#[test]
fn classifies_a_typed_data_property_assertion() {
    let claims =
        parse_fragment(r#"ex:m sulo:hasValue "37.8"^^xsd:double ."#, &pm()).unwrap();
    match &claims[0] {
        Claim::DataPropertyAssertion { literal, .. } => {
            assert_eq!(literal.lexical, "37.8");
            assert_eq!(literal.datatype, "http://www.w3.org/2001/XMLSchema#double");
        }
        other => panic!("expected DataPropertyAssertion, got {other:?}"),
    }
}

#[test]
fn multiple_statements_yield_multiple_claims() {
    let f = "ex:encounter sulo:hasParticipant ex:alice, ex:drsmith .";
    assert_eq!(parse_fragment(f, &pm()).unwrap().len(), 2);
}

#[test]
fn a_blank_node_subject_is_rejected() {
    // Blank nodes cannot be addressed by a reasoner query and never
    // compare equal across runs.
    assert!(parse_fragment("_:b sulo:isPartOf ex:a .", &pm()).is_err());
}

#[test]
fn an_unbound_prefix_is_an_error() {
    assert!(parse_fragment("nope:x sulo:isPartOf ex:a .", &pm()).is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test claim`
Expected: FAIL, unresolved import `sulo_testharness::claim`.

- [ ] **Step 3: Write minimal implementation**

`src/claim.rs`:

```rust
//! Turning an author's Turtle fragment into questions a reasoner can
//! answer.
//!
//! The fragment is real Turtle, parsed with the suite prefix map
//! prepended, then each triple is classified by shape. A triple
//! matching no shape is an error: silently skipping it would report a
//! green for a check that never ran.

use curie::PrefixMapping;
use oxrdf::{Subject, Term, Triple};
use oxrdfio::{RdfFormat, RdfParser};

use crate::prefixes::PrefixError;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_EQUIVALENTCLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

/// An RDF literal, compared by term rather than by string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Literal {
    pub lexical: String,
    pub datatype: String,
    pub language: Option<String>,
}

/// A single checkable assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    Subsumption { sub: String, sup: String },
    Equivalence { left: String, right: String },
    Unsatisfiable { class: String },
    ClassAssertion { individual: String, class: String },
    ObjectPropertyAssertion { subject: String, property: String, object: String },
    DataPropertyAssertion { subject: String, property: String, literal: Literal },
}

#[derive(Debug, thiserror::Error)]
pub enum ClaimError {
    #[error("fragment is not valid Turtle: {0}")]
    Syntax(String),
    #[error("blank nodes cannot be used in a claim ({0}); use a skolemised IRI")]
    BlankNode(String),
    #[error("prefix problem: {0}")]
    Prefix(#[from] PrefixError),
    #[error("a literal object is only meaningful for a data property, got predicate {0}")]
    LiteralWithNonDataPredicate(String),
}

/// Parse a fragment into claims. `pm` supplies the `@prefix` header.
pub fn parse_fragment(fragment: &str, pm: &PrefixMapping) -> Result<Vec<Claim>, ClaimError> {
    let mut doc = String::new();
    for (prefix, iri) in pm.mappings() {
        doc.push_str(&format!("@prefix {prefix}: <{iri}> .\n"));
    }
    doc.push_str(fragment);

    let parser = RdfParser::from_format(RdfFormat::Turtle);
    let mut claims = Vec::new();

    for quad in parser.for_reader(doc.as_bytes()) {
        let quad = quad.map_err(|e| ClaimError::Syntax(e.to_string()))?;
        let triple: Triple = quad.into();
        claims.push(classify(&triple)?);
    }

    Ok(claims)
}

fn classify(t: &Triple) -> Result<Claim, ClaimError> {
    let subject = match &t.subject {
        Subject::NamedNode(n) => n.as_str().to_string(),
        other => return Err(ClaimError::BlankNode(format!("{other}"))),
    };
    let predicate = t.predicate.as_str().to_string();

    match &t.object {
        Term::NamedNode(obj) => {
            let object = obj.as_str().to_string();
            Ok(match predicate.as_str() {
                RDF_TYPE => Claim::ClassAssertion { individual: subject, class: object },
                RDFS_SUBCLASSOF if object == OWL_NOTHING => {
                    Claim::Unsatisfiable { class: subject }
                }
                RDFS_SUBCLASSOF => Claim::Subsumption { sub: subject, sup: object },
                OWL_EQUIVALENTCLASS => Claim::Equivalence { left: subject, right: object },
                _ => Claim::ObjectPropertyAssertion { subject, property: predicate, object },
            })
        }
        Term::Literal(lit) => {
            if predicate == RDF_TYPE || predicate == RDFS_SUBCLASSOF {
                return Err(ClaimError::LiteralWithNonDataPredicate(predicate));
            }
            Ok(Claim::DataPropertyAssertion {
                subject,
                property: predicate,
                literal: Literal {
                    lexical: lit.value().to_string(),
                    datatype: lit.datatype().as_str().to_string(),
                    language: lit.language().map(str::to_string),
                },
            })
        }
        other => Err(ClaimError::BlankNode(format!("{other}"))),
    }
}
```

Add to `src/lib.rs`:

```rust
pub mod claim;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test claim`
Expected: PASS, 8 tests. If `pm.mappings()` is not the iterator method on this `curie` version, use whatever enumerates the bindings; the `@prefix` header is what matters. If `an_unbound_prefix_is_an_error` passes for the wrong reason (a Turtle syntax error rather than a prefix error), that is acceptable: both are exit-2 configuration errors.

- [ ] **Step 5: Commit**

```bash
git add src/claim.rs src/lib.rs tests/claim.rs
git commit -m "feat(claim): classify Turtle fragments into typed claims"
```

---

### Task 7: The oracle, dispatching claims to reasoner queries

This is where the spec's two hard-won corrections live: `ClassAssertion` goes through `instances_of` (not `realize`, which returns only most-specific types), and `ObjectPropertyAssertion` goes through a class-expression query (not `inferred_object_property_values`, which omits reflexive self-loops).

**Files:**
- Create: `src/oracle.rs`, `tests/oracle.rs`, `tests/fixtures/parts.ttl`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `Claim`, `Literal` (Task 6), `Verdict`, `CheckOutcome`, `IndeterminateReason` (Task 1), `Loaded` (Task 2).
- Produces: `enum Expectation { Entailed, NotEntailed }`, `fn check(onto: &SetOntology<RcStr>, claim: &Claim, expect: Expectation) -> CheckOutcome`, `fn holds(onto: &SetOntology<RcStr>, claim: &Claim) -> Result<bool, String>`.

- [ ] **Step 1: Write the failing test**

`tests/fixtures/parts.ttl` (concatenate SULO with a small parts chain; the test builds it at runtime so the fixture stays small):

```turtle
@prefix sulo: <https://w3id.org/sulo/> .
@prefix ex:   <http://example.org/> .

ex:a sulo:isPartOf ex:b .
ex:b sulo:isPartOf ex:c .
ex:d a sulo:SpatialObject .
```

```rust
// tests/oracle.rs
use std::path::Path;

use sulo_testharness::claim::Claim;
use sulo_testharness::load::{load_file, merge};
use sulo_testharness::oracle::{Expectation, check, holds};
use sulo_testharness::verdict::Verdict;

const SULO: &str = "../sulo/sulo.ttl";

fn parts_ontology() -> horned_owl::ontology::set::SetOntology<horned_owl::model::RcStr> {
    let mut base = load_file(Path::new(SULO)).expect("SULO should load").ontology;
    let data = load_file(Path::new("tests/fixtures/parts.ttl"))
        .expect("parts fixture should load")
        .ontology;
    merge(&mut base, data);
    base
}

#[test]
fn transitivity_closes() {
    let onto = parts_ontology();
    let claim = Claim::ObjectPropertyAssertion {
        subject: "http://example.org/a".into(),
        property: "https://w3id.org/sulo/isPartOf".into(),
        object: "http://example.org/c".into(),
    };
    assert!(holds(&onto, &claim).unwrap(), "isPartOf is transitive");
}

#[test]
fn reflexivity_is_found_despite_property_values_omitting_self_loops() {
    // Regression guard: dispatching this through
    // inferred_object_property_values returns nothing, because it
    // does not emit reflexive self-loops. The oracle must use a
    // class-expression query instead.
    let onto = parts_ontology();
    let claim = Claim::ObjectPropertyAssertion {
        subject: "http://example.org/d".into(),
        property: "https://w3id.org/sulo/isPartOf".into(),
        object: "http://example.org/d".into(),
    };
    assert!(holds(&onto, &claim).unwrap(), "isPartOf is reflexive");
}

#[test]
fn subproperty_propagation_to_isin_fires() {
    let onto = parts_ontology();
    let claim = Claim::ObjectPropertyAssertion {
        subject: "http://example.org/a".into(),
        property: "https://w3id.org/sulo/isIn".into(),
        object: "http://example.org/c".into(),
    };
    assert!(holds(&onto, &claim).unwrap(), "isPartOf is a subproperty of isIn");
}

#[test]
fn class_assertion_uses_the_full_closure_not_most_specific_types() {
    // ex:d is asserted SpatialObject; Object is an ancestor. realize
    // would report only SpatialObject, so this must go via instances_of.
    let onto = parts_ontology();
    let claim = Claim::ClassAssertion {
        individual: "http://example.org/d".into(),
        class: "https://w3id.org/sulo/Object".into(),
    };
    assert!(holds(&onto, &claim).unwrap(), "SpatialObject is under Object");
}

#[test]
fn the_deep_subsumption_chain_closes() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let claim = Claim::Subsumption {
        sub: "https://w3id.org/sulo/StartTime".into(),
        sup: "https://w3id.org/sulo/Object".into(),
    };
    assert!(holds(&onto, &claim).unwrap());
}

#[test]
fn a_known_non_subsumption_does_not_hold() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let claim = Claim::Subsumption {
        sub: "https://w3id.org/sulo/Process".into(),
        sup: "https://w3id.org/sulo/Object".into(),
    };
    assert!(!holds(&onto, &claim).unwrap(), "Process is disjoint from Object");
}

#[test]
fn expectation_entailed_and_holding_is_a_trustworthy_pass() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let claim = Claim::Subsumption {
        sub: "https://w3id.org/sulo/Role".into(),
        sup: "https://w3id.org/sulo/Feature".into(),
    };
    assert_eq!(check(&onto, &claim, Expectation::Entailed).verdict, Verdict::Pass);
}

#[test]
fn expectation_not_entailed_and_not_holding_is_only_unrefuted() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let claim = Claim::Subsumption {
        sub: "https://w3id.org/sulo/Process".into(),
        sup: "https://w3id.org/sulo/Object".into(),
    };
    // Absence of a proof is not proof of absence.
    assert_eq!(
        check(&onto, &claim, Expectation::NotEntailed).verdict,
        Verdict::UnrefutedPass
    );
}

#[test]
fn expectation_not_entailed_but_holding_is_a_trustworthy_fail() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let claim = Claim::Subsumption {
        sub: "https://w3id.org/sulo/Role".into(),
        sup: "https://w3id.org/sulo/Feature".into(),
    };
    assert!(matches!(
        check(&onto, &claim, Expectation::NotEntailed).verdict,
        Verdict::Fail(_)
    ));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test oracle`
Expected: FAIL, unresolved import `sulo_testharness::oracle`.

- [ ] **Step 3: Write minimal implementation**

`src/oracle.rs`:

```rust
//! Dispatching claims to the reasoner.
//!
//! Two dispatch choices are deliberate and were measured:
//!
//! * `ClassAssertion` uses `instances_of`, which returns the full
//!   type closure. `realize` returns only most-specific types, so it
//!   would fail every non-leaf class assertion.
//! * `ObjectPropertyAssertion` uses a `p value o` class-expression
//!   query. `inferred_object_property_values` omits reflexive
//!   self-loops, so it would fail every reflexivity check even though
//!   the entailment holds.

use horned_owl::model::{
    Build, ClassExpression, Individual, NamedIndividual, ObjectPropertyExpression, RcStr,
};
use horned_owl::ontology::set::SetOntology;

use crate::claim::{Claim, Literal};
use crate::verdict::{CheckOutcome, IndeterminateReason, Verdict};

/// What the case says should happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expectation {
    Entailed,
    NotEntailed,
}

/// Does the claim hold under the reasoner? `Err` carries a message
/// for an Indeterminate verdict.
pub fn holds(onto: &SetOntology<RcStr>, claim: &Claim) -> Result<bool, String> {
    match claim {
        Claim::Subsumption { sub, sup } => {
            owl_dl_reasoner::is_subclass_of(onto, sub, sup).map_err(|e| e.to_string())
        }
        Claim::Equivalence { left, right } => {
            let a = owl_dl_reasoner::is_subclass_of(onto, left, right)
                .map_err(|e| e.to_string())?;
            let b = owl_dl_reasoner::is_subclass_of(onto, right, left)
                .map_err(|e| e.to_string())?;
            Ok(a && b)
        }
        Claim::Unsatisfiable { class } => owl_dl_reasoner::is_class_satisfiable(onto, class)
            .map(|sat| !sat)
            .map_err(|e| e.to_string()),
        Claim::ClassAssertion { individual, class } => {
            // Full closure, not most-specific types.
            owl_dl_reasoner::is_instance_of(onto, class, individual).map_err(|e| e.to_string())
        }
        Claim::ObjectPropertyAssertion { subject, property, object } => {
            let build: Build<RcStr> = Build::new();
            let ce = ClassExpression::ObjectHasValue {
                ope: ObjectPropertyExpression::ObjectProperty(build.object_property(property.as_str())),
                i: Individual::Named(NamedIndividual(build.iri(object.as_str()))),
            };
            let inst = owl_dl_reasoner::class_expression_instances(onto, &ce)
                .map_err(|e| e.to_string())?;
            Ok(inst.individuals().iter().any(|i| i == subject))
        }
        Claim::DataPropertyAssertion { subject, property, literal } => {
            let build: Build<RcStr> = Build::new();
            let ce = ClassExpression::DataHasValue {
                dp: build.data_property(property.as_str()),
                l: to_horned_literal(&build, literal),
            };
            let inst = owl_dl_reasoner::class_expression_instances(onto, &ce)
                .map_err(|e| e.to_string())?;
            Ok(inst.individuals().iter().any(|i| i == subject))
        }
    }
}

fn to_horned_literal(build: &Build<RcStr>, lit: &Literal) -> horned_owl::model::Literal<RcStr> {
    match &lit.language {
        Some(lang) => horned_owl::model::Literal::Language {
            literal: lit.lexical.clone(),
            lang: lang.clone(),
        },
        None => horned_owl::model::Literal::Datatype {
            literal: lit.lexical.clone(),
            datatype_iri: build.iri(lit.datatype.as_str()),
        },
    }
}

/// Run one claim against its expectation and produce a verdict.
///
/// The asymmetry is the whole point. A reasoner that says "entailed"
/// is trustworthy because it is sound. A reasoner that says "not
/// entailed" has only failed to find a proof, so a negative
/// expectation it satisfies yields `UnrefutedPass`, not `Pass`.
pub fn check(onto: &SetOntology<RcStr>, claim: &Claim, expect: Expectation) -> CheckOutcome {
    let name = format!("{claim:?}");

    let verdict = match holds(onto, claim) {
        Err(msg) => Verdict::Indeterminate(IndeterminateReason::OracleError(msg)),
        Ok(true) => match expect {
            Expectation::Entailed => Verdict::Pass,
            Expectation::NotEntailed => {
                Verdict::Fail(format!("expected NOT entailed, but it is entailed: {claim:?}"))
            }
        },
        Ok(false) => match expect {
            Expectation::Entailed => Verdict::Fail(format!(
                "expected entailed, but no proof was found: {claim:?}. \
                 Incompleteness is a possible cause; the CI differential settles it."
            )),
            Expectation::NotEntailed => Verdict::UnrefutedPass,
        },
    };

    CheckOutcome { name, verdict }
}
```

Add to `src/lib.rs`:

```rust
pub mod oracle;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test oracle`
Expected: PASS, 9 tests. The tests read `../sulo/sulo.ttl`, so the sibling checkout must exist. If `ClassExpression::ObjectHasValue` or `DataHasValue` field names differ at the pinned rev, run `cargo doc -p horned-owl --open` and match the real variant shape; the query being a `p value o` expression is what matters.

- [ ] **Step 5: Commit**

```bash
git add src/oracle.rs src/lib.rs tests/oracle.rs tests/fixtures/parts.ttl
git commit -m "feat(oracle): dispatch claims to reasoner queries

ClassAssertion via instances_of (realize gives only most-specific
types) and ObjectPropertyAssertion via a 'p value o' class-expression
query (inferred_object_property_values omits reflexive self-loops).
Both dispatch choices carry a regression test."
```

---

### Task 8: Manchester class-expression claims

**Files:**
- Modify: `src/claim.rs`, `src/oracle.rs`
- Create: `tests/manchester.rs`

**Interfaces:**
- Consumes: `SubsumptionExpr`, `InstanceExpr` (Task 5), `PrefixMapping` (Task 4).
- Produces: `fn parse_ce(expr: &str, pm: &PrefixMapping) -> Result<ClassExpression<RcStr>, ClaimError>`, and on the oracle: `fn check_subsumption_expr`, `fn check_instance_expr`, `fn check_satisfiable_expr`, each returning `CheckOutcome`.

- [ ] **Step 1: Write the failing test**

```rust
// tests/manchester.rs
use std::path::Path;

use sulo_testharness::claim::parse_ce;
use sulo_testharness::load::load_file;
use sulo_testharness::oracle::{Expectation, check_subsumption_expr};
use sulo_testharness::prefixes::base_mapping;
use sulo_testharness::verdict::Verdict;

const SULO: &str = "../sulo/sulo.ttl";

#[test]
fn curies_resolve_via_the_prefix_map() {
    // No rewriting to full <IRI> needed: parse_class_expression takes
    // the PrefixMapping directly.
    let ce = parse_ce("sulo:Capability or sulo:Role", &base_mapping());
    assert!(ce.is_ok(), "expected a parse, got {ce:?}");
}

#[test]
fn feature_covering_is_entailed() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let out = check_subsumption_expr(
        &onto,
        "sulo:Feature",
        "sulo:Capability or sulo:InformationObject or sulo:Quality or sulo:Role",
        Expectation::Entailed,
        &base_mapping(),
    );
    assert_eq!(out.verdict, Verdict::Pass, "the Feature disjoint union covers");
}

#[test]
fn object_non_covering_is_not_entailed() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let out = check_subsumption_expr(
        &onto,
        "sulo:Object",
        "sulo:SpatialObject or sulo:Feature",
        Expectation::NotEntailed,
        &base_mapping(),
    );
    // Object deliberately has no covering axiom.
    assert_eq!(out.verdict, Verdict::UnrefutedPass);
}

#[test]
fn a_tautology_is_entailed_even_without_the_ontology() {
    // Guard against the schema example regression: C and D <= C holds
    // in any ontology, so such a case proves nothing. This test
    // documents the trap rather than endorsing it.
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let out = check_subsumption_expr(
        &onto,
        "sulo:Process and sulo:hasParticipant some sulo:Role",
        "sulo:Process",
        Expectation::Entailed,
        &base_mapping(),
    );
    assert_eq!(out.verdict, Verdict::Pass);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test manchester`
Expected: FAIL, `parse_ce` and `check_subsumption_expr` do not exist.

- [ ] **Step 3: Write minimal implementation**

Add to `src/claim.rs`:

```rust
use horned_owl::model::{Build, ClassExpression, RcStr};

/// Parse a Manchester class expression. CURIEs resolve against `pm`,
/// so no rewriting to full `<IRI>` form is needed.
pub fn parse_ce(expr: &str, pm: &PrefixMapping) -> Result<ClassExpression<RcStr>, ClaimError> {
    let build: Build<RcStr> = Build::new();
    horned_owl::io::omn::reader::parse_class_expression(expr, pm, &build)
        .map_err(|e| ClaimError::Syntax(format!("Manchester expression '{expr}': {e}")))
}
```

Add to `src/oracle.rs`:

```rust
use crate::claim::parse_ce;
use curie::PrefixMapping;

/// Turn a raw bool-plus-expectation into a verdict, shared by all the
/// class-expression checks. Identical asymmetry to `check`.
fn verdict_for(held: bool, expect: Expectation, what: &str) -> Verdict {
    match (held, expect) {
        (true, Expectation::Entailed) => Verdict::Pass,
        (true, Expectation::NotEntailed) => {
            Verdict::Fail(format!("expected NOT to hold, but it does: {what}"))
        }
        (false, Expectation::Entailed) => Verdict::Fail(format!(
            "expected to hold, but no proof was found: {what}. \
             Incompleteness is a possible cause; the CI differential settles it."
        )),
        (false, Expectation::NotEntailed) => Verdict::UnrefutedPass,
    }
}

/// `sub_expr` subsumed by `sup_expr`?
pub fn check_subsumption_expr(
    onto: &SetOntology<RcStr>,
    sub_expr: &str,
    sup_expr: &str,
    expect: Expectation,
    pm: &PrefixMapping,
) -> CheckOutcome {
    let what = format!("{sub_expr} subClassOf {sup_expr}");
    let (sub, sup) = match (parse_ce(sub_expr, pm), parse_ce(sup_expr, pm)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            return CheckOutcome {
                name: what,
                verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
            };
        }
    };

    let verdict = match owl_dl_reasoner::class_expression_entailed_subclass(onto, &sub, &sup) {
        Ok(v) => verdict_for(v.holds(), expect, &what),
        Err(e) => Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
    };

    CheckOutcome { name: what, verdict }
}

/// Is `individual` provably in `expr`?
pub fn check_instance_expr(
    onto: &SetOntology<RcStr>,
    individual: &str,
    expr: &str,
    expect: Expectation,
    pm: &PrefixMapping,
) -> CheckOutcome {
    let what = format!("{individual} instanceOf {expr}");
    let ce = match parse_ce(expr, pm) {
        Ok(c) => c,
        Err(e) => {
            return CheckOutcome {
                name: what,
                verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
            };
        }
    };

    let verdict = match owl_dl_reasoner::class_expression_instances(onto, &ce) {
        Ok(inst) => {
            let held = inst.individuals().iter().any(|i| i == individual);
            verdict_for(held, expect, &what)
        }
        Err(e) => Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
    };

    CheckOutcome { name: what, verdict }
}

/// Does `expr` have a model? Guards a pattern going unsatisfiable.
pub fn check_satisfiable_expr(
    onto: &SetOntology<RcStr>,
    expr: &str,
    pm: &PrefixMapping,
) -> CheckOutcome {
    let what = format!("satisfiable: {expr}");
    let ce = match parse_ce(expr, pm) {
        Ok(c) => c,
        Err(e) => {
            return CheckOutcome {
                name: what,
                verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
            };
        }
    };

    let verdict = match owl_dl_reasoner::class_expression_satisfiable(onto, &ce) {
        Ok(v) if v.holds() => Verdict::Pass,
        Ok(_) => Verdict::Fail(format!("expression is unsatisfiable: {expr}")),
        Err(e) => Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
    };

    CheckOutcome { name: what, verdict }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test manchester`
Expected: PASS, 4 tests.

- [ ] **Step 5: Commit**

```bash
git add src/claim.rs src/oracle.rs tests/manchester.rs
git commit -m "feat(oracle): Manchester class-expression checks

Covering and non-covering are entailment checks, not consistency
probes, because rustdl does not enforce the covering half of a
DisjointUnion in the ABox path."
```

---

### Task 9: The consistency gate, the runner, and symmetric loss downgrade

**Files:**
- Create: `src/suite.rs`, `src/report.rs`, `tests/suite.rs`
- Modify: `src/lib.rs`, `src/main.rs`

**Interfaces:**
- Consumes: everything from Tasks 1 to 8.
- Produces: `struct CaseResult { pub id: String, pub verdict: Verdict, pub checks: Vec<CheckOutcome>, pub skipped: bool }`, `fn run_case(case: &Case, default_ontology: &Path) -> CaseResult`, `fn run_suite(dir: &Path, default_ontology: &Path) -> Vec<CaseResult>`, `fn downgrade_for_loss(outcomes: &mut Vec<CheckOutcome>, loss: &[String])`, `fn render(results: &[CaseResult]) -> String`.

- [ ] **Step 1: Write the failing test**

```rust
// tests/suite.rs
use sulo_testharness::suite::downgrade_for_loss;
use sulo_testharness::verdict::{CheckOutcome, IndeterminateReason, Verdict};

fn o(v: Verdict) -> CheckOutcome {
    CheckOutcome { name: "c".into(), verdict: v }
}

#[test]
fn loss_downgrades_all_four_untrusted_outcomes() {
    let loss = vec!["parse: 4 triples unconsumed".to_string()];
    let mut outs = vec![
        // Trustworthy under monotonicity: entailed by a subset is
        // entailed by the whole. Must survive.
        o(Verdict::Pass),
        o(Verdict::Fail("expected NOT entailed, but it is entailed: x".into())),
        // Untrusted: rests on "no proof found". Must downgrade.
        o(Verdict::UnrefutedPass),
        o(Verdict::Fail("expected entailed, but no proof was found: y".into())),
    ];

    downgrade_for_loss(&mut outs, &loss);

    assert_eq!(outs[0].verdict, Verdict::Pass, "a positive Pass stays trusted");
    assert!(
        matches!(outs[1].verdict, Verdict::Fail(_)),
        "a negative Fail stays trusted"
    );
    assert!(
        matches!(outs[2].verdict, Verdict::Indeterminate(IndeterminateReason::AxiomLoss(_))),
        "an unrefuted pass rests on absence of proof and must downgrade"
    );
    assert!(
        matches!(outs[3].verdict, Verdict::Indeterminate(IndeterminateReason::AxiomLoss(_))),
        "a positive Fail may be a loss artifact and must downgrade"
    );
}

#[test]
fn no_loss_changes_nothing() {
    let mut outs = vec![o(Verdict::UnrefutedPass), o(Verdict::Pass)];
    let before = outs.clone();
    downgrade_for_loss(&mut outs, &[]);
    assert_eq!(outs, before);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test suite`
Expected: FAIL, unresolved import `sulo_testharness::suite`.

- [ ] **Step 3: Write minimal implementation**

`src/suite.rs`:

```rust
//! Case orchestration.
//!
//! Two rules carry most of the weight:
//!
//! 1. The consistency gate runs first. An inconsistent ontology
//!    entails everything, so running checks against one produces
//!    meaningless passes. Remaining checks are SKIPPED, never passed.
//! 2. Axiom loss downgrades every verdict that rests on "no proof
//!    found". Reasoning over a subset O' of O is monotonic: entailed
//!    by O' implies entailed by O, so a positive Pass and a negative
//!    Fail stay trustworthy. "Not entailed by O'" says nothing about
//!    O, and that answer underlies four outcomes, not one.

use std::path::Path;

use crate::claim::parse_fragment;
use crate::load::{load_file, merge};
use crate::manifest::Case;
use crate::oracle::{
    Expectation, check, check_instance_expr, check_satisfiable_expr, check_subsumption_expr,
};
use crate::prefixes::{base_mapping, with_overrides};
use crate::verdict::{CheckOutcome, IndeterminateReason, Verdict, aggregate};

pub struct CaseResult {
    pub id: String,
    pub verdict: Verdict,
    pub checks: Vec<CheckOutcome>,
    /// True when the consistency gate stopped the case early.
    pub skipped: bool,
}

/// Downgrade the verdicts that rest on an absence of proof.
pub fn downgrade_for_loss(outcomes: &mut Vec<CheckOutcome>, loss: &[String]) {
    if loss.is_empty() {
        return;
    }
    let reason = loss.join("; ");

    for out in outcomes.iter_mut() {
        let untrusted = match &out.verdict {
            // Rests on "no proof found".
            Verdict::UnrefutedPass => true,
            // A positive expectation that found no proof.
            Verdict::Fail(msg) => msg.contains("no proof was found"),
            _ => false,
        };
        if untrusted {
            out.verdict =
                Verdict::Indeterminate(IndeterminateReason::AxiomLoss(reason.clone()));
        }
    }
}

/// Run one case end to end.
pub fn run_case(case: &Case, default_ontology: &Path) -> CaseResult {
    let mut checks = Vec::new();

    // Resolve and load.
    let onto_path = case
        .ontology
        .as_ref()
        .map(|p| case.base_dir.join(p))
        .unwrap_or_else(|| default_ontology.to_path_buf());

    let loaded = match load_file(&onto_path) {
        Ok(l) => l,
        Err(e) => {
            return CaseResult {
                id: case.id.clone(),
                verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
                checks,
                skipped: true,
            };
        }
    };

    let mut onto = loaded.ontology;
    let mut loss = loaded.loss;

    for extra in case.imports.iter().chain(case.data.iter()) {
        match load_file(&case.base_dir.join(extra)) {
            Ok(l) => {
                loss.extend(l.loss);
                merge(&mut onto, l.ontology);
            }
            Err(e) => {
                return CaseResult {
                    id: case.id.clone(),
                    verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(
                        e.to_string(),
                    )),
                    checks,
                    skipped: true,
                };
            }
        }
    }

    let pm = with_overrides(&base_mapping(), &case.prefixes);

    // Gate: consistency before anything else.
    let consistent = match owl_dl_reasoner::is_consistent(&onto) {
        Ok(c) => c,
        Err(e) => {
            return CaseResult {
                id: case.id.clone(),
                verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
                checks,
                skipped: true,
            };
        }
    };

    let gate = match (case.expect_inconsistent, consistent) {
        (true, false) => CheckOutcome {
            name: "gate: expected inconsistent".into(),
            verdict: Verdict::Pass,
        },
        (true, true) => CheckOutcome {
            name: "gate: expected inconsistent".into(),
            // "Consistent" is the direction soundness does not vouch
            // for, and is_consistent exposes no incomplete flag. The
            // CI differential settles it.
            verdict: Verdict::Fail(
                "expected inconsistent, but the reasoner found it consistent; \
                 an axiom may have stopped biting. Routed to the CI differential."
                    .into(),
            ),
        },
        (false, false) => CheckOutcome {
            name: "gate: expected consistent".into(),
            verdict: Verdict::Fail(
                "ontology plus data is inconsistent, so every entailment check \
                 below would pass vacuously. Remaining checks skipped."
                    .into(),
            ),
        },
        (false, true) => CheckOutcome {
            name: "gate: expected consistent".into(),
            verdict: Verdict::Pass,
        },
    };

    let gate_stops_here = matches!(gate.verdict, Verdict::Fail(_)) || case.expect_inconsistent;
    let gate_failed = matches!(gate.verdict, Verdict::Fail(_));
    checks.push(gate);

    if gate_stops_here {
        downgrade_for_loss(&mut checks, &loss);
        let verdict = aggregate(&checks);
        return CaseResult {
            id: case.id.clone(),
            verdict,
            checks,
            // An expect_inconsistent case that passed its gate is
            // complete, not skipped, but its other checks are skipped.
            skipped: !gate_failed,
        };
    }

    // Positive and negative Turtle-fragment claims.
    for (fragment, expect) in [
        (case.entails.as_ref(), Expectation::Entailed),
        (case.not_entails.as_ref(), Expectation::NotEntailed),
    ] {
        if let Some(text) = fragment {
            match parse_fragment(text, &pm) {
                Ok(claims) => {
                    for claim in &claims {
                        checks.push(check(&onto, claim, expect));
                    }
                }
                Err(e) => checks.push(CheckOutcome {
                    name: "fragment parse".into(),
                    verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(
                        e.to_string(),
                    )),
                }),
            }
        }
    }

    // Class-expression claims.
    for s in &case.entails_manchester {
        checks.push(check_subsumption_expr(
            &onto,
            &s.sub_expr,
            &s.sup_expr,
            Expectation::Entailed,
            &pm,
        ));
    }
    for s in &case.not_entails_manchester {
        checks.push(check_subsumption_expr(
            &onto,
            &s.sub_expr,
            &s.sup_expr,
            Expectation::NotEntailed,
            &pm,
        ));
    }
    for i in &case.instance_of_expr {
        checks.push(check_instance_expr(
            &onto,
            &i.individual,
            &i.expr,
            Expectation::Entailed,
            &pm,
        ));
    }
    for e in &case.satisfiable_expr {
        checks.push(check_satisfiable_expr(&onto, e, &pm));
    }
    for class in &case.unsatisfiable {
        let claim = crate::claim::Claim::Unsatisfiable {
            class: crate::prefixes::expand(&pm, class).unwrap_or_else(|_| class.clone()),
        };
        checks.push(check(&onto, &claim, Expectation::Entailed));
    }

    downgrade_for_loss(&mut checks, &loss);
    let verdict = aggregate(&checks);

    CaseResult { id: case.id.clone(), verdict, checks, skipped: false }
}
```

`src/report.rs`:

```rust
//! Human-readable output.

use crate::suite::CaseResult;
use crate::verdict::Verdict;

/// Render results, worst first within each case.
#[must_use]
pub fn render(results: &[CaseResult]) -> String {
    let mut out = String::new();
    let mut unrefuted = 0usize;

    for r in results {
        let tag = match &r.verdict {
            Verdict::Pass => "PASS",
            Verdict::UnrefutedPass => "PASS*",
            Verdict::Indeterminate(_) => "INDET",
            Verdict::Fail(_) => "FAIL",
        };
        out.push_str(&format!("{tag:<6} {}\n", r.id));

        for c in &r.checks {
            match &c.verdict {
                Verdict::UnrefutedPass => unrefuted += 1,
                Verdict::Fail(msg) => out.push_str(&format!("         {msg}\n")),
                Verdict::Indeterminate(reason) => {
                    out.push_str(&format!("         indeterminate: {reason:?}\n"));
                }
                Verdict::Pass => {}
            }
        }

        if r.skipped {
            out.push_str("         remaining checks skipped (see gate)\n");
        }
    }

    if unrefuted > 0 {
        out.push_str(&format!(
            "\n{unrefuted} check(s) marked PASS* : a negative expectation the \
             reasoner failed to refute, not a proof of non-entailment.\n"
        ));
    }

    out
}
```

Add to `src/lib.rs`:

```rust
pub mod report;
pub mod suite;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS, all tests across all files.

- [ ] **Step 5: Commit**

```bash
git add src/suite.rs src/report.rs src/lib.rs tests/suite.rs
git commit -m "feat(suite): consistency gate, case runner, symmetric loss downgrade

The gate skips rather than passes remaining checks, because an
inconsistent ontology entails everything. Loss downgrades all four
outcomes that rest on 'no proof found', not just the positive Fail."
```

---

### Task 10: The mutation self-test

The harness's own regression suite. A green harness that catches nothing is the real failure mode, and three of the spec's original mutation mappings were wrong, so this is built before the bulk of the suite rather than after.

**Files:**
- Create: `mutants/README.md`, `mutants/*.ttl` (7 files), `tests/mutation.rs`, `suites/proof/*.yaml`
- Modify: none

**Interfaces:**
- Consumes: `run_case`, `load_case`, `Verdict`.
- Produces: no library API. A test binary asserting each mutant is caught by a named case.

- [ ] **Step 1: Write the failing test**

First create a minimal proof suite, `suites/proof/role-chain.yaml`:

```yaml
id: pro-role-chain
description: The PRO role chain recovers the role holder's participation.
data: data/pro-encounter.ttl
prefixes:
  ex: http://example.org/
entails: |
  ex:encounter sulo:hasParticipant ex:alice .
  ex:encounter sulo:hasParticipant ex:drsmith .
```

`suites/proof/data/pro-encounter.ttl`:

```turtle
@prefix sulo: <https://w3id.org/sulo/> .
@prefix ex:   <http://example.org/> .

ex:encounter a sulo:Process ;
    sulo:hasParticipant ex:alice_role, ex:smith_role .

ex:alice_role a sulo:Role ;
    sulo:isFeatureOf ex:alice .

ex:smith_role a sulo:Role ;
    sulo:isFeatureOf ex:drsmith .

ex:alice   a sulo:SpatialObject .
ex:drsmith a sulo:SpatialObject .
```

`suites/proof/covering-feature.yaml`:

```yaml
id: covering-feature
description: The Feature disjoint union covers its four members.
entails_manchester:
  - sub_expr: "sulo:Feature"
    sup_expr: "sulo:Capability or sulo:InformationObject or sulo:Quality or sulo:Role"
```

`suites/proof/transitivity-ispartof.yaml`:

```yaml
id: transitivity-ispartof
description: isPartOf is transitive, so a three-step chain closes.
data: data/parts.ttl
prefixes:
  ex: http://example.org/
entails: |
  ex:a sulo:isPartOf ex:c .
```

`suites/proof/data/parts.ttl`:

```turtle
@prefix sulo: <https://w3id.org/sulo/> .
@prefix ex:   <http://example.org/> .

ex:a sulo:isPartOf ex:b .
ex:b sulo:isPartOf ex:c .
```

`suites/proof/subproperty-isin.yaml`:

```yaml
id: subproperty-isin
description: isPartOf is a subproperty of isIn, so parthood implies containment.
data: data/parts.ttl
prefixes:
  ex: http://example.org/
entails: |
  ex:a sulo:isIn ex:c .
```

Now the test:

```rust
// tests/mutation.rs
//! Mutation self-test.
//!
//! Each mutant is a deliberately broken copy of sulo.ttl. For every
//! (mutant, case) pair listed here, the case MUST fail on the mutant
//! and MUST pass on clean SULO. A mutant nothing catches is a
//! coverage hole in the suite, not a passing test.

use std::path::{Path, PathBuf};

use sulo_testharness::manifest::load_case;
use sulo_testharness::suite::run_case;
use sulo_testharness::verdict::Verdict;

const CLEAN: &str = "../sulo/sulo.ttl";

fn verdict_of(case_file: &str, ontology: &Path) -> Verdict {
    let case = load_case(Path::new(case_file)).expect("case should parse");
    run_case(&case, ontology).verdict
}

fn assert_caught(mutant: &str, case_file: &str) {
    let mutant_path = PathBuf::from("mutants").join(mutant);

    let clean = verdict_of(case_file, Path::new(CLEAN));
    assert!(
        matches!(clean, Verdict::Pass | Verdict::UnrefutedPass),
        "{case_file} must pass on clean SULO, got {clean:?}"
    );

    let mutated = verdict_of(case_file, &mutant_path);
    assert!(
        matches!(mutated, Verdict::Fail(_)),
        "{case_file} must FAIL on mutant {mutant}, got {mutated:?}. \
         An uncaught mutant is a coverage hole."
    );
}

#[test]
fn deleting_the_role_chain_breaks_the_pro_case() {
    assert_caught("no-role-chain.ttl", "suites/proof/role-chain.yaml");
}

#[test]
fn dropping_ispartof_transitivity_breaks_the_transitivity_case() {
    assert_caught("no-transitive-ispartof.ttl", "suites/proof/transitivity-ispartof.yaml");
}

#[test]
fn deleting_the_feature_disjoint_union_breaks_only_the_covering_case() {
    assert_caught("no-feature-union.ttl", "suites/proof/covering-feature.yaml");
}

#[test]
fn deleting_the_ispartof_isin_subproperty_axiom_breaks_the_isin_case() {
    assert_caught("no-subproperty-isin.ttl", "suites/proof/subproperty-isin.yaml");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test mutation`
Expected: FAIL, the `mutants/` files do not exist, so `load_file` errors and the clean-pass assertion or the mutant path fails.

- [ ] **Step 3: Write minimal implementation**

Generate each mutant from clean SULO with a single documented edit. Run these from the repository root:

```bash
mkdir -p mutants

# 1. Delete the PRO role chain.
grep -v "owl:propertyChainAxiom" ../sulo/sulo.ttl > mutants/no-role-chain.ttl

# 2. Drop isPartOf's transitivity. isPartOf declares
#    "owl:ReflexiveProperty, owl:TransitiveProperty ;" so remove only
#    the transitive half and keep the reflexive one.
python3 - <<'PY'
import re
src = open('../sulo/sulo.ttl').read()
# Narrow the edit to the isPartOf block so hasPart keeps its axioms.
start = src.index('sulo:isPartOf a owl:ObjectProperty')
end = src.index('\n\n', start)
block = src[start:end]
patched = block.replace('owl:ReflexiveProperty,\n        owl:TransitiveProperty ;',
                        'owl:ReflexiveProperty ;')
assert patched != block, "transitivity pattern not found; inspect the block"
open('mutants/no-transitive-ispartof.ttl','w').write(src[:start] + patched + src[end:])
PY

# 3. Delete the Feature disjointUnionOf, leaving its redundant
#    AllDisjointClasses in place. Only the covering case should react.
grep -v "owl:disjointUnionOf ( sulo:Capability sulo:InformationObject sulo:Quality sulo:Role )" \
  ../sulo/sulo.ttl > mutants/no-feature-union.ttl

# 4. Delete the isPartOf -> isIn subproperty axiom (line 272 in clean SULO).
python3 - <<'PY'
src = open('../sulo/sulo.ttl').read()
needle = '    rdfs:subPropertyOf sulo:isIn .'
assert needle in src, "subPropertyOf isIn not found"
open('mutants/no-subproperty-isin.ttl','w').write(src.replace(needle, '    a owl:ObjectProperty .', 1))
PY
```

`mutants/README.md`:

```markdown
# Mutants

Each file is clean `sulo.ttl` with exactly one documented axiom
removed or weakened. `tests/mutation.rs` asserts that every mutant is
caught by a specific named case, and that the same case passes on
clean SULO.

A mutant nothing catches is a coverage hole in the suite, reported as
such. These are the harness's own regression tests: they are what
distinguishes a suite that guards the ontology from a suite that is
merely green.

| File | Edit | Case that must fail |
| --- | --- | --- |
| `no-role-chain.ttl` | removes `owl:propertyChainAxiom` on `hasParticipant` | `suites/proof/role-chain.yaml` |
| `no-transitive-ispartof.ttl` | removes `owl:TransitiveProperty` from `isPartOf`, keeps reflexivity | `suites/proof/transitivity-ispartof.yaml` |
| `no-feature-union.ttl` | removes `Feature`'s `disjointUnionOf`, keeps its `AllDisjointClasses` | `suites/proof/covering-feature.yaml` only |
| `no-subproperty-isin.ttl` | removes `isPartOf rdfs:subPropertyOf isIn` | `suites/proof/subproperty-isin.yaml` |

Note on `no-feature-union.ttl`: the sibling disjointness counter-examples
must NOT react to it, because the redundant `AllDisjointClasses` axiom
still asserts pairwise disjointness. An earlier version of this table
claimed otherwise, and was only ever right by accident, because
horned-owl drops `AllDisjointClasses` silently.
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test mutation`
Expected: PASS, 4 tests. If a mutant is not caught, do not weaken the assertion: either the mutant edit did not land (check with `diff ../sulo/sulo.ttl mutants/<file>`) or the case genuinely does not guard that axiom, which is a real coverage hole to fix by strengthening the case.

- [ ] **Step 5: Commit**

```bash
git add mutants/ suites/ tests/mutation.rs
git commit -m "test: mutation self-test proving the proof suite is load-bearing

Each mutant must be caught by a named case, and that case must pass on
clean SULO. Built before the bulk of the suite because three of the
spec's original mutation mappings were wrong and only a mutant would
have shown it."
```

---

### Task 11: The golden closure diff

The primary defence for the untrusted direction. It works because it does not care about completeness: both sides of the diff come from the same oracle at the same version, so whatever the reasoner cannot see is held constant and cancels out.

**Files:**
- Create: `src/golden.rs`, `tests/golden.rs`, `suites/sulo.golden`
- Modify: `src/lib.rs`, `src/main.rs`

**Interfaces:**
- Consumes: `Loaded` (Task 2).
- Produces: `fn closure(onto: &SetOntology<RcStr>) -> Result<String, String>`, `fn diff(current: &str, golden: &str) -> Option<String>`, `struct GoldenHeader { pub reasoner_version: String }`, `fn check_golden(onto: &SetOntology<RcStr>, path: &Path, accept: bool) -> GoldenOutcome`, `enum GoldenOutcome { Match, Drift(String), Rebaselined, RebaselineRequired(String) }`.

- [ ] **Step 1: Write the failing test**

```rust
// tests/golden.rs
use std::path::Path;

use sulo_testharness::golden::{closure, diff};
use sulo_testharness::load::load_file;

const SULO: &str = "../sulo/sulo.ttl";

#[test]
fn the_closure_is_deterministic() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let a = closure(&onto).unwrap();
    let b = closure(&onto).unwrap();
    assert_eq!(a, b, "closure must be byte-identical across runs");
}

#[test]
fn the_closure_is_sorted() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let text = closure(&onto).unwrap();
    let body: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    let mut sorted = body.clone();
    sorted.sort_unstable();
    assert_eq!(body, sorted, "closure lines must be sorted for a readable diff");
}

#[test]
fn the_closure_records_known_entailments() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let text = closure(&onto).unwrap();
    assert!(
        text.contains("subClassOf\thttps://w3id.org/sulo/StartTime\thttps://w3id.org/sulo/Object"),
        "the deep chain should appear in the closure"
    );
    assert!(
        text.contains("satisfiable\thttps://w3id.org/sulo/Process"),
        "every class's satisfiability should be recorded"
    );
}

#[test]
fn an_identical_closure_has_no_diff() {
    assert!(diff("a\nb\n", "a\nb\n").is_none());
}

#[test]
fn a_changed_closure_reports_the_lines() {
    let d = diff("a\nc\n", "a\nb\n").expect("should differ");
    assert!(d.contains('c') && d.contains('b'), "diff should name both sides: {d}");
}

#[test]
fn dropping_an_axiom_changes_the_closure() {
    // The mechanism the golden file exists for: a regression that no
    // hand-written case asserts still shows up as drift.
    let clean = closure(&load_file(Path::new(SULO)).unwrap().ontology).unwrap();
    let mutant =
        closure(&load_file(Path::new("mutants/no-subproperty-isin.ttl")).unwrap().ontology)
            .unwrap();
    assert_ne!(clean, mutant, "removing a subproperty axiom must move the closure");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test golden`
Expected: FAIL, unresolved import `sulo_testharness::golden`.

- [ ] **Step 3: Write minimal implementation**

`src/golden.rs`:

```rust
//! The golden inference closure.
//!
//! A regression harness needs to detect that the answer CHANGED, not
//! to know absolute truth. That is why this works despite the
//! reasoner being incomplete: both sides of the diff come from the
//! same oracle at the same version, so whatever it cannot see is held
//! constant and cancels out. It therefore guards every entailment in
//! the closure, not only the ones somebody thought to assert.
//!
//! The header pins the reasoner version. A version change legitimately
//! moves the closure, so it is reported as "re-baseline required"
//! rather than as drift or as a pass.

use std::collections::BTreeSet;
use std::path::Path;

use horned_owl::model::RcStr;
use horned_owl::ontology::set::SetOntology;

/// The reasoner version this closure was produced with.
pub const REASONER_VERSION: &str = "rustdl v0.4.22";

/// Outcome of comparing a closure to its golden file.
#[derive(Debug, PartialEq, Eq)]
pub enum GoldenOutcome {
    Match,
    Drift(String),
    Rebaselined,
    RebaselineRequired(String),
}

/// Serialise the full inferred closure, sorted and canonical.
pub fn closure(onto: &SetOntology<RcStr>) -> Result<String, String> {
    let classification = owl_dl_reasoner::classify(onto).map_err(|e| e.to_string())?;

    let mut lines: BTreeSet<String> = BTreeSet::new();

    for class in classification.classes() {
        // Satisfiability of every named class.
        let sat = owl_dl_reasoner::is_class_satisfiable(onto, class)
            .map_err(|e| e.to_string())?;
        lines.insert(format!("satisfiable\t{class}\t{sat}"));

        // Every entailed named subsumption, not only the direct ones,
        // so a lost intermediate axiom still shows as drift.
        for other in classification.classes() {
            if class == other {
                continue;
            }
            if owl_dl_reasoner::is_subclass_of(onto, class, other)
                .map_err(|e| e.to_string())?
            {
                lines.insert(format!("subClassOf\t{class}\t{other}"));
            }
        }
    }

    let mut out = String::new();
    out.push_str(&format!("# reasoner: {REASONER_VERSION}\n"));
    out.push_str("# generated by sulo-testharness; regenerate with --accept-golden\n");
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }

    Ok(out)
}

/// Line-level diff. `None` means identical.
#[must_use]
pub fn diff(current: &str, golden: &str) -> Option<String> {
    let cur: BTreeSet<&str> = current.lines().filter(|l| !l.starts_with('#')).collect();
    let gold: BTreeSet<&str> = golden.lines().filter(|l| !l.starts_with('#')).collect();

    if cur == gold {
        return None;
    }

    let mut out = String::new();
    for line in gold.difference(&cur) {
        out.push_str(&format!("- {line}\n"));
    }
    for line in cur.difference(&gold) {
        out.push_str(&format!("+ {line}\n"));
    }
    Some(out)
}

fn golden_reasoner_version(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("# reasoner: "))
        .map(str::to_string)
}

/// Compare against the golden file, optionally re-baselining.
pub fn check_golden(
    onto: &SetOntology<RcStr>,
    path: &Path,
    accept: bool,
) -> GoldenOutcome {
    let current = match closure(onto) {
        Ok(c) => c,
        Err(e) => return GoldenOutcome::Drift(format!("could not compute closure: {e}")),
    };

    if accept || !path.exists() {
        if let Err(e) = std::fs::write(path, &current) {
            return GoldenOutcome::Drift(format!("could not write golden file: {e}"));
        }
        return GoldenOutcome::Rebaselined;
    }

    let golden = match std::fs::read_to_string(path) {
        Ok(g) => g,
        Err(e) => return GoldenOutcome::Drift(format!("could not read golden file: {e}")),
    };

    match golden_reasoner_version(&golden) {
        Some(v) if v != REASONER_VERSION => {
            return GoldenOutcome::RebaselineRequired(format!(
                "golden file was produced with {v}, running {REASONER_VERSION}. \
                 A reasoner change legitimately moves the closure; \
                 review and re-run with --accept-golden."
            ));
        }
        None => {
            return GoldenOutcome::RebaselineRequired(
                "golden file has no reasoner version header".to_string(),
            );
        }
        Some(_) => {}
    }

    match diff(&current, &golden) {
        None => GoldenOutcome::Match,
        Some(d) => GoldenOutcome::Drift(d),
    }
}
```

Add to `src/lib.rs`:

```rust
pub mod golden;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test golden`
Expected: PASS, 6 tests. If `classification.classes()` is not the accessor name, check `cargo doc -p owl-dl-reasoner --open` for `Classification`; iterating the named classes is what matters. The pairwise subsumption loop is O(n squared) over 17 classes, which is fine here and must not be "optimised" into direct-subsumers-only, since that would stop detecting a lost intermediate axiom.

- [ ] **Step 5: Generate the initial golden file and confirm it is stable**

```bash
cargo run -- golden --ontology ../sulo/sulo.ttl --golden suites/sulo.golden --accept-golden
cargo run -- golden --ontology ../sulo/sulo.ttl --golden suites/sulo.golden
```

Expected: the first writes the file, the second reports a match and exits 0. Then confirm it catches a real regression:

```bash
cargo run -- golden --ontology mutants/no-subproperty-isin.ttl --golden suites/sulo.golden; echo "exit=$?"
```

Expected: drift reported, `exit=4`.

- [ ] **Step 6: Wire the CLI**

`src/main.rs`:

```rust
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sulo_testharness::golden::{GoldenOutcome, check_golden};
use sulo_testharness::load::load_file;

#[derive(Parser)]
#[command(name = "sulo-testharness", about = "Regression harness for the SULO ontology")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compare the inferred closure against a golden file.
    Golden {
        #[arg(long)]
        ontology: PathBuf,
        #[arg(long)]
        golden: PathBuf,
        /// Re-baseline instead of comparing.
        #[arg(long)]
        accept_golden: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Golden { ontology, golden, accept_golden } => {
            let loaded = match load_file(&ontology) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::from(2);
                }
            };

            for l in &loaded.loss {
                eprintln!("warning: axiom loss: {l}");
            }

            match check_golden(&loaded.ontology, &golden, accept_golden) {
                GoldenOutcome::Match => {
                    println!("golden closure matches");
                    ExitCode::SUCCESS
                }
                GoldenOutcome::Rebaselined => {
                    println!("golden closure written to {}", golden.display());
                    ExitCode::SUCCESS
                }
                GoldenOutcome::Drift(d) => {
                    println!("golden closure drift:\n{d}");
                    ExitCode::from(4)
                }
                GoldenOutcome::RebaselineRequired(m) => {
                    println!("re-baseline required: {m}");
                    ExitCode::from(4)
                }
            }
        }
    }
}
```

- [ ] **Step 7: Run the full test suite**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all tests pass, no clippy warnings, formatting clean.

- [ ] **Step 8: Commit**

```bash
git add src/golden.rs src/main.rs src/lib.rs tests/golden.rs suites/sulo.golden
git commit -m "feat(golden): incompleteness-invariant closure diff

Both sides of the diff come from the same oracle at the same version,
so what the reasoner cannot see is held constant and cancels out. The
header pins the reasoner version, so an upgrade reports re-baseline
required rather than drift or a silent pass."
```

---

## What this plan does not cover

Phases 5 to 8 of the spec, deferred to a second plan once the engine lands:

- **Competency questions**: the oxigraph store, the materialisation defined in spec section 8 step 6 (including the reflexive self-loops `inferred_object_property_values` omits), and the `expect_rows` comparison semantics of spec section 7.3.
- **The full SULO suite**: roughly 70 cases across taxonomy, properties, restrictions, domain and range, and the two patterns, plus the remaining mutants from spec section 10.
- **The HermiT differential**: spec section 5.3, the CI-only job that settles negative assertions, consistency verdicts, and the `oracle: hermit` cases rustdl cannot enforce (data-range `allValuesFrom`).
- **Release and consumer integration**: prebuilt binaries, `action.yml`, and the workflow pull request to `AIDAVA-DEV/sulo`.

## Self-review notes

Checked against the spec:

- Spec sections 5.1 (verdicts), 5.2 (golden diff), 5.4 (exit codes), 6.1 (claim table, all three notes), 7.1 (the `entails_manchester` reshape), 7.2 (prefix resolution), 8 steps 1 to 5 and 7 (pipeline minus CQs), 10 (mutation), 12 (error handling and the symmetric downgrade), and 13.1's implementation constraint all map to tasks above.
- Spec sections 5.3, 7.3, 8 step 6, 9, and 11 are the deferred set listed above.
- Task 7's dispatch choices carry the regression tests that would have caught the two "fails on healthy SULO" bugs from the review: `reflexivity_is_found_despite_property_values_omitting_self_loops` and `class_assertion_uses_the_full_closure_not_most_specific_types`.
- Task 3 tests both halves of `DisjointUnion` separately, so a future rustdl fix shows up as the covering test passing without the workaround rather than as a silent behaviour change.
- Types are consistent across tasks: `Loaded`, `Case`, `Claim`, `Expectation`, `CheckOutcome`, `Verdict`, `CaseResult`, `GoldenOutcome` are each defined once and referenced with the same field names throughout.
