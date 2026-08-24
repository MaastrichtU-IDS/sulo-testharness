# sulo-testharness: Competency Questions and the SULO Suite

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the harness the two things it still lacks: a competency-question path (SPARQL over the materialised inference closure) and the ~70-case SULO suite that actually makes an edit to `sulo.ttl` fail.

**Architecture:** The engine is complete and merged. This plan adds `cq:` to the manifest, a term-level row comparator, a materialiser that builds an in-memory oxigraph store from the reasoner's closure, and the suite content itself. No change to the verdict system, the consistency gate, the loss downgrade, or the oracle's dispatch.

**Tech Stack:** Rust (edition 2024, toolchain 1.95.0), the existing `horned-owl` / `owl-dl-reasoner` pins, plus `oxigraph 0.5.9` with `default-features = false`.

**Spec:** `docs/superpowers/specs/2026-08-21-sulo-testharness-design.md`. Sections 7.3 (row comparison), 8 step 6 (materialisation), and 9 (suite inventory) are the binding text for this plan.

**Predecessor:** `docs/superpowers/plans/2026-08-22-sulo-testharness-engine.md`, complete and merged (111 tests).

## Global Constraints

- **Rust toolchain 1.95.0**, edition 2024.
- **Do not touch the `horned-owl` pin.** It is `version = "1.4"` in `[dependencies]` plus a `[patch.crates-io]` redirect to git rev `b188edaf7c92600918f0524962d928097ecd6b4d`. Naming the git rev directly produces two incompatible copies of the crate.
- **`oxigraph` must be added as `default-features = false`.** Verified: the default features pull `oxrocksdb-sys` (a native RocksDB build) which this harness does not need; the in-memory `Store::new()` works without them.
- **Use `SparqlEvaluator`, never `Store::query`.** Verified: `Store::query` is deprecated in 0.5.9 ("Use `SparqlEvaluator` interface instead") and would fail the `-D warnings` gate.
- **Every reasoner call must be bounded.** The crate has a precedent of an unbounded call hanging 24 minutes 30 seconds. `is_consistent` in the gate is the one documented exception; do not add a second.
- **The harness must never overstate what it verified.** Four verdicts: `Pass`, `UnrefutedPass` (non-failing, counted separately), `Indeterminate`, `Fail`. A CQ that cannot be evaluated is `Indeterminate`, never a `Pass`.
- **Zero em-dash characters** anywhere in the repo, including report prose. It is currently at exactly zero.
- **No blank nodes in suite data.** `inferred_object_property_values` covers named individuals only, so blank nodes are invisible to the CQ path. Skolemise.
- **`cargo test` exits zero, `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --all -- --check` clean.** CI enforces all three.

## What already exists, and must not be rebuilt

Read these before writing anything; the engine moved a long way from its own plan.

| Module | Provides |
| --- | --- |
| `src/verdict.rs` | `Verdict`, `IndeterminateReason`, `CheckOutcome`, `aggregate`, `exit_code` |
| `src/load.rs` | `Loaded { ontology, loss, baseline_loss }`, `load_file`, `merge`, `lower_disjoint_unions`, `recover_all_disjoint_classes` |
| `src/prefixes.rs` | `base_mapping`, `with_overrides`, `expand`, `PrefixError` |
| `src/manifest.rs` | `Case`, `load_case`, `SubsumptionExpr`, `InstanceExpr`, `ManifestError` |
| `src/claim.rs` | `Claim`, `Literal`, `parse_fragment`, `parse_ce`, `ClaimError` |
| `src/oracle.rs` | `Expectation`, `holds_with_deadline`, `check`, `check_subsumption_expr`, `check_instance_expr`, `check_satisfiable_expr`, `verdict_for`, `probe_satisfiable`, `NO_PROOF_MARKER`, `REASONER_DEADLINE`, `Declared` |
| `src/suite.rs` | `run_case`, `CaseResult`, `downgrade_for_loss`, the gate |
| `src/golden.rs` | `closure`, `diff`, `check_golden` |
| `src/report.rs` | `render` |

Known limitations that constrain the suite, all measured and recorded in the spec:

- **A language-tagged literal can never be positively confirmed.** rustdl v0.4.22 cannot confirm `rdf:langString` `DataHasValue` membership by any path. A `@fr` literal under `entails:` is a permanent Fail. Suite cases must not do this.
- **`inferred_object_property_values` omits reflexive self-loops.** The materialiser must inject them or `?x sulo:isPartOf ?x` silently returns nothing.
- **XSD facet subtyping is unsupported.** `xsd:int` and `xsd:integer` are distinct to the fast path.
- **The golden closure detects 1 of 4 mutants.** Not this plan's problem, but do not assume it covers a new case.

## File Structure

| File | Responsibility |
| --- | --- |
| `Cargo.toml` | add `oxigraph` (modify) |
| `src/manifest.rs` | add `cq:` to `Case` (modify) |
| `src/rows.rs` | **new.** Term-level comparison of expected versus actual SPARQL rows |
| `src/materialize.rs` | **new.** Build an oxigraph store from asserted plus inferred triples |
| `src/cq.rs` | **new.** Run one CQ against a store, produce a `CheckOutcome` |
| `src/suite.rs` | drive `cq` per case (modify) |
| `suites/sulo/**` | **new.** The ~70-case suite, its data, and its queries |
| `mutants/` | new mutants for the new suite groups (modify) |

Tasks 1 to 5 are code. Tasks 6 to 10 are suite content: a different kind of work, where the deliverable is data and the risk is a case that asserts something vacuous rather than a compile error.

---

### Task 1: `cq` in the manifest schema

**Files:**
- Modify: `src/manifest.rs`
- Test: `tests/manifest.rs`
- Create: `tests/fixtures/case-with-cq.yaml`

**Interfaces:**
- Consumes: `Case`, `load_case`, `ManifestError`.
- Produces: `pub struct CqSpec { pub query: PathBuf, pub expect_rows: Vec<BTreeMap<String, Option<String>>>, pub exact: bool, pub ordered: bool }`, and `Case.cq: Vec<CqSpec>`.

`expect_rows` is a list of rows; each row maps a variable name to `Some(token)` or `None` for an expected-unbound. `exact` defaults `true`, `ordered` defaults `false`.

- [ ] **Step 1: Write the failing test**

`tests/fixtures/case-with-cq.yaml`:

```yaml
id: cq-shape
description: Exercises every corner of the cq schema.
data: data/parts.ttl
prefixes:
  ex: http://example.org/
cq:
  - query: queries/who.rq
    expect_rows:
      - { p: "ex:alice" }
      - { p: "ex:drsmith" }
  - query: queries/values.rq
    expect_rows:
      - { v: '"37.8"^^xsd:double', unit: "ex:celsius" }
      - { v: '"unbound-case"', unit: null }
    exact: false
    ordered: true
```

```rust
// tests/manifest.rs (append)
#[test]
fn parses_the_cq_block() {
    let c = load_case(Path::new("tests/fixtures/case-with-cq.yaml")).unwrap();
    assert_eq!(c.cq.len(), 2, "both cq entries parsed");

    let first = &c.cq[0];
    assert_eq!(first.query, Path::new("queries/who.rq"));
    assert_eq!(first.expect_rows.len(), 2);
    assert_eq!(
        first.expect_rows[0].get("p"),
        Some(&Some("ex:alice".to_string()))
    );
    assert!(first.exact, "exact defaults to true");
    assert!(!first.ordered, "ordered defaults to false");

    let second = &c.cq[1];
    assert!(!second.exact, "exact was set false");
    assert!(second.ordered, "ordered was set true");
    assert_eq!(
        second.expect_rows[1].get("unit"),
        Some(&None),
        "a null in YAML means expected-unbound, not a missing key"
    );
}

#[test]
fn a_case_with_only_a_cq_is_a_real_case() {
    // `cq` must satisfy the no-assertions guard from the engine plan,
    // otherwise a pure competency-question case is rejected.
    let c = load_case(Path::new("tests/fixtures/case-with-cq.yaml")).unwrap();
    assert!(!c.cq.is_empty());
}

#[test]
fn an_unknown_key_inside_cq_is_rejected() {
    // Same reasoning as the top-level deny_unknown_fields: a typo'd
    // `expect_row:` must not silently mean "no rows expected".
    let err = load_case(Path::new("tests/fixtures/case-cq-bad-key.yaml"))
        .expect_err("unknown key inside a cq entry must be an error");
    assert!(err.to_string().contains("expect_row"), "error names the key: {err}");
}
```

`tests/fixtures/case-cq-bad-key.yaml`:

```yaml
id: cq-typo
description: Has a typo'd key inside the cq block.
cq:
  - query: queries/who.rq
    expect_row:
      - { p: "ex:alice" }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test manifest`
Expected: FAIL, `Case` has no field `cq`.

- [ ] **Step 3: Write minimal implementation**

Add to `src/manifest.rs`:

```rust
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
```

Add `cq: Vec<CqSpec>` to both `RawCase` (with `#[serde(default)]`) and `Case`, and carry it through `load_case`.

Then extend the no-assertions guard so a non-empty `cq` counts as an assertion. Find the existing check that produces `ManifestError::NoAssertions` and add `cq.is_empty()` to the conjunction.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test manifest`
Expected: PASS. Then `cargo test` in full: the existing suite must be unaffected.

- [ ] **Step 5: Stub check**

Replace `load_case`'s `cq` assignment with `Vec::new()`. Confirm `parses_the_cq_block` and `a_case_with_only_a_cq_is_a_real_case` both fail. Revert. Report the numbers.

- [ ] **Step 6: Commit**

```bash
git add src/manifest.rs tests/manifest.rs tests/fixtures/
git commit -m "feat(manifest): cq block with expect_rows, exact and ordered"
```

---

### Task 2: Term-level row comparison

This is the highest-risk piece in the plan, because a comparator that is too lenient turns every competency question into a test that cannot fail. Spec section 7.3 is binding.

**Files:**
- Create: `src/rows.rs`, `tests/rows.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `prefixes::expand`, `curie::PrefixMapping`.
- Produces:

```rust
pub enum Expected { Bound(Term), Unbound }
pub fn parse_expected(token: Option<&str>, pm: &PrefixMapping) -> Result<Expected, RowError>;
pub struct RowSet { /* Vec<BTreeMap<String, Option<Term>>> */ }
pub fn compare(expected: &[BTreeMap<String, Option<Term>>],
               actual:   &[BTreeMap<String, Option<Term>>],
               exact: bool, ordered: bool) -> Result<(), String>;
pub enum RowError { Prefix(PrefixError), Syntax(String), BlankNode(String) }
```

`compare` returns `Ok(())` on a match, or `Err(explanation)` naming what was missing or extra.

- [ ] **Step 1: Write the failing test**

```rust
// tests/rows.rs
use std::collections::BTreeMap;
use oxrdf::{Literal, NamedNode, Term};
use sulo_testharness::prefixes::base_mapping;
use sulo_testharness::rows::{Expected, compare, parse_expected};

fn iri(s: &str) -> Term { Term::NamedNode(NamedNode::new(s).unwrap()) }

fn row(pairs: &[(&str, Option<Term>)]) -> BTreeMap<String, Option<Term>> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

#[test]
fn a_curie_expands_to_an_iri_term() {
    let e = parse_expected(Some("sulo:Process"), &base_mapping()).unwrap();
    assert_eq!(e, Expected::Bound(iri("https://w3id.org/sulo/Process")));
}

#[test]
fn an_angle_bracket_iri_passes_through() {
    let e = parse_expected(Some("<http://example.org/x>"), &base_mapping()).unwrap();
    assert_eq!(e, Expected::Bound(iri("http://example.org/x")));
}

#[test]
fn a_typed_literal_keeps_its_datatype() {
    let e = parse_expected(Some(r#""37.8"^^xsd:double"#), &base_mapping()).unwrap();
    let want = Term::Literal(Literal::new_typed_literal(
        "37.8",
        NamedNode::new("http://www.w3.org/2001/XMLSchema#double").unwrap(),
    ));
    assert_eq!(e, Expected::Bound(want));
}

#[test]
fn a_bare_literal_is_an_xsd_string_not_a_wildcard() {
    // Spec 7.3: literal equality is RDF TERM equality, so a bare
    // literal is xsd:string and does NOT equal "37.8"^^xsd:double.
    let bare = parse_expected(Some(r#""37.8""#), &base_mapping()).unwrap();
    let typed = parse_expected(Some(r#""37.8"^^xsd:double"#), &base_mapping()).unwrap();
    assert_ne!(bare, typed, "value-space equality would hide serialisation regressions");
}

#[test]
fn a_language_literal_keeps_its_tag() {
    let e = parse_expected(Some(r#""bonjour"@fr"#), &base_mapping()).unwrap();
    let want = Term::Literal(Literal::new_language_tagged_literal("bonjour", "fr").unwrap());
    assert_eq!(e, Expected::Bound(want));
}

#[test]
fn null_means_expected_unbound() {
    assert_eq!(parse_expected(None, &base_mapping()).unwrap(), Expected::Unbound);
}

#[test]
fn a_blank_node_is_a_configuration_error() {
    // Blank nodes never compare equal across runs.
    assert!(parse_expected(Some("_:b0"), &base_mapping()).is_err());
}

#[test]
fn an_unbound_prefix_is_an_error() {
    assert!(parse_expected(Some("nope:thing"), &base_mapping()).is_err());
}

#[test]
fn exact_compare_rejects_an_extra_actual_row() {
    let e = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    let a = vec![
        row(&[("p", Some(iri("http://example.org/a")))]),
        row(&[("p", Some(iri("http://example.org/b")))]),
    ];
    assert!(compare(&e, &a, true, false).is_err(), "exact must reject extras");
    assert!(compare(&e, &a, false, false).is_ok(), "subset must allow extras");
}

#[test]
fn subset_still_rejects_a_missing_expected_row() {
    let e = vec![
        row(&[("p", Some(iri("http://example.org/a")))]),
        row(&[("p", Some(iri("http://example.org/z")))]),
    ];
    let a = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    assert!(compare(&e, &a, false, false).is_err(), "subset is not 'anything goes'");
}

#[test]
fn unordered_compare_ignores_position() {
    let e = vec![
        row(&[("p", Some(iri("http://example.org/a")))]),
        row(&[("p", Some(iri("http://example.org/b")))]),
    ];
    let a = vec![
        row(&[("p", Some(iri("http://example.org/b")))]),
        row(&[("p", Some(iri("http://example.org/a")))]),
    ];
    assert!(compare(&e, &a, true, false).is_ok());
    assert!(compare(&e, &a, true, true).is_err(), "ordered must respect position");
}

#[test]
fn duplicate_rows_are_significant() {
    // Multiset, not set: a query returning a row twice is a different
    // answer from one returning it once.
    let e = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    let a = vec![
        row(&[("p", Some(iri("http://example.org/a")))]),
        row(&[("p", Some(iri("http://example.org/a")))]),
    ];
    assert!(compare(&e, &a, true, false).is_err(), "duplicates must not collapse");
}

#[test]
fn an_unbound_actual_matches_only_an_expected_unbound() {
    let e = vec![row(&[("p", None)])];
    let a_unbound = vec![row(&[("p", None)])];
    let a_bound = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    assert!(compare(&e, &a_unbound, true, false).is_ok());
    assert!(compare(&e, &a_bound, true, false).is_err());
}

#[test]
fn the_error_names_what_was_missing() {
    let e = vec![row(&[("p", Some(iri("http://example.org/zzz")))])];
    let a = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    let err = compare(&e, &a, true, false).unwrap_err();
    assert!(err.contains("zzz"), "the message must name the missing row: {err}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test rows`
Expected: FAIL, unresolved import `sulo_testharness::rows`.

- [ ] **Step 3: Write minimal implementation**

`src/rows.rs`. Implement `parse_expected` by recognising, in order: `None` gives `Unbound`; a token starting `<` and ending `>` gives an IRI; a token starting `"` is parsed as a literal with optional `^^datatype` (the datatype itself resolved through the prefix map) or `@lang`, defaulting to `xsd:string`; a token starting `_:` is a `RowError::BlankNode`; anything else is a CURIE through `prefixes::expand`.

Implement `compare` as: when `ordered`, compare element-wise and fail on the first positional mismatch; otherwise compare as multisets by removing each expected row from a mutable copy of the actual rows. When `exact`, additionally require the leftover actual rows to be empty. Build the error string naming the first missing expected row and, for `exact`, the count and first example of the leftovers.

Add `pub mod rows;` to `src/lib.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test rows`
Expected: PASS, 14 tests.

- [ ] **Step 5: Stub check, and report the breakdown**

Stub `compare` to always return `Ok(())`. Every test whose name says "rejects", "is_err", or "must respect" must fail: that is at least 6. Then stub `parse_expected` to always return `Expected::Bound(iri("http://x"))`; at least 5 must fail. Zero panics. If any test survives both, close the gap and say which.

- [ ] **Step 6: Commit**

```bash
git add src/rows.rs src/lib.rs tests/rows.rs
git commit -m "feat(rows): term-level comparison of expected and actual CQ rows"
```

---

### Task 3: Materialise the closure into an oxigraph store

**Files:**
- Modify: `Cargo.toml`, `src/lib.rs`
- Create: `src/materialize.rs`, `tests/materialize.rs`

**Interfaces:**
- Consumes: `SetOntology<RcStr>`, `oracle::REASONER_DEADLINE`.
- Produces: `pub fn materialize(onto: &SetOntology<RcStr>, deadline: Duration) -> Result<Store, MaterializeError>`, `pub enum MaterializeError { Reasoner(String), Store(String), Timeout }`.

- [ ] **Step 1: Write the failing test**

```rust
// tests/materialize.rs
use std::path::Path;
use std::time::Duration;

use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use sulo_testharness::load::{load_file, merge};
use sulo_testharness::materialize::materialize;

const SULO: &str = "../sulo/sulo.ttl";

fn ask(store: &oxigraph::store::Store, q: &str) -> bool {
    let r = SparqlEvaluator::new().parse_query(q).unwrap()
        .on_store(store).execute().unwrap();
    match r {
        QueryResults::Boolean(b) => b,
        _ => panic!("expected an ASK"),
    }
}

fn parts_store() -> oxigraph::store::Store {
    let mut onto = load_file(Path::new(SULO)).unwrap().ontology;
    let data = load_file(Path::new("tests/fixtures/parts.ttl")).unwrap().ontology;
    merge(&mut onto, data);
    materialize(&onto, Duration::from_secs(30)).unwrap()
}

#[test]
fn asserted_triples_are_present() {
    let s = parts_store();
    assert!(ask(&s, "ASK { <http://example.org/a> <https://w3id.org/sulo/isPartOf> <http://example.org/b> }"));
}

#[test]
fn inferred_transitive_closure_is_present() {
    let s = parts_store();
    assert!(ask(&s, "ASK { <http://example.org/a> <https://w3id.org/sulo/isPartOf> <http://example.org/c> }"),
        "isPartOf is transitive, so a isPartOf c must be materialised");
}

#[test]
fn inferred_subproperty_propagation_is_present() {
    let s = parts_store();
    assert!(ask(&s, "ASK { <http://example.org/a> <https://w3id.org/sulo/isIn> <http://example.org/c> }"),
        "isPartOf is a subproperty of isIn");
}

#[test]
fn reflexive_self_loops_are_injected() {
    // inferred_object_property_values omits these, so without explicit
    // injection a CQ pattern ?x sulo:isPartOf ?x silently returns
    // nothing despite isPartOf being reflexive. Spec section 8 step 6.
    let s = parts_store();
    assert!(ask(&s, "ASK { <http://example.org/d> <https://w3id.org/sulo/isPartOf> <http://example.org/d> }"),
        "reflexive self-loop must be injected for every named individual");
}

#[test]
fn inferred_class_assertions_use_the_full_closure() {
    // ex:d is asserted SpatialObject; Object is an ancestor.
    let s = parts_store();
    assert!(ask(&s, "ASK { <http://example.org/d> a <https://w3id.org/sulo/Object> }"),
        "class assertions must be the full closure, not most-specific types");
}

#[test]
fn a_non_entailment_is_absent() {
    // The store must not contain everything: a false statement must
    // be absent, or every CQ would pass.
    let s = parts_store();
    assert!(!ask(&s, "ASK { <http://example.org/d> a <https://w3id.org/sulo/Process> }"),
        "ex:d is a SpatialObject, which is disjoint from Process");
}

#[test]
fn a_zero_deadline_times_out_rather_than_hanging() {
    let mut onto = load_file(Path::new(SULO)).unwrap().ontology;
    let data = load_file(Path::new("tests/fixtures/parts.ttl")).unwrap().ontology;
    merge(&mut onto, data);
    let r = materialize(&onto, Duration::from_millis(0));
    assert!(r.is_err(), "a zero deadline must not silently succeed");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test materialize`
Expected: FAIL, unresolved import.

- [ ] **Step 3: Write minimal implementation**

Add to `Cargo.toml`:

```toml
oxigraph = { version = "0.5.9", default-features = false }
```

Verified: this resolves to the SAME `oxrdf 0.3.3` and `oxrdfio 0.2.5` the crate already uses, so there is exactly one `Term` type in the tree. Confirm with `cargo tree | grep oxrdf` after adding; two versions means stop and fix.

`src/materialize.rs`. Build the store per spec section 8 step 6, in this order:

1. Every asserted triple. Serialise the `SetOntology` to Turtle via horned-owl's writer and load it into the store, or walk the components; either is acceptable provided the test for asserted triples passes.
2. Every inferred class assertion: for each named class in the ontology, call the bounded instance query and add `individual rdf:type class`.
3. Every inferred object and data property assertion, from `inferred_object_property_values` and `inferred_data_property_values`, both of which take a `pair_deadline`.
4. The reflexive self-loops: for every named individual, add `x sulo:isPartOf x` and `x sulo:hasPart x`.

Named individuals only. Every reasoner call takes the deadline; on expiry return `MaterializeError::Timeout`.

Add `pub mod materialize;` to `src/lib.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test materialize`
Expected: PASS, 7 tests. Report the wall-clock: this is the most expensive operation in the harness and the suite will run it once per case.

- [ ] **Step 5: Stub check**

Stub `materialize` to return an empty `Store`. Six of seven tests must fail (`a_non_entailment_is_absent` will pass against an empty store, which is correct and is why the other six exist). Report the count.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/materialize.rs src/lib.rs tests/materialize.rs
git commit -m "feat(materialize): oxigraph store from the asserted plus inferred closure"
```

---

### Task 4: Run one competency question

**Files:**
- Create: `src/cq.rs`, `tests/cq.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `CqSpec`, `rows::compare`, `materialize`, `CheckOutcome`, `Verdict`.
- Produces: `pub fn check_cq(store: &Store, spec: &CqSpec, base_dir: &Path, pm: &PrefixMapping) -> CheckOutcome`.

- [ ] **Step 1: Write the failing test**

```rust
// tests/cq.rs
// Uses the same parts fixture as tests/materialize.rs.
#[test]
fn a_matching_cq_passes() { /* expect_rows equal to the real answer -> Verdict::Pass */ }

#[test]
fn a_mismatched_cq_fails_and_names_the_difference() {
    // Verdict::Fail whose message contains the missing IRI.
}

#[test]
fn an_unparseable_query_is_indeterminate_not_a_fail() {
    // A broken .rq is a configuration error on the author's part, not
    // an ontology regression. Verdict::Indeterminate.
}

#[test]
fn a_missing_query_file_is_indeterminate_and_names_the_path() {}

#[test]
fn an_ask_query_is_rejected_with_a_clear_message() {
    // expect_rows only makes sense for SELECT. An ASK must be a
    // configuration error rather than silently comparing zero rows.
}
```

Write these out fully, following the shape of `tests/manchester.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test cq`

- [ ] **Step 3: Write minimal implementation**

`src/cq.rs`: resolve `spec.query` against `base_dir`, read it, parse with `SparqlEvaluator::new().parse_query(..)`, execute `.on_store(store).execute()`, and require `QueryResults::Solutions`. Convert each solution into a `BTreeMap<String, Option<Term>>` over the query's variables. Parse `spec.expect_rows` through `rows::parse_expected` with the case's prefix map. Call `rows::compare`. Map `Ok` to `Verdict::Pass` and `Err(msg)` to `Verdict::Fail(msg)`. Any file, parse, or result-shape problem is `Verdict::Indeterminate(IndeterminateReason::OracleError(..))`.

Note the asymmetry deliberately: a CQ is a positive assertion about what the ontology answers, so a match is a trustworthy `Pass`. There is no `UnrefutedPass` here, because the store is a materialised closure rather than a proof search.

- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Stub check.** Stub `check_cq` to return `Pass`; the mismatch and the three error-path tests must fail.
- [ ] **Step 6: Commit**

---

### Task 5: Wire CQs into `run_case`

**Files:**
- Modify: `src/suite.rs`
- Test: `tests/suite.rs`

- [ ] **Step 1: Write the failing test**

A case whose `cq` matches yields an overall `Pass`; a case whose `cq` mismatches yields `Fail`; a case with both `entails` and `cq` runs both and reports both checks. Assert on `checks.len()` as the existing gate tests do, so a silently skipped CQ is caught.

Also: a case whose gate stops it must NOT run its CQs. Assert `checks.len()` proves they never ran.

- [ ] **Step 2 to 6:** as usual.

Implementation notes: materialise ONCE per case, after the gate passes and before the CQ loop, not once per CQ. Pass `Duration::from_millis(case.timeout_ms)`. A `MaterializeError` makes every CQ in that case `Indeterminate` with the reason, rather than failing them.

---

### Task 6: Suite group `taxonomy` (about 22 cases)

**Files:** create `suites/sulo/taxonomy/*.yaml` and any data under `suites/sulo/taxonomy/data/`.

Spec section 9 is the inventory. Every case below is a separate YAML file named after its id.

- `all-classes-satisfiable`: one case, `unsatisfiable: []` is wrong here; instead assert each of the 17 named classes is satisfiable via `satisfiable_expr`, one entry per class.
- `asserted-subsumptions`: all 15 named `rdfs:subClassOf` axioms under `entails:`.
- `deep-chain`: `sulo:StartTime rdfs:subClassOf sulo:Object` under `entails:`.
- `non-subsumptions`: `Process` not under `Object`, `Role` not under `Quality`, `Unit` not under `Time`, `SpatialObject` not under `Feature`, all under `not_entails:`.
- 14 counter-example cases, one per disjoint pair, each `expect_inconsistent: true` with a two-line data file typing one skolemised individual as both: the 6 `Feature` sibling pairs, the 3 `Time` sibling pairs, and `Object`/`Process`, `Feature`/`SpatialObject`, `Time`/`Unit`, `Collection`/`Quantity`, `EndTime`/`StartTime`.
- `covering-feature`, `covering-time`: `entails_manchester` subsumptions, as in `suites/proof/covering-feature.yaml`.
- `non-covering-object`, `non-covering-informationobject`: the same shape under `not_entails_manchester`.

Worked example, `suites/sulo/taxonomy/disjoint-object-process.yaml`:

```yaml
id: disjoint-object-process
description: Object and Process are disjoint, so nothing can be both.
data: data/object-and-process.ttl
prefixes:
  ex: http://example.org/
expect_inconsistent: true
tags: [taxonomy, disjointness]
```

`suites/sulo/taxonomy/data/object-and-process.ttl`:

```turtle
@prefix sulo: <https://w3id.org/sulo/> .
@prefix ex:   <http://example.org/> .

ex:both a sulo:Object, sulo:Process .
```

- [ ] Write every case listed above.
- [ ] Run each through the harness and confirm the expected verdict.
- [ ] **Verification that matters:** for the 14 counter-example cases, temporarily remove the relevant disjointness axiom from a scratch copy of `sulo.ttl` and confirm the case flips from Pass to Fail. A counter-example case that passes on a mutant is asserting nothing. Report any that do not flip.
- [ ] Commit.

---

### Task 7: Suite group `properties` (about 10 cases)

Per spec section 9:

- `subproperty-axioms`: all four non-trivial ones under `entails:`.
- `inverse-pairs`: all 9 pairs, each asserted both directions over a two-individual fixture.
- `transitivity`: three-step chains for `isPartOf`, `hasPart`, `isIn`, `contains`.
- `reflexivity`: `x isPartOf x` and `x hasPart x` for an individual with no asserted parthood.
- `functional-hasvalue`: `expect_inconsistent: true`, two distinct typed literals on one `Quantity`.
- `non-transitivity-isdirectpartof`: `not_entails:` over a two-step `isDirectPartOf` chain. This is the axiom a well-meaning edit would "fix".

- [ ] Write, run, verify each flips against a targeted mutation, commit.

---

### Task 8: Suite group `restrictions` (about 13 cases)

The 16 restriction axioms from spec section 9, minus the 3 documented as semantically inert (`Collection ⊑ ∀hasItem.owl:Thing`, `InformationObject ⊑ ∀hasValue.rdfs:Literal`, `Object ⊑ ¬∃hasPart.Process`). Write those three as comments in a README in the group directory explaining why they have no case, so their absence reads as a decision.

- 5 `hasPart` propagation cases, one per `C ⊑ ∀hasPart.C`.
- 6 object `someValuesFrom` cases as `entails_manchester`.
- `duration-nonnegative`: `expect_inconsistent: true` with a negative decimal.
- `timeinstant-datarange`: **mark this `tags: [oracle-hermit]` and do NOT expect it to pass.** rustdl cannot enforce data-range `allValuesFrom`; a `TimeInstant` with `"hello"^^xsd:string` is reported consistent. Write the case, tag it, and exclude it from the default run until the HermiT differential lands. Document that in the group README.

- [ ] Write, run, verify, commit.

---

### Task 9: Suite groups `domains-ranges` and `patterns` (about 15 cases)

**domains-ranges:** domain and range for all 18 object properties plus `hasValue`, each as an entailed class assertion driven from a bare property assertion between untyped skolemised individuals, plus one range-violation case going inconsistent.

**patterns/pro:** Figure 7's data faithfully adapted (see spec 9.1 for the required repairs: the paper's listing is not valid Turtle and its role individuals are never typed `sulo:Role`). Cases: the role chain fires; the chain does not run backwards; the pattern-membership `instance_of_expr`; and a competency question `who-participated.rq` returning both role holders.

**patterns/solid:** Figure 4's data with skolemised IRIs. Cases: the entailed typing chain; the unit forced to be a `Feature` but NOT a `Unit`; a second `hasValue` going inconsistent; and a competency question recovering value, quality, and unit together, which is the executable form of the paper's "predictable location for data values" claim.

Worked example, `suites/sulo/patterns/pro/queries/who-participated.rq`:

```sparql
PREFIX sulo: <https://w3id.org/sulo/>
SELECT ?p WHERE {
  <http://example.org/encounter> sulo:hasParticipant ?p .
  ?p a <http://purl.obolibrary.org/obo/NCBITaxon_9606> .
}
ORDER BY ?p
```

- [ ] Write, run, verify, commit.

---

### Task 10: Extend the mutation suite to the new groups

The mutation suite is what proves the new cases are load-bearing. Four mutants currently exist and the staleness test re-derives each from a live read of `sulo.ttl`.

- [ ] Add one mutant per new suite group whose axiom is not already covered: at minimum a removed `hasPart` propagation axiom, a removed `someValuesFrom`, a removed domain, and a removed range.
- [ ] Add each to `mutants/regenerate.sh` and to `mutants_are_not_stale_against_current_sulo`, or the new mutants become frozen copies with no guard.
- [ ] Extend `tests/mutation.rs` with one `assert_caught` per new mutant, naming the case that must catch it.
- [ ] **Report any mutant no case catches.** That is a genuine coverage hole and must be reported, not papered over by weakening the assertion. If a mutant turns out semantically inert (as two did in the engine plan, because SULO's axioms are mutually redundant given the inverse pairs), say so and fix the mutant rather than the test.
- [ ] Commit.

---

## What this plan does not cover

- **The HermiT differential** (spec 5.3). The `oracle-hermit` tagged case from Task 8 stays excluded until it lands.
- **Release binaries, the composite Action, and the consumer workflow PR** to `AIDAVA-DEV/sulo`. Gated on the repository visibility decision.
- **The three missing golden-closure components** (inferred class assertions, property assertions, disjointness). Needs a probe ABox; would take mutant sensitivity from 1 of 4 to 4 of 4.
- **The unbounded `is_consistent` gate.** Documented in `run_case` and the spec; no deadline-bearing variant exists at v0.4.22.

## Self-review notes

- Spec 7.3 maps to Task 2, spec 8 step 6 to Task 3, spec 9's inventory to Tasks 6 to 9.
- Task 2 is deliberately its own task despite being small: a lenient comparator makes every CQ in Tasks 6 to 9 a test that cannot fail, which is the defect shape this project hit eleven times.
- Every suite task ends with a mutation check rather than "the case passes", for the same reason.
- The `oracle-hermit` tag in Task 8 is the honest handling of a case the pinned reasoner provably cannot decide: write it, tag it, exclude it, and say why, rather than omitting it and losing the knowledge.
- Types used across tasks (`CqSpec`, `Expected`, `MaterializeError`, `check_cq`) are each defined once, in the task that introduces them.
