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

**Disjoint unions and disjointness.** Nine axioms, extracted from `sulo.ttl`
rather than from the paper:

- `Feature owl:disjointUnionOf (Capability InformationObject Quality Role)`
- `Time owl:disjointUnionOf (Duration TimeInstant TimeInterval)`
- `Object owl:disjointWith Process`
- `Feature owl:disjointWith SpatialObject`
- `Time owl:disjointWith Unit`
- `Collection owl:disjointWith Quantity`
- `EndTime owl:disjointWith StartTime`
- `[] a owl:AllDisjointClasses ; owl:members (Capability InformationObject Quality Role)`
- `[] a owl:AllDisjointClasses ; owl:members (Duration TimeInstant TimeInterval)`

The two `AllDisjointClasses` axioms sit at the end of `sulo.ttl` (lines 374 to
378) and are logically redundant, since each restates the pairwise disjointness
already implied by the corresponding `disjointUnionOf`. They matter anyway for
two reasons. First, **horned-owl 2.0.0 drops them silently**: its RDF reader has
no `AllDisjointClasses` handling, the orphaned triples land in the
`IncompleteParse` return, and `horned-convert` discards that without a warning.
So a naive harness reasons over a strictly weaker ontology than the one that
ships, and never says so. `load.rs` must therefore check
`IncompleteParse::is_complete()` and treat leftovers exactly like rustdl's
`dropped_axioms`. Second, their redundancy changes what a mutation proves: with
them present, deleting a `disjointUnionOf` leaves pairwise disjointness intact,
so only the covering case should react. See section 10.

Two absences are as important as the presences, and the suite pins both as
intentional rather than leaving them ambiguous:

- **`Object` has no covering axiom.** `SpatialObject` and `Feature` are disjoint
  subclasses of `Object`, but there is no `Object owl:disjointUnionOf`, so an
  `Object` that is neither remains consistent.
- **`InformationObject` has no covering axiom.** `Collection` and `Quantity` are
  disjoint, but they do not exhaust `InformationObject`.

**Named subsumptions.** Fifteen asserted named `rdfs:subClassOf` axioms.
`Process` and `Object` are the only classes directly under `owl:Thing`.

**Subproperty axioms.** Four non-trivial ones, not two, plus two
`owl:topObjectProperty` declarations that carry no content:

- `isDirectPartOf rdfs:subPropertyOf isPartOf` (line 156)
- `hasDirectPart rdfs:subPropertyOf hasPart` (line 251)
- `isPartOf rdfs:subPropertyOf isIn` (line 272)
- `hasPart rdfs:subPropertyOf contains` (line 363)

The last two are easy to overlook and are doing real work. Verified on a parts
chain: asserting `a isPartOf b` and `b isPartOf c` yields `a isIn b`, `a isIn c`,
and `b isIn c`, from these axioms plus `isIn` transitivity. Any competency
question phrased over `isIn` or `contains` depends on them, so they get their own
cases and their own mutants.

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
repository root, so `extensions/sphn25-1.ttl`, the three `.ttl` files in `mapping/`, and
the 13 files in `versions/` are never syntax checked. This is a one-line glob
fix in an existing workflow, unrelated to logical regressions.

## 4. Decisions

| Decision | Choice | Rationale |
| --- | --- | --- |
| Location | Standalone `sulo-testharness` repository, consumed by `AIDAVA-DEV/sulo` CI as a pinned dependency | Mirrors the `horned-owl` / `horned-roundtrip` split; reusable against any SULO-based ontology; keeps the SULO repository lean |
| Implementation | Rust CLI plus a composite GitHub Action | No JVM, no interpreter, hermetic; dogfoods `rustdl` on a real ontology; consumer CI needs no toolchain |
| Reasoning | `owl-dl-reasoner` (rustdl) **pinned to tag `v0.4.22`** as an in-process library | Sound SROIQ(D). Covers what SULO uses, including the covering half of `DisjointUnion` (corrected, see 6.1). One measured exception remains: data-range `allValuesFrom`, handled in section 9. The version pin is load-bearing, not incidental |
| Parsing | `horned-owl` at the **same git rev rustdl pins**, `b188edaf7c92600918f0524962d928097ecd6b4d` (declares version 1.4.0) | Its RDF reader is parameterised over `oxrdfio::RdfFormat`, so Turtle is read directly with no conversion step. The rev matters: rustdl pins horned-owl by git rev, so depending on the published 2.0.0 instead would give two distinct crate instances whose `SetOntology` types cannot interoperate. Verified that this rev already contains the format-parameterisation commit `e6e3c49` |
| SPARQL | `oxigraph` in-memory store | Competency questions run over asserted plus materialised triples |
| Test declaration | YAML manifest plus sidecar `.ttl` and `.rq` files | Greppable, diff-friendly, adaptable by a non-programmer; no RDF ceremony for scaffolding |
| Verdict architecture | Typed claims dispatched to reasoner queries, separate CQ path | Keeps the entailment oracle a real reasoner rather than a triple dump |
| Untrusted direction | Golden closure diff as the default gate, plus a HermiT differential in CI | A sound-but-incomplete reasoner cannot certify a non-entailment. The golden diff is incompleteness-invariant and JVM-free; HermiT is complete and settles what the diff cannot. See section 5 |
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

The table is correct, but an earlier draft of this spec drew the wrong conclusion
from it. It proposed routing every `incomplete` query to an Indeterminate
verdict, on the premise that "SULO sits comfortably inside the tractable
fragment, so Indeterminate should be empty in practice."

**That premise is false, and it was measured.** SULO uses `disjointUnionOf`,
`complementOf`, and `allValuesFrom` over unions, all outside EL and Horn.
`incomplete: true` comes back on essentially every non-EL query, including both
covering checks in section 9 and every `subclass-expr` against plain `sulo.ttl`.
Under the discarded rule, every negative test lands Indeterminate, the suite
exits 3 on every run, and the first thing any maintainer does is set
`--allow-indeterminate` permanently, switching off exactly the checks the flag
existed to protect. `rustdl consistent` also exposes no `incomplete` field at
all, so the consistency gate could not have applied the rule even in principle.

So `incomplete` is **not** a verdict input. It is a blanket per-query property of
the path the reasoner took, not a statement about this answer. The untrusted
direction is handled by two mechanisms instead, and the harness's honesty rests
on them rather than on a flag.

### 5.1 Verdicts for hand-written assertions

- Positive expectation, entailed: **Pass**. Trustworthy by soundness.
- Negative expectation, entailed: **Fail**. Trustworthy by soundness.
- Positive expectation, not entailed: **Fail**. Reported with the caveat that
  incompleteness is a possible cause; the CI differential (5.3) resolves which.
- Negative expectation, not entailed: **Pass (unrefuted)**. Counted and reported
  separately from verified Passes, because absence of a proof is not proof of
  absence. It does not fail the build on its own.

**Indeterminate** is reserved for genuine non-answers: a timeout, or a dropped
axiom or incomplete parse that could bear on this specific query. It is no
longer triggered by the `incomplete` flag, so it should be rare and stays red.

**`satisfiable_expr` sits on the unprovable side of this table, and reports as
such.** The probe behind it answers "is this expression UNsatisfiable?", and
UNSAT is its only trustworthy answer: SAT is what a missed clash also produces.
So a `satisfiable_expr` that is satisfiable as expected reports **Pass
(unrefuted)**, not Pass, and a `satisfiable_expr` that turns out unsatisfiable
reports a trustworthy **Fail**. An earlier implementation had these the other
way round, which made a spuriously-satisfiable expression a verified Pass and
made a genuine unsatisfiability regression carry the "no proof was found" marker
that the axiom-loss downgrade then demoted to Indeterminate.

**An undeclared term in a class expression is a configuration error, not a
verdict.** Manchester parsing never consults the ontology, and rustdl's
conversion registers any IRI that appears in an axiom whether declared or not,
so `sulo:Featuer` would silently become a fresh unconstrained class: trivially
satisfiable, trivially not subsumed, hence green for a typo. All three
class-expression checks reject any class, object property, data property or
individual the ontology does not declare, as `Indeterminate` naming the term.
This is the same guard already applied to Turtle-fragment predicates.

### 5.2 Golden closure diff

The primary defence for the untrusted direction, and the reason the design does
not need a complete reasoner to catch drift. The harness serialises the inferred
closure into a canonical, sorted golden file. Any change is diffed on every run.

This works precisely because it does not care about completeness. Both sides of
the diff come from the same oracle at the same version, so whatever rustdl cannot
see is held constant and cancels out. A regression harness needs to detect that
*the answer changed*, not to know absolute truth.

**What the golden diff actually covers, corrected after building it.** An earlier
revision of this section claimed the diff "delivers that for every entailment in
the closure rather than only the ones somebody thought to assert". That was false,
and measuring it was the most useful thing the implementation produced.

Shipped sensitivity surface: named class subsumption, per-class satisfiability,
named class equivalence, named object and data property subsumption and
equivalence, and the undecided-pair set.

Structurally blind to: property characteristics (transitivity, reflexivity,
functionality), property chains, domains and ranges, disjointness, disjoint-union
covering, and every ABox-level entailment.

The consequence, measured by running `golden` against each of the ten mutants in
`mutants/` and recording the exit code: the golden diff detects **two of ten**.
This figure said "one of four" until the six later mutants were measured against
it; it is a measurement, so re-run it when a mutant is added rather than
predicting what the new one should do. No test pins the figure.

Caught: `no-subproperty-containment` moves the property hierarchy;
`no-feature-object` deletes `Feature subClassOf Object`, a named-to-named edge,
which is exactly the shape the class matrix records, and it costs 14 rows.

Blind to the other eight. `no-role-chain` because the reasoner's subproperty
materialisation explicitly skips a chain sub-expression. `no-transitive-parthood`
because `TransitiveObjectProperty` is not a materialised component kind.
`no-participant-domain-and-inverse-range` (domains and ranges) and
`no-object-process-disjoint` (disjointness) because both kinds are on the
structurally-blind list above. `no-selfpart-feature-and-informationobject`,
`no-selfpart-process`, and `no-quantity-unit-somevaluesfrom` because each removes
a `subClassOf` whose superclass is an anonymous restriction, which no named-class
subsumption, satisfiability, or equivalence entry can see, and no named class
becomes unsatisfiable. `no-feature-union` because it removes only the covering
half, while the four `subClassOf Feature` edges are asserted separately and
pairwise disjointness survives in the redundant `AllDisjointClasses`.

Three of the five components this section originally listed are therefore still
absent: inferred class assertions, inferred property assertions, and inferred
disjointness. That is where most of the eight blind mutants live. Closing that
needs a fixed probe ABox, since `sulo.ttl` declares no individuals, so it is a
subsystem rather than a fix and belongs to the follow-on plan. Until it lands,
the hand-written assertions and the mutation suite carry the load (which catches
all ten), and this section states the gap rather than implying coverage that does
not exist.

The golden file header records the rustdl version. A version mismatch is a
distinct outcome, "re-baseline required", never a silent pass and never a Fail.
Re-baselining is explicit: `--accept-golden`.

### 5.3 HermiT differential in CI

HermiT is complete for OWL 2 DL, so it is the oracle of record for exactly the
answers soundness cannot vouch for. A CI-only job cross-checks every negative
assertion and every consistency verdict against HermiT via the existing ROBOT
setup. This also covers the two gaps rustdl provably cannot see today: the
data-range `allValuesFrom` (section 9).

**And every positive assertion rustdl reported as a `Fail` because it found no
proof.** Scoping the differential to negatives and the gate was the original
plan and was too narrow: `oracle::verdict_for` tells the user, on exactly those
Fails, "Incompleteness is a possible cause; the CI differential settles it".
That `Fail` rests on absence of proof precisely as a negative `UnrefutedPass`
does, so leaving it out would have shipped that sentence as a falsehood. It is
also the most valuable direction. A positive `Fail` HermiT cannot prove either
is a real SULO regression, and the two reasoners agreeing on it (one of them
complete) is a proof of absence; a positive `Fail` HermiT CAN prove is a rustdl
incompleteness rather than an ontology regression, and the report says so and
names rustdl as the outlier.

Disagreement between the two reasoners is its own verdict, **Divergence**, and it
is always loud. It does not mean SULO regressed; it means one of the two
reasoners is wrong, which is the most valuable signal either could produce.

The JVM stays out of the default and local path. It is a CI job only.

### 5.4 Exit codes

- `0` all checks pass (unrefuted Passes reported in the summary)
- `1` any Fail
- `2` harness or configuration error (bad YAML, missing file, parse failure)
- `3` any Indeterminate, unless `--allow-indeterminate`, which lowers 3 to 0
  and can never suppress a `Fail` (Fail outranks Indeterminate in aggregation)
- `4` golden closure drift, or re-baseline required; also the `differential`
  subcommand's stale pin, meaning a divergence the pin describes that no longer
  occurs, or a pin whose provenance header does not match this run (ruling 13:
  that is the opposite finding to a `5`, so it does not share the code)
- `5` oracle divergence between rustdl and HermiT, meaning a divergence no pin
  describes

Precedence within `differential` is `5` over `4` over `3` over `0`. A live
disagreement is news about the two reasoners; a stale or uncomparable pin is
news about the pin, and the report is printed either way so a `4` never hides a
`5`. A PINNED divergence whose question came back Indeterminate is UNCONFIRMED,
which is `3` and not `0` or `4`: a question HermiT could not answer is evidence
of nothing in either direction.

Check verdicts aggregate worst-first within a case: Fail beats Indeterminate
beats unrefuted Pass beats Pass.

## 6. Architecture

A single Rust crate, library plus a thin CLI. No workspace.

```
sulo-testharness/
  Cargo.toml
  src/
    lib.rs
    manifest.rs   # YAML case -> typed Case struct (serde_yaml), schema validation
    prefixes.rs   # the prefix map every entity-naming field resolves through
    load.rs       # horned-owl ingest: ontology + data files -> one SetOntology
    claim.rs      # entails / not_entails Turtle fragment -> typed Claims
    oracle.rs     # Claims -> owl-dl-reasoner queries -> Verdict
    verdict.rs    # the four verdicts, their precedence, and the exit codes
    materialize.rs # asserted + inferred triples -> one oxigraph store
    rows.rs       # expect_rows comparison semantics (7.3)
    cq.rs         # materialise -> oxigraph store -> SPARQL -> row comparison
    golden.rs     # the inference-closure diff (5.2)
    suite.rs      # discovery, filtering, per-case orchestration
    report.rs     # pretty stdout, --format json, --format junit
    main.rs       # `run` and `golden` subcommands
  tests/          # integration tests, including the mutation self-test
  suites/sulo/    # the reference SULO suite
  mutants/        # deliberately broken sulo.ttl variants, for self-testing
  action.yml      # composite GitHub Action for consumer CI
  .github/workflows/  # ci.yml, release.yml
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
| `:x :p :y`, `:p` an object property | PropertyAssertion | `instances-expr "p value :y"`, membership of `:x` |
| `:x :p "lit"`, `:p` a data property | DataPropertyAssertion | `instances-expr "p value lit"` |

A statement matching no row is a configuration error (exit 2), never a silent
skip.

**Note on PropertyAssertion.** It must go through an entailment check, not
through enumeration. Verified: `property-values` returns the transitive closure
and the subproperty propagation but emits **no reflexive self-loops**, so an
`entails: ex:d sulo:isPartOf ex:d` case dispatched to it fails spuriously on
healthy SULO even though `isPartOf` is declared reflexive. The entailment is
genuinely there: `instances-expr "(<isPartOf> value <ex:d>)"` returns `ex:d`.
`property-values` is therefore reserved for CQ materialisation (section 8 step
6), where the same gap means reflexive self-loops must be injected separately or
a CQ pattern `?x sulo:isPartOf ?x` silently returns nothing.

**Note on ClassAssertion.** It must be backed by `instances`, not by `realize`.
Verified during design: `realize` reports only the most specific type, so for the
SOLID example it returns `alice_temp_1 a Quantity` and nothing else, whereas
`instances` returns the full closure and correctly lists `alice_temp_1` under
`Quantity`, `InformationObject`, `Feature`, and `Object`. Backing the claim with
`realize` would make every non-most-specific class assertion fail spuriously.

**Note on DisjointUnion, and why the harness expands it anyway.** An earlier
revision of this spec recorded, as a measured fact, that rustdl "does not enforce
the covering half of `DisjointUnion` in the ABox path", and an adversarial review
independently "confirmed" it. **Both measurements were wrong, and the error is
worth recording because of how it happened.**

Both were taken through a locally built `rustdl` CLI binary sitting at
`target/release/rustdl` in a sibling checkout. **Neither of us checked what that
binary actually was.** Re-measured properly by building each commit from source:

| Version | Verdict on the covering violation |
| --- | --- |
| that stale binary (reports `0.4.2`, built weeks earlier) | `consistent`, WRONG |
| `v0.4.22` (`666d31b`), the pinned tag | `inconsistent`, correct |
| `f1ab66b` | `inconsistent`, correct |
| `v0.4.23` (`dc662a3`) | `inconsistent`, correct |

So there is **no regression**, and there never was one after `v0.4.22`. The defect
lives only in an old `0.4.2`-era build that happened to be lying around in a
`target/` directory. Every version this project could plausibly depend on handles
`DisjointUnion` correctly.

The lesson generalises past this one axiom, and it is sharper than the first
version of this note claimed: **identify the binary, do not infer it from the
repository's `HEAD`.** A checkout's `git rev-parse HEAD` says nothing about when
`target/release/` was last built or from what. `--version` and the file's mtime
would have caught this in seconds. The conclusion survived my own writing, an
adversarial review by a second model, a spec revision, and a code workaround built
on top of it, before a rebuild-from-source disproved it.

Two consequences stand, for reasons that survive the correction:

1. **Covering is still checked as an entailment, not as a consistency probe.**
   `subclass-expr Feature "(Capability or InformationObject or Quality or Role)"`
   returns `entailed: true` on clean SULO, and the deliberate non-covering
   `Object ⋢ (SpatialObject or Feature)` returns `entailed: false`. This is the
   better formulation regardless of the bug: it asserts the axiom's content
   directly rather than inferring it from a counter-example, and it gives the two
   deliberate non-coverings something to assert that is not vacuous.
2. **`load.rs` still expands every `DisjointUnion(C, D1..Dn)`** into
   `EquivalentClasses(C, ObjectUnionOf(D1..Dn))` plus `DisjointClasses(D1..Dn)`.
   This is **not** a bug workaround. It is a semantics-preserving explicitation:
   the two axioms are exactly what `DisjointUnion` abbreviates, so the expansion
   cannot make the ontology assert anything false. It is kept as defense-in-depth
   against precisely the class of oracle regression documented above, so the
   harness does not silently depend on the reasoner implementing the covering half
   natively. Because the expansion is semantics-preserving, it needs no version
   condition and no removal date.

Note also that the Manchester parser requires full `<IRI>` forms or a declared
prefix map; bare names are rejected. See section 7.

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

`horned-owl` (git rev `b188eda`, matching rustdl's pin exactly, see section 4),
`owl-dl-reasoner`, `oxigraph`, `oxrdfio`, `serde`, `serde_yaml`, `clap`. Pinned
exactly, because the oracle's behaviour is the harness's semantics.

**The horned-owl pin is a hard constraint, and naming the git rev directly does
not achieve it.** Measured during phase 1. rustdl's `owl-dl-reasoner` depends on
**crates.io** `horned-owl = "1.4"`, and cargo will not unify a git dependency
with a crates.io dependency on the same package. Declaring
`horned-owl = { git = ..., rev = ... }` in `[dependencies]` therefore yields two
copies, `cargo tree -p horned-owl` reports "ambiguous specification", and the
`SetOntology` the harness parses becomes a different type from the one
`owl_dl_reasoner::is_consistent` accepts.

The form that actually resolves is a crates.io dependency plus a patch, which is
how rustdl's own `Cargo.toml` does it:

```toml
[dependencies]
horned-owl = { version = "1.4", default-features = false }

[patch.crates-io]
horned-owl = { git = "https://github.com/micheldumontier/horned-owl", rev = "b188edaf7c92600918f0524962d928097ecd6b4d" }
```

Verify with `cargo tree -p horned-owl`, which must print exactly one instance, and
with `Cargo.lock`, which must hold a single `horned-owl` whose `source` is the
pinned git rev.

## 7. Manifest schema

Everything past `id` and `description` is optional.

```yaml
id: pro-role-chain
description: Participation of the role holder is recovered via the PRO role chain.
ontology: sulo.ttl                    # default comes from --ontology
imports: []                           # extra ontologies to merge
data: data/pro-encounter.ttl           # string or list
prefixes:                              # merged over the suite-level defaults
  ex: http://example.org/
expect_inconsistent: false             # true flips the consistency gate
entails: |                             # Turtle fragment
  ex:encounter sulo:hasParticipant ex:alice, ex:drsmith .
not_entails: |
  ex:alice sulo:hasParticipant ex:encounter .
instance_of_expr:                      # individual is in a class expression
  - { individual: "ex:encounter",
      expr: "Process and hasParticipant some (Role and isFeatureOf some Object)" }
satisfiable_expr:                      # expression must have a model
  - "Process and hasParticipant some (Role and isFeatureOf some Object)"
entails_manchester:                    # sub_expr must be subsumed by sup_expr
  - { sub_expr: "Feature",
      sup_expr: "Capability or InformationObject or Quality or Role" }
unsatisfiable: []                      # classes required to be unsatisfiable
cq:
  - query: queries/who-participated.rq
    expect_rows: [{ p: "ex:alice" }, { p: "ex:drsmith" }]
    exact: true                        # false = subset check
    ordered: false                     # default set comparison
tags: [pattern, pro]
timeout_ms: 30000
```

Everything past `id` and `description` is optional, but a case must assert
*something*: at least one of `entails`, `not_entails`, `entails_manchester`,
`not_entails_manchester`, `instance_of_expr`, `satisfiable_expr`,
`unsatisfiable`, `cq`, or `expect_inconsistent: true`. A manifest with only `id`
and `description` is rejected (`ManifestError::NoAssertions`), and an `entails:`
block that parses to zero triples (empty, or only comments) is reported as
`Indeterminate`. Both are the same failure `deny_unknown_fields` guards against,
reached from a different direction: a case that asserts nothing otherwise
reports a confident green.

`tags` is parsed and carried but nothing reads it yet: the `--tag` case filter
is deferred. A tag today is documentation, not a selector.

**Known limitation, pinned reasoner v0.4.22: a language-tagged literal in
`entails` can never succeed.** rustdl cannot positively confirm
`rdf:langString` `DataHasValue` membership by *any* path, verified against real
SULO: not the materialised `inferred_data_property_values` (which drops the
language tag entirely), not the bounded satisfiability probe, and not the
retired unbounded `class_expression_instances`, even for an individual with that
exact literal asserted directly. So

```yaml
entails: |
  ex:n sulo:hasValue "bonjour"@fr .
```

is a permanent `Fail` (or, wherever any axiom loss is present, a permanent
`Indeterminate`), and no change to the ontology can fix it. This is a reasoner
completeness gap, not a SULO defect. Assert such a value under `not_entails` if
it must be mentioned at all, and expect `Pass (unrefuted)`. The same note lives
on `manifest::Case::entails` so an author reading the code hits it too.

### 7.1 Why `entails_manchester` changed shape

An earlier draft's canonical example was
`{expr: "Process and hasParticipant some (Role and isFeatureOf some Object)",
subclass_of: "Process"}`. That asserts `C ⊓ D ⊑ C`, a tautology: verified
`entailed: true` against a declarations-only ontology carrying none of SULO's
axioms. Any implementer copying the schema's own example would have shipped a
case that cannot fail.

The field is now a two-expression subsumption (`sub_expr`, `sup_expr`), which is
what the covering checks need. Testing the PRO pattern properly needs a
different question, so two fields exist for it: `instance_of_expr` asks whether a
named individual falls in the pattern expression, which exercises the chain and
the typing together, and `satisfiable_expr` guards against the pattern becoming
unsatisfiable.

### 7.2 Prefix resolution

Every Turtle fragment, Manchester expression, and `expect_rows` value resolves
CURIEs against one prefix map, assembled in this order, later winning:

1. `sulo:` bound to `https://w3id.org/sulo/`, and the standard `rdf:`, `rdfs:`,
   `owl:`, `xsd:`, `skos:` bindings, always injected.
2. A suite-level `prefixes.yaml`, holding the shared bindings (`ex:`, `obo:`).
3. The case's own `prefixes:` block.

The map is held as a `curie::PrefixMapping`, which both consumers accept
directly. Turtle fragments are parsed with it prepended as `@prefix` lines, so
authors never declare prefixes inline. Manchester expressions are handed the same
mapping via `horned_owl::io::omn::reader::parse_class_expression(s, pm, build)`,
which resolves CURIEs natively, so no rewriting to full `<IRI>` form is needed.
Bare unprefixed names remain invalid; the parse failure observed during design
came from passing an empty `PrefixMapping`, not from a parser limitation. An
unresolvable CURIE anywhere is a configuration error (exit 2), never a failed
check.

### 7.3 `expect_rows` comparison semantics

Undefined comparison rules were the largest ambiguity in the earlier draft. The
rules are:

- A value is compared by RDF term, not by string. Each expected value is parsed
  into a term first: `ex:alice` becomes an IRI via the prefix map, `<http://...>`
  an IRI, `"37.8"^^xsd:double` a typed literal, `"text"@en` a language literal,
  and a bare `"37.8"` an `xsd:string` literal.
- Literal equality is RDF term equality, so `"37.8"` does **not** equal
  `"37.8"^^xsd:double`. Authors must write the datatype they expect. This is
  deliberate: value-space equality would hide serialisation regressions, which is
  a thing the harness exists to catch.
- Rows are compared as a multiset, so duplicate rows are significant. `ordered:
  true` compares as a sequence instead, and is only valid with an `ORDER BY` in
  the query. That last clause is enforced, not merely stated: `cq::check_cq`
  reports `Indeterminate` for `ordered: true` over a query with no `ORDER BY`,
  because SPARQL leaves the row order arbitrary there and the comparison would
  be a coin flip reported as a verdict.
- `exact: true` requires set equality. `exact: false` requires every expected row
  to be present, extra rows allowed. Never the reverse.
- A variable expected to be unbound is written `null`. A row whose variable is
  unbound in the result matches only `null`.
- Blank nodes never compare equal across runs and are a configuration error in
  `expect_rows`. Suite data uses skolemised IRIs instead (section 9).
- An expected row must name **every** variable the query projects. Rows compare
  as whole maps, and an actual row always carries one key per projected variable
  (bound, or explicitly unbound), so a row that omits one can never match. An
  absent key is not "not compared"; it is a row that cannot match.

Two `cq` configurations are refused by `load_case` as `ManifestError` variants
(exit code 2, a configuration error), not run and reported as a check:

- `ordered: true` with `exact: false`. The rules above do not say whether an
  unmatched actual row may appear before, between, or only after the expected
  sequence, so the combination is undefined rather than silently given one of
  its two equally licensed readings. Use `ordered: true, exact: true` for an
  exact sequence, or `ordered: false, exact: false` for an unordered subset.
- An empty `expect_rows` with `exact: false`. Every expected row is trivially
  accounted for and extra actual rows are tolerated, so the check passes
  whatever the query returns: a check that cannot fail, which is
  `ManifestError::NoAssertions` reached one level in. An empty `expect_rows`
  with `exact: true` is the legitimate "this query must return nothing"
  assertion and is accepted.

Both are decidable from the manifest alone, which is why they are refused at
load rather than routed to `Indeterminate` at check time: `Indeterminate`
(exit 3) means the reasoner could not answer, and rejecting at load catches the
mistake even when the ontology itself fails to load. The `cq` situations that DO
yield `Indeterminate` all require the `.rq` file to be read or the query to be
run. `cq::check_cq` has eight of them: an unreadable query file, a parse failure,
an execution failure, an `ASK`, a `CONSTRUCT`/`DESCRIBE`, a failure part-way
through the result stream, `ordered: true` over a query with no `ORDER BY`, and
an `expect_rows` token the term parser rejects. A ninth is raised by
`suite::run_case` rather than by `check_cq`: a materialisation failure, reported
once per `cq` entry, because the store the questions would have been asked
against was never built. This list was previously written as a closed count of
four while the code had seven exits; enumerate it, and keep it in step with the
`indeterminate` call sites, which are the ground truth.

`ordered: true` without an `ORDER BY` belongs on that list and not at load time,
and it is the case that shows the load-versus-check split is a real distinction
rather than a convention: the manifest does not contain the query text, so no
load-time guard could see it. The reverse holds for `ordered: true` with
`exact: false`, which is settled by two booleans in the manifest and would be
unreachable if it were also guarded here.

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
   - `expect_inconsistent: true` and consistent: **Fail**, caveated. This is the
     axiom-stopped-biting regression and the entire point of these cases, but
     "consistent" is exactly the direction soundness does not vouch for, and
     `rustdl consistent` exposes no `incomplete` flag to condition on. So the
     Fail is reported with its caveat and routed to the CI differential (5.3),
     where HermiT settles it. A dropped axiom or incomplete parse downgrades it
     to Indeterminate instead, per section 12.
   - expecting consistent and inconsistent: **Fail** the case, skip the rest,
     and report the clashing axioms via `justify`.
4. **Positive entailments** dispatched per the claim table.
5. **Negative entailments**, the same queries with inverted expectations.
6. **Competency questions.** Build the store, then query. "Materialise" is
   defined concretely, because leaving it vague would let two implementers build
   stores with different contents and the same CQ pass on one and fail on the
   other. The store contains exactly:
   - every asserted triple from the ontology and data files,
   - every inferred class assertion, from `instances` over all 17 named classes
     (the full closure, not most-specific types),
   - every inferred object and data property assertion, from `property-values`,
   - plus, injected separately, the reflexive self-loops `x isPartOf x` and
     `x hasPart x` for every named individual, which `property-values` omits
     (section 6.1). Without this a CQ pattern `?x sulo:isPartOf ?x` silently
     returns nothing despite the axiom.

   Named individuals only. Blank nodes are outside `property-values` coverage, so
   suite data uses skolemised IRIs. Then run the SPARQL and compare per section
   7.3.
7. **Golden closure diff.** Serialise the canonical closure and diff it against
   the committed golden file (section 5.2). Runs once per ontology, not per case.
8. **Aggregate** verdicts worst-first.

## 9. Suite inventory

Roughly 70 cases in six groups, under `suites/sulo/`.

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
- the **covering** half of the two disjoint unions, as `entails_manchester`
  subsumptions rather than consistency probes (section 6.1):
  `Feature ⊑ (Capability or InformationObject or Quality or Role)` and
  `Time ⊑ (Duration or TimeInstant or TimeInterval)`
- the two deliberate **non**-coverings, pinned as intentional, likewise as
  non-entailments: `Object ⋢ (SpatialObject or Feature)` and
  `InformationObject ⋢ (Collection or Quantity)`. Without these, someone
  "completing the pattern" by adding a third disjoint union would break
  downstream data and pass CI.

**properties**

- all four non-trivial subproperty axioms: `isDirectPartOf` under `isPartOf`,
  `hasDirectPart` under `hasPart`, and the two easily-missed ones,
  `isPartOf` under `isIn` and `hasPart` under `contains`. The latter pair gets
  entailment cases driven from an asserted `isPartOf` chain, since a CQ phrased
  over `isIn` or `contains` depends on them entirely.
- all 9 inverse pairs round-trip: `atTime`/`isTimeOf`,
  `isPrecededBy`/`precedes`, `isReferredToIn`/`refersTo`, `contains`/`isIn`,
  `hasFeature`/`isFeatureOf`, `hasItem`/`isItemIn`,
  `hasParticipant`/`isParticipantIn`, `hasDirectPart`/`isDirectPartOf`,
  `hasPart`/`isPartOf`
- transitivity closes over a three-step chain for all four transitive
  properties, `isPartOf`, `hasPart`, `isIn`, and `contains`, not just the first
  two
- `isPartOf` and `hasPart` reflexivity, checked via `instances-expr` per
  section 6.1 and not via enumeration
- `hasValue` functionality, enforced by two distinct literals going inconsistent
- domain and range axioms as entailed class assertions: `:p hasParticipant :o`
  entails `:p a Process` and `:o a Object`
- a range violation going inconsistent
- **`isDirectPartOf` must not close transitively.** This is the axiom a
  well-meaning edit would "fix", and it is what keeps OWL 2 cardinality
  restrictions legal over the property.

**restrictions**

`sulo.ttl` carries 16 class-expression restriction axioms that an earlier draft
of this spec barely mentioned, leaving them the suite's softest spot: deleting
any one of them passed everything. The full inventory, extracted from the OFN:

- **5 `hasPart` propagation axioms**, `C ⊑ ∀hasPart.C` for `Object`, `Process`,
  `SpatialObject`, `Feature`, and `InformationObject`. Each gets an entailment
  case driving a part of a `C` and requiring the part to be typed `C`. Only
  `Feature`'s was previously exercised, and only incidentally, via the SOLID
  unit.
- **6 object `someValuesFrom` axioms**: `Quantity ⊑ ∃hasPart.Unit`,
  `Feature ⊑ ∃isFeatureOf.(Object or Process)`, and `TimeInterval`'s four
  (`∃hasDirectPart.StartTime`, `∃hasDirectPart.EndTime`, `∃hasPart.Duration`,
  `∃hasPart.Unit`). All checked as `entails_manchester` subsumptions.
- **1 data `someValuesFrom`**: `Duration ⊑ ∃hasValue.decimal[≥ 0]`. Confirmed
  enforced: a `Duration` with `-5.0^^xsd:decimal` is correctly inconsistent, so
  the facet bites and this is testable as a counter-example.
- **1 data `allValuesFrom` that rustdl cannot enforce**:
  `TimeInstant ⊑ ∀hasValue.(dateTime ∪ dateTimeStamp)`. A `TimeInstant` with
  `"hello"^^xsd:string` is reported **consistent**, with no dropped-axiom
  diagnostic. This is a silent-loss channel, so the case is marked
  `oracle: hermit` and runs only in the CI differential (5.3).

**Three of the 16 are semantically inert**, and the suite says so rather than
pretending to test them:

- `Collection ⊑ ∀hasItem.owl:Thing` is a tautology; every value is an
  `owl:Thing`.
- `InformationObject ⊑ ∀hasValue.rdfs:Literal` is likewise vacuous.
- `Object ⊑ ¬∃hasPart.Process` is derivable from `Object ⊑ ∀hasPart.Object`
  plus `Object disjointWith Process`, so deleting it alone is
  semantics-preserving.

No test can fail on any of these three, so each is recorded here with a comment
in the suite explaining why it has no case. That is a deliberate documented
absence rather than an oversight, and it is worth raising with the SULO authors
separately as possible cleanup.

**domain and range**

An earlier draft covered `hasParticipant` only. The other 16 object properties
had no domain or range case, and the PRO case types every individual explicitly
so it never exercised them either. Now covered: `precedes` and `isPrecededBy`
(`Process` to `Process`), `atTime` range `Time`, `hasItem` domain `Collection`,
`isItemIn` range `Collection`, `refersTo` domain `InformationObject`, and the
union domain and range on `hasFeature` and `isFeatureOf`. Each is an entailed
class assertion driven from a bare property assertion between untyped
individuals, plus a violation going inconsistent where the range is disjoint
from something.

**patterns/pro**

- Figure 7's data, **faithfully adapted rather than verbatim**, with the role
  chain firing to yield `encounter hasParticipant alice, drsmith`
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

### 9.1 Paper errata, and why "verbatim" is impossible

Neither listing can be used as printed. The repairs are enumerated here because
the suite data must be auditable against the paper, and because the list is
itself a useful errata report for the SULO authors:

- **Both figures** write the namespace as `http://w3id.org/sulo/`; the real one
  is `https://w3id.org/sulo/`. The competency-question tests are exactly what
  catches this class of typo in downstream data.
- **Figure 7 is not valid Turtle.** Its `@prefix obo:` line has no terminating
  dot; a stray `.` after the `:encounter` type assertion orphans the following
  `sulo:hasParticipant` line; and `taxon:` is used but never declared.
- **Figure 7's stated inference names the wrong subject**, giving
  `:visit_1 sulo:hasParticipant :alice, :drsmith` where the data defines
  `:encounter`. The suite uses `:encounter`.
- **Figure 7's roles are typed only as OMRSE classes** (`OMRSE_00000011`,
  `OMRSE_00000012`), which are not imported, so nothing makes them `sulo:Role`.
  The suite types them `sulo:Role` explicitly.

  **Corrected after measuring.** An earlier version of this bullet said the chain
  "cannot fire" without that typing. That is false. `hasParticipant`'s
  `owl:propertyChainAxiom ( hasParticipant [ owl:inverseOf hasFeature ] )` carries no
  class conditions on any position, so `encounter hasParticipant alice, drsmith`
  follows from Figure 7's data as printed, which is exactly the inference Figure 7
  claims. The repair is still substantive, but for a different reason: the `sulo:Role`
  typing is required for **Figure 5's pattern class expression**
  (`Role and isFeatureOf some Object`), not for **Figure 6's role chain**. Since this
  list is intended for the SULO authors, the distinction matters: Figure 6 works as
  published, Figure 5 does not apply to Figure 7's data without the added typing.
- **Figure 4 puts the unit and quality in blank nodes.** rustdl's
  `property-values` covers named individuals only, so blank-node values are
  invisible to the CQ path. All suite data uses skolemised IRIs.

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
| delete `Feature`'s `disjointUnionOf` **only** | `taxonomy/covering-feature` alone (see below) |
| delete `Feature`'s `disjointUnionOf` **and** its `AllDisjointClasses` | `taxonomy/covering-feature` plus the 6 `Feature`-sibling counter-examples |
| delete `Time`'s `disjointUnionOf` and its `AllDisjointClasses` | `taxonomy/covering-time` plus the 3 `Time`-sibling counter-examples |
| **add** an `Object owl:disjointUnionOf (SpatialObject Feature)` | `taxonomy/non-covering-object` |
| delete one `owl:inverseOf` | `properties/inverses` |
| delete `isPartOf rdfs:subPropertyOf isIn` | `properties/subproperty-isin` |
| delete `hasPart rdfs:subPropertyOf contains` | `properties/subproperty-contains` |
| delete `Object ⊑ ∀hasPart.Object` | `restrictions/propagation-object` |
| delete `Quantity ⊑ ∃hasPart.Unit` | `restrictions/somevalues-quantity-unit` |
| delete the `Duration` decimal facet | `restrictions/duration-nonnegative` |
| delete `hasParticipant`'s range | `domains-ranges/hasparticipant` |
| delete `atTime`'s range | `domains-ranges/attime` |

### 10.1 Three mapping errors this table used to contain

Worth recording, because each was wrong for an instructive reason and the
corrected rows above depend on understanding why.

1. **"Delete `Feature`'s `disjointUnionOf`" was mapped to the 6 sibling
   counter-examples.** It should not touch them. The redundant
   `AllDisjointClasses` axiom (section 2) still asserts pairwise disjointness, so
   the siblings still clash and only the covering case reacts. Under the current
   toolchain the sibling probes *do* go consistent on this mutant, but only
   because horned-owl drops `AllDisjointClasses` entirely, so the mapping
   "worked" by riding a parser bug and would break the moment that bug is fixed
   or HermiT is asked. Hence the split into two rows: deleting the union alone,
   and deleting both.
2. **"Weaken the `disjointUnionOf` to a plain `disjointWith`"** was listed as a
   distinct mutant, but given the same redundancy it is behaviourally
   indistinguishable from row 5. Removed.
3. **"Add an `Object disjointUnionOf`" was uncatchable as originally
   specified.** With `taxonomy/non-covering-object` written as an
   `expect_inconsistent` probe, the mutant came back consistent and the case
   passed, making the mutant uncaught and the case vacuous. It only became
   catchable once the non-covering checks were respecified as non-entailments
   (section 6.1). Verified on the mutant:
   `subclass-expr Object "(SpatialObject or Feature)"` returns `entailed: true`,
   against `entailed: false` on clean SULO.

A mutant that no case catches is a coverage hole, and is reported as one. This
is the only evidence that the cases are load-bearing rather than decorative, and
it doubles as the harness's own regression suite. The three errors above are also
the argument for building phase 5 early rather than last: every one of them was
invisible to review and obvious to a mutant.

## 10.2 The CLI surface

Three subcommands. Every one of 5.4's exit codes, `0` through `5`, is observed
by `tests/cli.rs` by launching the binary; a unit test over `verdict::exit_code`
cannot catch a `main` that forgets to propagate, or that aggregates the wrong
set, or that prints a report and returns success anyway. Exit `5` is observed on
`restrictions/timeinstant-datarange`, where the two reasoners genuinely disagree,
and exit `0` is observed on a case where they agree: one direction alone would
not be evidence.

```
sulo-testharness run --suite <dir> [--ontology <ttl>] [--filter <substr>]
                     [--format text|json|junit]
                     [--deferred skip|include|only] [--allow-indeterminate]

sulo-testharness differential --suite <dir> --ontology <ttl> --robot <jar>
                              [--filter <substr>] [--format text|json]
                              [--workdir <dir>]
                              [--divergences <file>] [--accept-divergences]

sulo-testharness golden --ontology <ttl> --golden <file> [--accept-golden]
```

`--divergences` names the pinned set of KNOWN divergences
(`suites/sulo.divergences`), and it, not the raw divergence count, is what
decides a pinned run's exit code: a divergence the pin describes is documented
and exits `0`, one it does not is `5`, and one the pin describes that no longer
occurs is `4` (ruling 12 and ruling 13 of the differential plan). It cannot be
combined with `--filter`, because a pin claims a specific set is the WHOLE set
the suite produces and a filtered run never asks the questions outside the
filter. `--accept-divergences` re-baselines the pin, mirroring `--accept-golden`,
and is refused over a run holding an Indeterminate: accepting from a run with a
broken jar would write an empty pin and leave a permanently green job that
asserted nothing.

`differential` is its own subcommand rather than a flag on `run` because it
needs a JVM and a ROBOT jar, and neither may leak into the default or local
path. It refuses a run that asked NO questions for the same reason `run` refuses
a suite with no cases: nothing asked is not everything agreed. That guard cannot
fire against the suite as it stands, because every case yields at least its
consistency-gate question, and it is kept and tested anyway.

Three ways a run could check nothing and still report a pass are refused as
configuration errors (exit 2), not tolerated: a suite root holding no cases, a
`--filter` matching nothing, and a selection every one of whose cases is
deferred. A `*.yml` in the suite tree is refused by name for the same reason:
it would be read by nobody and reported by nothing.

`--deferred` governs the cases tagged `oracle-hermit`, whose oracle of record
is the CI differential rather than the pinned reasoner (5.3, and the note at
the end of section 9). The default, `skip`, names and counts them but does not
run them, and they are a distinct type from a case result so that reaching the
exit-code aggregation is structurally impossible rather than filtered against.
`only` runs exactly them under the pinned reasoner. The `differential`
subcommand does NOT use that seam: it includes every case unconditionally,
because a differential that skipped the cases it is the oracle of record for
would leave them checked by nothing.

## 11. CI integration

A release workflow builds a static `linux-x86_64` binary (musl, static-pie, so
it starts on an older glibc than the builder's), plus `macos-aarch64` for local
runs, and attaches both to a GitHub release along with the tag's own `suites/`
tree. That third asset is what makes the consumer snippet below work at all:
the action downloads a binary, but the cases live in this repository, so
without it a consumer would discover zero cases. Shipping them together also
guarantees the cases and the engine are the pair that were tested together.
`action.yml` is a composite action that downloads them for a pinned tag and
runs the suite. Consumer CI in the SULO repository becomes:

```yaml
- uses: MaastrichtU-IDS/sulo-testharness@v0.1.0
  with: { ontology: sulo.ttl }
```

**On the existing reasoning job: it stays permanently.** An earlier draft
proposed keeping `reasoning.yml` for one release cycle and retiring it "once
rustdl and HermiT have agreed across a few releases." That reasoning was
backwards. The two reasoners agree on healthy SULO by construction, both report
it consistent, so agreement across releases is evidence of nothing. They diverge
exactly when a covering violation or a data-range violation appears, which is
precisely the regression class rustdl provably cannot see (sections 6.1 and 9).
Retiring HermiT on the strength of routine agreement would remove the oracle at
the only moment it would ever have spoken.

So HermiT is promoted from transitional cross-check to the permanent oracle of
record for the untrusted direction, per 5.3. It runs as a CI-only job, covering
every negative assertion, every consistency verdict, **every positive assertion
rustdl reported as a `Fail` because it found no proof** (5.3, ruling 7: that
`Fail` rests on absence of proof exactly as an unrefuted negative does), and the
cases marked `oracle: hermit` because rustdl cannot enforce them. The JVM stays
out of the default and local path.

The job is weekly and on demand rather than on push, and it is green when the
world matches the documented state: the one real disagreement is pinned in
`suites/sulo.divergences` with both reasoners' answers, and the pin is diffed in
both directions so that the day the gap CLOSES is a failure too. A job that is
permanently red gets muted, and a muted alarm is the same defect as a check that
cannot fail.

## 12. Error handling

- Malformed YAML, unknown manifest keys, missing referenced files, and
  unparseable `.ttl` or `.rq` files are configuration errors: exit 2, reported
  with the manifest path and the offending key or line. They are never silently
  skipped and never reported as test failures.
- A statement in an `entails` or `not_entails` fragment that matches no row in
  the claim table is a configuration error, not a skipped check.
- **Axiom loss is detected on both channels.** A non-empty `dropped_axioms` from
  the rustdl conversion, *and* a non-empty `IncompleteParse` from horned-owl,
  each raise a suite-level warning naming what was lost. The parse channel is not
  optional: horned-owl silently drops SULO's two `AllDisjointClasses` axioms
  today (section 2), and without this check the harness reasons over a weaker
  ontology than the one that ships and never says so.
- **The loss downgrade is symmetric.** An earlier draft downgraded only
  positive-entailment Fails, which fixed half the problem and left the more
  dangerous half trusted. Reasoning over a subset `O'` of `O` is monotonic:
  "entailed by `O'`" implies "entailed by `O`", so a positive Pass and a negative
  Fail stay trustworthy. But "not entailed by `O'`" says nothing about `O`, and
  that unreliable answer appears in *four* places, not one. On any axiom loss,
  all four are downgraded to Indeterminate:
  - a positive-expectation Fail,
  - a negative-expectation Pass,
  - an `expect_inconsistent` Fail (inconsistency is a positive entailment of
    falsehood, so a lost axiom removing the clash is exactly analogous),
  - a "consistent" verdict from the gate.

  Leaving the last three trusted is how a dropped axiom turns into a green build,
  and the `DisjointUnion` covering loss in section 6.1 is a live instance of the
  same class of error occurring inside the reasoner rather than the parser.
- Query timeouts produce Indeterminate, never Fail. The per-case budget comes
  from `timeout_ms`, defaulting to 30000.
- An inconsistent ontology where consistency was expected reports the clashing
  axiom set via rustdl's `justify`, so the failure is actionable rather than
  just red. **NOT IMPLEMENTED, deferred.** The gate reports the unexpected
  inconsistency, names the case, and states that every check below it would have
  passed vacuously, but it does not call `justify` and does not print a clashing
  axiom set. Recorded here rather than left as a promise the code does not keep;
  closing it belongs in the follow-on plan alongside the deferred golden-closure
  components of 5.2.
- **The consistency gate is unbounded.** `owl_dl_reasoner::is_consistent` has no
  deadline-bearing variant at v0.4.22 (`is_consistent_with_stats` takes none
  either), so the gate cannot honour `timeout_ms`, has no `Indeterminate`
  timeout route, and a pathological case blocks the suite. Every other reasoner
  call the harness makes is bounded. Expressing the gate as a bounded
  `owl:Thing`-satisfiability probe was tried and rejected: it agrees with
  `is_consistent` on every fixture in the repository, but
  `is_class_satisfiable_with_timeout` skips the two ABox pre-checks
  `is_consistent` runs and short-circuits to "satisfiable" on a pure-EL
  ontology, so substituting it risks a gate that MISSES an inconsistency, which
  is strictly worse than one that can hang. See `suite::run_case`'s doc comment.

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

### 13.1 What the first validation pass missed, and what it got wrong

The table above tested only the paths that work. A later adversarial review tested
the paths that do not. It found four real problems, and it also produced one
**false** finding that I confirmed rather than checked, which is the more useful
lesson: the `DisjointUnion` covering row below was measured through an ad-hoc CLI
stale `0.4.2` binary that was never identified, and is retracted. Identify the
binary; do not infer it from the repository's HEAD. See section 6.1.

The table above tested only the paths that work. A subsequent adversarial review
tested the paths that do not, and found four problems serious enough to have
broken phase 1. This is recorded because the lesson generalises: a
design-validation pass that only confirms its own happy path is not validation.

| Probe | Result |
| --- | --- |
| `DisjointUnion` covering enforced in the ABox? | **RETRACTED TWICE.** First read `consistent` and called it a property of the pinned version. Then called it a post-`v0.4.22` regression. Both wrong: the binary was a stale `0.4.2` build. `v0.4.22`, `f1ab66b` and `v0.4.23` all handle it correctly. See section 6.1. |
| covering as an entailment instead? | works, and is retained as the better formulation. `Feature ⊑ (Capability or InformationObject or Quality or Role)`: `entailed: true`; the deliberate `Object ⋢ (SpatialObject or Feature)`: `entailed: false` |
| `property-values` emits reflexive self-loops? | **no.** No `x isPartOf x` for any individual, so the reflexivity case would have failed spuriously. `instances-expr "(isPartOf value ex:d)"` returns `ex:d` correctly. |
| `incomplete` flag rare on SULO? | **no.** `true` on essentially every non-EL query, including both covering checks. Invalidated the "Indeterminate should be empty" premise the verdict design rested on. |
| `sulo.ttl` disjointness axiom count | **9, not 7.** Two `AllDisjointClasses` axioms at lines 374 to 378, silently dropped by horned-owl. |
| non-trivial subproperty axioms | **4, not 2.** `isPartOf ⊑ isIn` and `hasPart ⊑ contains` were missed, and both fire: a parts chain yields `a isIn b`, `a isIn c`, `b isIn c`. |
| restriction axioms in `sulo.ttl` | **16**, of which the earlier draft covered essentially none. Three are semantically inert (section 9). |
| data-range `allValuesFrom` enforced? | **no.** A `TimeInstant` with `"hello"^^xsd:string`: `consistent`, no diagnostic. Facets do work: a negative `Duration` decimal is correctly inconsistent. |
| the `entails_manchester` schema example | a tautology. `C ⊓ D ⊑ C` returns `entailed: true` against a declarations-only ontology. |

**One implementation constraint discovered.** The `rustdl` CLI accepts OFN only,
with no format flag, so it cannot read `sulo.ttl` directly. This does not affect
the design, because the harness links `owl-dl-reasoner` as a library and does its
own parsing through `horned-owl` with `RdfFormat::Turtle`. It does mean phase 1
must not shell out to the `rustdl` binary as a shortcut, since that would silently
reintroduce an OFN conversion step and a subprocess boundary.

## 14. Build order

Six phases, each independently verifiable.

1. Crate skeleton: manifest parsing with the prefix map (7.2), loading with the
   `IncompleteParse` check and `DisjointUnion` pre-lowering (6.1), consistency
   gate, reporting. Runs end to end on a two-case suite. Includes a probe test
   for the known silent-loss channels, so a future toolchain upgrade that fixes
   or worsens them is noticed.
2. `claim.rs` and `oracle.rs`: positive and negative entailments, the verdict
   scheme of section 5.1.
3. **Mutants and self-tests, moved up from last.** Three of the original
   mutation mappings were wrong (10.1) and every one was invisible to review and
   trivial for a mutant to expose. Building this before the bulk of the suite
   means each case is proved load-bearing as it is written, rather than the whole
   suite being audited afterwards.
4. Golden closure diff (5.2), including the rustdl version pin and
   `--accept-golden`.
5. Competency-question path: the materialisation defined in section 8 step 6,
   oxigraph, and the comparison semantics of 7.3.
6. The SULO suite content, now roughly 70 cases across taxonomy, properties,
   restrictions, domain and range, and the two patterns.
7. HermiT differential job (5.3), covering negative assertions, consistency
   verdicts, the positive assertions rustdl could not prove, and the
   `oracle: hermit` cases. Done: `src/hermit.rs`, `src/differential.rs`, the
   `differential` subcommand, and `.github/workflows/differential.yml`.
8. Release binaries, `action.yml`, and the consumer workflow pull request to
   `AIDAVA-DEV/sulo`.
