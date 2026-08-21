# sulo-testharness: design

**Date:** 2026-08-21
**Status:** approved, pending implementation plan

## 1. Problem

The [SULO](https://github.com/AIDAVA-DEV/sulo) repository has five CI workflows,
and only two of them test anything:

- `syntax_check.yml` runs `rapper` round-trips over `*.ttl` at the repository
  root.
- `reasoning.yml` downloads the ROBOT 1.9.7 jar and runs `robot reason
  --reasoner hermit` for a whole-ontology consistency check.

The other three generate documentation, a FOOPS badge, and a version badge.

There is no regression suite. Nothing asserts that a given subsumption still
holds, that the PRO role chain still fires, that a disjointness axiom still
excludes anything, or that `isDirectPartOf` is still non-transitive. An edit to
`sulo.ttl` fails CI only if it makes the entire ontology inconsistent or
unparseable. Every other regression ships silently.

## 2. What is being protected

`sulo.ttl` declares 17 classes, 18 object properties (9 inverse pairs), and 1
data property. The logic worth guarding is concrete:

**Disjoint unions and disjointness.** Exactly seven axioms, extracted from
`sulo.ttl` rather than from the paper:

- `Feature owl:disjointUnionOf (Capability InformationObject Quality Role)`
- `Time owl:disjointUnionOf (Duration TimeInstant TimeInterval)`
- `Object owl:disjointWith Process`
- `Feature owl:disjointWith SpatialObject`
- `Time owl:disjointWith Unit`
- `Collection owl:disjointWith Quantity`
- `EndTime owl:disjointWith StartTime`

Two absences are as important as the presences, and the suite pins both as
intentional rather than leaving them ambiguous:

- **`Object` has no covering axiom.** `SpatialObject` and `Feature` are disjoint
  subclasses of `Object`, but there is no `Object owl:disjointUnionOf`, so an
  `Object` that is neither remains consistent.
- **`InformationObject` has no covering axiom.** `Collection` and `Quantity` are
  disjoint, but they do not exhaust `InformationObject`.

**Named subsumptions.** Fifteen asserted named `rdfs:subClassOf` axioms.
`Process` and `Object` are the only classes directly under `owl:Thing`.

**Property characteristics**

- `isPartOf` and `hasPart`: reflexive and transitive
- `isIn` and `contains`: transitive
- `hasValue`: functional
- `isDirectPartOf` and `hasDirectPart`: deliberately non-transitive
  subproperties of `isPartOf` and `hasPart`, so that OWL 2 cardinality
  restrictions remain legal over them
- 9 `owl:inverseOf` pairs

**The PRO role chain**

`hasParticipant o inverse(hasFeature) -> hasParticipant`, which is the paper's
`hasParticipant o isFeatureOf -> hasParticipant`.

**The SOLID pattern**

`hasValue` plus `refersTo` plus `hasPart` on a `Quantity`.

**An observation, recorded but not tested.** The property chain makes
`hasParticipant` non-simple in OWL 2, and non-simplicity propagates to its
inverse `isParticipantIn`, so cardinality restrictions over either are already
illegal in OWL 2 DL. By contrast `isDirectPartOf` is simple: being a subproperty
of the transitive `isPartOf` does not make it non-simple, which is precisely why
its own non-transitivity is worth a regression test. Profile conformance is out
of scope for this harness, so the `hasParticipant` consequence is documented
here rather than asserted anywhere.

## 3. Scope

In scope, as decided during design:

- **Logical regressions.** Expected entailments and non-entailments, class
  satisfiability, and consistency, including consistency counter-examples that
  prove a disjointness or characteristic axiom still bites.
- **Competency questions.** SPARQL queries over small example datasets that must
  return expected answers, authored fresh from the motivating examples in the
  FOUST 2025 paper (`docs/papers/FOUST2025/SULO_FOUST2025.pdf`) and the SULO
  postcard.

Out of scope:

- Structural and metadata quality gates (label and definition completeness, IRI
  conventions, OWL 2 DL profile conformance). FOOPS already scores these.
- Artifact and mapping integrity (agreement between `versions/latest/*`
  serialisations, `w3id` content negotiation, parseability of `extensions/`,
  `mapping/`, and archived `versions/`).
- Harvesting test content from downstream consumers (`sulo-mimickg`,
  `pathway-extract`). The suite is self-contained.

Noted but deliberately excluded: `syntax_check.yml` globs only `*.ttl` at the
repository root, so `extensions/sphn25-1.ttl`, all four files in `mapping/`, and
the 13 files in `versions/` are never syntax checked. This is a one-line glob
fix in an existing workflow, unrelated to logical regressions.

## 4. Decisions

| Decision | Choice | Rationale |
| --- | --- | --- |
| Location | Standalone `sulo-testharness` repository, consumed by `AIDAVA-DEV/sulo` CI as a pinned dependency | Mirrors the `horned-owl` / `horned-roundtrip` split; reusable against any SULO-based ontology; keeps the SULO repository lean |
| Implementation | Rust CLI plus a composite GitHub Action | No JVM, no interpreter, hermetic; dogfoods `rustdl` on a real ontology; consumer CI needs no toolchain |
| Reasoning | `owl-dl-reasoner` (rustdl) as an in-process library | Sound SROIQ(D); covers every construct SULO uses |
| Parsing | `horned-owl` 2.0.0 | Its RDF reader is parameterised over `oxrdfio::RdfFormat`, so Turtle is read directly with no conversion step |
| SPARQL | `oxigraph` in-memory store | Competency questions run over asserted plus materialised triples |
| Test declaration | YAML manifest plus sidecar `.ttl` and `.rq` files | Greppable, diff-friendly, adaptable by a non-programmer; no RDF ceremony for scaffolding |
| Verdict architecture | Typed claims dispatched to reasoner queries, separate CQ path | Keeps the entailment oracle a real reasoner rather than a triple dump |
| Suite home | `suites/sulo/` inside the harness repository | The mutation tests need the suite and the engine together; the Action takes a `--tests` path, so relocating later is configuration, not a rewrite |

Deferred: the git remote. Development happens at `~/code/sulo-testharness` with
no remote set. Whether this lands under `AIDAVA-DEV` or `MaastrichtU-IDS` is
decided at phase 6.

## 5. Verdicts

rustdl is **sound** but not provably complete beyond the EL and Horn fragment,
and reports an `incomplete` flag per query. That flag determines which verdicts
can be trusted:

| | reasoner: entailed | reasoner: not entailed |
| --- | --- | --- |
| test expects entailed | PASS, trustworthy by soundness | suspect: a real regression, or an incompleteness artifact |
| test expects not entailed | FAIL, trustworthy by soundness | suspect: a genuine regression could hide here |

Two verdicts cannot express this, so the harness has three: **Pass**, **Fail**,
and **Indeterminate**. Indeterminate carries its reason: the reasoner set
`incomplete`, the query timed out, or a dropped axiom could affect the result.
Collapsing Indeterminate into Pass or Fail is how a test harness starts lying.

SULO is 17 classes and sits comfortably inside the tractable fragment, so
Indeterminate should be empty in practice. That is exactly why it must be loud
when it is not: Indeterminate is red by default (exit 3), with
`--allow-indeterminate` to downgrade it to a warning.

Exit codes:

- `0` all checks pass
- `1` any Fail
- `2` harness or configuration error (bad YAML, missing file, parse failure)
- `3` any Indeterminate, unless `--allow-indeterminate`

Check verdicts aggregate worst-first within a case: Fail beats Indeterminate
beats Pass.

## 6. Architecture

A single Rust crate, library plus a thin CLI. No workspace.

```
sulo-testharness/
  Cargo.toml
  src/
    lib.rs
    manifest.rs   # YAML case -> typed Case struct (serde_yaml), schema validation
    load.rs       # horned-owl ingest: ontology + data files -> one SetOntology
    claim.rs      # entails / not_entails Turtle fragment -> typed Claims
    oracle.rs     # Claims -> owl-dl-reasoner queries -> Verdict
    cq.rs         # materialise -> oxigraph store -> SPARQL -> row comparison
    suite.rs      # discovery, filtering, per-case orchestration
    report.rs     # pretty stdout, --json, --junit
    main.rs
  suites/sulo/    # the reference SULO suite
  mutants/        # deliberately broken sulo.ttl variants, for self-testing
  action.yml      # composite GitHub Action for consumer CI
```

Each module has one job and a narrow interface: `manifest.rs` never touches a
reasoner, `oracle.rs` never reads a file, `report.rs` never decides a verdict.

### 6.1 claim.rs

The load-bearing module. It turns an author's convenient Turtle fragment into
something a reasoner can actually be asked. It parses the fragment with
`oxrdfio` and classifies each statement:

| Fragment shape | Claim | rustdl query |
| --- | --- | --- |
| `:C rdfs:subClassOf :D`, both named classes | Subsumption | `is_subclass` |
| `:C owl:equivalentClass :D` | Equivalence | `is_subclass` both directions |
| `:C rdfs:subClassOf owl:Nothing` | Unsatisfiable | satisfiability |
| `:x a :C` | ClassAssertion | `instances` (see note) |
| `:x :p :y`, `:p` an object property | PropertyAssertion | `property-values` |
| `:x :p "lit"`, `:p` a data property | DataPropertyAssertion | `property-values` |

A statement matching no row is a configuration error (exit 2), never a silent
skip.

**Note on ClassAssertion.** It must be backed by `instances`, not by `realize`.
Verified during design: `realize` reports only the most specific type, so for the
SOLID example it returns `alice_temp_1 a Quantity` and nothing else, whereas
`instances` returns the full closure and correctly lists `alice_temp_1` under
`Quantity`, `InformationObject`, `Feature`, and `Object`. Backing the claim with
`realize` would make every non-most-specific class assertion fail spuriously.

Two manifest fields escape what Turtle can express, and both are needed for
SULO specifically:

- **`entails_manchester:`** because the PRO pattern's actual content is
  `Process and hasParticipant some (Role and isFeatureOf some Object)`, and
  there is no honest way to write that as a ground triple. rustdl exposes
  anonymous Manchester class-expression entailment and satisfiability.
- **`expect_inconsistent: true`** because the only way to test that a
  disjointness axiom bites is to assert a counter-example and demand the
  ontology break. `Object disjointWith Process` is untested until something is
  typed as both and the harness insists that is inconsistent.

### 6.2 Dependencies

`horned-owl` 2.0.0, `owl-dl-reasoner`, `oxigraph`, `oxrdfio`, `serde`,
`serde_yaml`, `clap`. Pinned exactly, because the oracle's behaviour is the
harness's semantics.

## 7. Manifest schema

Everything past `id` and `description` is optional.

```yaml
id: pro-role-chain
description: Participation of the role holder is recovered via the PRO role chain.
ontology: sulo.ttl                    # default comes from --ontology
imports: []                           # extra ontologies to merge
data: data/pro-encounter.ttl           # string or list
expect_inconsistent: false             # true flips the consistency gate
entails: |                             # Turtle fragment
  ex:encounter sulo:hasParticipant ex:alice, ex:drsmith .
not_entails: |
  ex:alice sulo:hasParticipant ex:encounter .
entails_manchester:
  - { expr: "Process and hasParticipant some (Role and isFeatureOf some Object)",
      subclass_of: "Process" }
unsatisfiable: []                      # classes required to be unsatisfiable
cq:
  - query: queries/who-participated.rq
    expect_rows: [{ p: "ex:alice" }, { p: "ex:drsmith" }]
    exact: true                        # false = subset check
    ordered: false                     # default set comparison
tags: [pattern, pro]
timeout_ms: 30000
```

## 8. Execution pipeline

Per case:

1. **Resolve.** All paths relative to the manifest, so a case directory is
   movable.
2. **Load.** Ontology, imports, and data merged into one `SetOntology`; format
   from the file extension. Any `dropped_axioms` reported by the conversion is
   captured now rather than discovered later.
3. **Consistency gate, before anything else.** An inconsistent ontology entails
   everything, so every positive check would pass vacuously and every negative
   check would fail for a reason unrelated to what it tests.
   - `expect_inconsistent: true` and inconsistent: the case passes, and all
     remaining checks are **skipped, not passed**.
   - `expect_inconsistent: true` and consistent: **Fail**. This is the
     axiom-stopped-biting regression, and it is the entire point of these cases.
   - expecting consistent and inconsistent: **Fail** the case, skip the rest,
     and report the clashing axioms via `justify`.
4. **Positive entailments** dispatched per the claim table.
5. **Negative entailments**, the same queries with inverted expectations.
6. **Competency questions.** Materialise inferred axioms, merge asserted plus
   inferred triples into an in-memory oxigraph store, run the SPARQL, compare
   bindings as a multiset (ordered comparison when `ordered: true`).
7. **Aggregate** verdicts worst-first.

## 9. Suite inventory

Roughly 40 cases in four groups, under `suites/sulo/`.

**taxonomy**

- all 17 classes satisfiable
- all 15 asserted named subsumptions still entailed
- the deep chain closes: `StartTime` to `TimeInstant` to `Time` to `Quantity` to
  `InformationObject` to `Feature` to `Object`
- named non-subsumptions: `Process` not under `Object`, `Role` not under
  `Quality`, `Unit` not under `Time`, `SpatialObject` not under `Feature`
- one `expect_inconsistent` counter-example per disjoint pair, **14** in total:
  6 from the `Feature` disjoint union, 3 from the `Time` disjoint union, and the
  5 plain pairs `Object`/`Process`, `Feature`/`SpatialObject`, `Time`/`Unit`,
  `Collection`/`Quantity`, `EndTime`/`StartTime`
- the **covering** half of the two disjoint unions, which no disjointness test
  reaches: a `Feature` asserted to be none of `Capability`, `InformationObject`,
  `Quality`, or `Role` must be inconsistent, and likewise a `Time` that is none
  of `Duration`, `TimeInstant`, or `TimeInterval`
- the two deliberate **non**-coverings, pinned as intentional: an `Object` that
  is neither `SpatialObject` nor `Feature` must stay **consistent**, and an
  `InformationObject` that is neither `Collection` nor `Quantity` must stay
  consistent. Without these, someone "completing the pattern" by adding a third
  disjoint union would break downstream data and pass CI.

**properties**

- the two subproperty axioms: `isDirectPartOf` under `isPartOf`,
  `hasDirectPart` under `hasPart`
- all 9 inverse pairs round-trip: `atTime`/`isTimeOf`,
  `isPrecededBy`/`precedes`, `isReferredToIn`/`refersTo`, `contains`/`isIn`,
  `hasFeature`/`isFeatureOf`, `hasItem`/`isItemIn`,
  `hasParticipant`/`isParticipantIn`, `hasDirectPart`/`isDirectPartOf`,
  `hasPart`/`isPartOf`
- `isPartOf` and `isIn` transitivity closes over a three-step chain
- `isPartOf` reflexivity
- `hasValue` functionality, enforced by two distinct literals going inconsistent
- domain and range axioms as entailed class assertions: `:p hasParticipant :o`
  entails `:p a Process` and `:o a Object`
- a range violation going inconsistent
- **`isDirectPartOf` must not close transitively.** This is the axiom a
  well-meaning edit would "fix", and it is what keeps OWL 2 cardinality
  restrictions legal over the property.

**patterns/pro**

- Figure 7's data verbatim, with the role chain firing to yield
  `encounter hasParticipant alice, drsmith`
- the chain does not run backwards
- the Manchester pattern-membership expression

**patterns/solid**

- Figure 4's data, with the entailed typing `Quantity` to `InformationObject` to
  `Feature` to `Object` on the measurement
- **the unit is forced to be a `Feature`.** Verified during design: the unit
  individual, typed only as `obo:UO_0000027`, is inferred to be an
  `InformationObject` and a `Feature` purely by being `hasPart` of one, via
  `Feature owl:subClassOf (hasPart only Feature)`. This is the pattern's
  semantics doing real work and it deserves an explicit case.
- **the unit is *not* forced to be a `Unit`.** Also verified: `Unit`-hood is not
  entailed, so downstream data must type units explicitly. Asserted as a
  non-entailment so that nobody later assumes the pattern supplies it.
- a competency question recovering value, quality, and unit together, which
  makes the paper's "predictable location for data values" claim executable
- a second `hasValue` going inconsistent, which is the guarantee the pattern
  relies on

Note: Figure 4 in the paper writes the prefix as `http://w3id.org/sulo/`, but
the real namespace is `https://w3id.org/sulo/`. The suite uses the correct one,
and the competency-question tests are what catch that class of typo in
downstream data.

## 10. Self-testing by mutation

The failure mode for a test harness is not being wrong, it is being green while
testing nothing. So `mutants/` holds deliberately broken variants of `sulo.ttl`,
and the harness's own `cargo test` asserts two things: unmutated SULO passes
everything, and each mutant is caught by a **specific named case**.

| Mutant | Case that must fail |
| --- | --- |
| delete the `propertyChainAxiom` on `hasParticipant` | `patterns/pro/role-chain` |
| drop `owl:TransitiveProperty` from `isPartOf` | `properties/transitivity-ispartof` |
| **add** `owl:TransitiveProperty` to `isDirectPartOf` | `properties/non-transitivity-isdirectpartof` |
| drop `owl:FunctionalProperty` from `hasValue` | `patterns/solid/single-value` |
| delete the `Feature` `disjointUnionOf` | the 6 `Feature`-sibling counter-examples, and `taxonomy/covering-feature` |
| weaken `Feature`'s `disjointUnionOf` to a plain `disjointWith` | `taxonomy/covering-feature` alone, since the sibling pairs still clash |
| **add** an `Object owl:disjointUnionOf (SpatialObject Feature)` | `taxonomy/non-covering-object` |
| delete one `owl:inverseOf` | `properties/inverses` |
| delete `hasParticipant`'s range | `properties/domains-ranges` |

A mutant that no case catches is a coverage hole, and is reported as one. This
is the only evidence that the 40 cases are load-bearing rather than decorative,
and it doubles as the harness's own regression suite.

## 11. CI integration

A release workflow builds a static `linux-x86_64` binary, plus
`macos-aarch64` for local runs, and attaches both to a GitHub release.
`action.yml` is a composite action that downloads the binary for a pinned tag
and runs it. Consumer CI in the SULO repository becomes:

```yaml
- uses: AIDAVA-DEV/sulo-testharness@v0.1.0
  with: { ontology: sulo.ttl }
```

**On the existing reasoning job.** `reasoning.yml` downloads the ROBOT 1.9.7 jar
to run HermiT for a consistency check, which the harness subsumes. Rather than
delete it immediately, keep it for one release cycle: it becomes a free
differential oracle on exactly the property most worth cross-checking, and if
rustdl and HermiT ever disagree about SULO's consistency, that disagreement is
the most valuable signal either tool could produce. Retire it once they have
agreed across a few releases.

## 12. Error handling

- Malformed YAML, unknown manifest keys, missing referenced files, and
  unparseable `.ttl` or `.rq` files are configuration errors: exit 2, reported
  with the manifest path and the offending key or line. They are never silently
  skipped and never reported as test failures.
- A statement in an `entails` or `not_entails` fragment that matches no row in
  the claim table is a configuration error, not a skipped check.
- A non-empty `dropped_axioms` from the rustdl conversion raises a suite-level
  warning naming the dropped axiom kinds, and any positive-entailment Fail in
  that run is downgraded to Indeterminate, because a dropped axiom is a sound
  under-approximation and the failure may be an artifact of it.
- Query timeouts produce Indeterminate, never Fail. The per-case budget comes
  from `timeout_ms`, defaulting to 30000.
- An inconsistent ontology where consistency was expected reports the clashing
  axiom set via rustdl's `justify`, so the failure is actionable rather than
  just red.

## 13. Design validation

Every load-bearing assumption was exercised against the real artefacts during
design, using the already-built `rustdl` and `horned-convert` binaries. Results,
against `sulo.ttl` at version 0.2.14:

| Assumption | Result |
| --- | --- |
| `horned-owl` 2.0.0 parses `sulo.ttl` | passes, 257-line OFN round-trip, no dropped axioms reported |
| `sulo.ttl` is consistent under rustdl | `consistent` |
| the 6-level deep chain closes | `StartTime` under `Object`: `yes` |
| a negative subsumption holds | `Process` under `Object`: `no` |
| the PRO role chain fires on Figure 7's data | both `encounter hasParticipant alice` and `encounter hasParticipant drsmith` inferred |
| the SOLID example is consistent | `consistent` |
| `hasValue` functionality bites | a second literal on the same `Quantity`: `inconsistent` |

**One implementation constraint discovered.** The `rustdl` CLI accepts OFN only,
with no format flag, so it cannot read `sulo.ttl` directly. This does not affect
the design, because the harness links `owl-dl-reasoner` as a library and does its
own parsing through `horned-owl` with `RdfFormat::Turtle`. It does mean phase 1
must not shell out to the `rustdl` binary as a shortcut, since that would silently
reintroduce an OFN conversion step and a subprocess boundary.

## 14. Build order

Six phases, each independently verifiable.

1. Crate skeleton: manifest parsing, loading, consistency gate, reporting. Runs
   end to end on a two-case suite.
2. `claim.rs` and `oracle.rs`: positive and negative entailments, the
   three-verdict lattice.
3. Competency-question path: materialise, oxigraph, binding comparison.
4. The SULO suite content, all 40 cases.
5. Mutants and self-tests.
6. Release binaries, `action.yml`, and the consumer workflow pull request to
   `AIDAVA-DEV/sulo`.
