# sulo-testharness

Regression and competency-question test harness for the [SULO](https://github.com/AIDAVA-DEV/sulo)
ontology. Rust, no JVM, using [rustdl](https://github.com/MaastrichtU-IDS/rustdl)
as an in-process OWL 2 DL reasoner.

## Why

SULO's CI checks that the ontology parses and that it is consistent as a whole.
Nothing asserts that a given subsumption still holds, that a disjointness axiom
still excludes anything, or that the PRO role chain still fires. An edit to
`sulo.ttl` fails CI only if it breaks the entire ontology; every other regression
ships silently.

This harness makes those regressions fail.

## Status

Complete and passing, 260 tests: the engine (manifest parsing, hermetic Turtle
loading with axiom-loss detection, typed claims, the reasoner oracle, Manchester
class-expression checks, a consistency gate, a golden inference-closure diff),
the competency-question path (SPARQL over a materialised inference closure), the
SULO suite itself, and a CLI with a composite GitHub Action for consumer CI.

### The suite

66 cases over six groups, each group's expected verdicts pinned in a table that
is diffed against the directory listing in both directions, so neither a new
case missing from the table nor a stale entry can hide.

| Group | Cases | What it pins |
| --- | ---: | --- |
| `taxonomy` | 22 | Asserted and inferred subsumptions, non-subsumptions, 14 disjointness counter-examples, covering axioms, satisfiability |
| `properties` | 9 | Inverse pairs, transitivity (and its absence for `isDirectPartOf`), reflexivity, subproperty axioms, functionality |
| `restrictions` | 12 | `hasPart` propagation, `someValuesFrom` restrictions, the duration data range |
| `domains-ranges` | 14 | Domain and range entailments for every object property, plus a range violation |
| `patterns/pro` | 4 | The Process-Role-Object pattern and its role chain, including a competency question |
| `patterns/solid` | 5 | The Single Object Literal Information Datum pattern, including a competency question |

### Mutation self-test

A suite that cannot fail is worse than no suite. Ten mutants, each a single
documented edit to SULO, prove the suite bites: every `assert_caught` requires
BOTH directions, a `Pass` on clean SULO and a `Fail` on the mutant. Every group
has at least one caught mutant, and both competency questions are mutation
proven.

The mutants are re-derived in Rust from a live read of `../sulo/sulo.ttl` on
every run and compared byte for byte, so a SULO bump that the mutants do not
reflect is a build failure rather than a suite quietly testing a frozen
ontology.

### HermiT differential

rustdl is sound but incomplete, so it cannot certify a non-entailment. HermiT is
complete for OWL 2 DL, so the `differential` subcommand puts every
absence-resting answer the harness produces back to HermiT through ROBOT: every
consistency verdict, every negative assertion, and every positive assertion
rustdl reported as a Fail because it could not find a proof. A disagreement is
`Divergence`, and it is reported with BOTH answers, because the point is that
one of the two reasoners is wrong and the reader has to be able to tell which.

It needs a JVM and a ROBOT jar, so it is a CI-only job
(`.github/workflows/differential.yml`) and never runs on the default path. On
SULO today it asks 92 questions and finds one real divergence, on
`timeinstant-datarange`: rustdl cannot represent the data-range `allValuesFrom`
at all, so it reports the offending data consistent while HermiT finds the
clash.

That one divergence is expected, so it is PINNED in `suites/sulo.divergences`
rather than left to keep the job permanently red, because a permanently red job
gets muted and a muted alarm is a check that cannot fail. The pin records both
reasoners' answers and is diffed in both directions: a divergence the pin does
not describe is exit 5, and a divergence the pin DOES describe that no longer
occurs is exit 4, because rustdl gaining that capability is exactly the news the
job exists to deliver and must not be absorbed silently. Re-baselining is
`--accept-divergences`, never automatic, and the checked-in pin is itself diffed
against a table in `tests/divergences.rs`.

### Not yet done

Three of the five golden-closure components, which need a probe ABox since
`sulo.ttl` declares no individuals.

## Design

The harness must never overstate what it verified. rustdl is sound but not
complete, so "not entailed" is an absence of proof rather than a proof of
absence. There are therefore four verdicts, not two:

| Verdict | Meaning |
| --- | --- |
| `Pass` | Trustworthy, guaranteed by the reasoner's soundness |
| `UnrefutedPass` | A negative expectation the reasoner failed to refute. Does not fail the build, counted separately |
| `Indeterminate` | A timeout, or an axiom loss bearing on this query |
| `Fail` | Trustworthy failure |

Exit codes: `0` pass, `1` any Fail, `2` harness or configuration error, `3` any
Indeterminate, `4` golden drift or re-baseline required, `5` oracle divergence.

The full design, including the measured limitations of the pinned reasoner and
several claims this project made and later had to retract, is in
`docs/superpowers/specs/2026-08-21-sulo-testharness-design.md`.

## Running it

Everything expects a SULO checkout as a sibling directory:

```sh
git clone https://github.com/AIDAVA-DEV/sulo ../sulo
```

Run the suite:

```sh
cargo run -- run --suite suites/sulo --ontology ../sulo/sulo.ttl
```

`--format json|junit` for machine consumers, `--filter <substr>` to narrow to a
group or a single case.

Four ways a run could check nothing, or mislead about what it checked, are
configuration errors (exit 2) rather than a green run: a suite root with no
cases, a filter matching nothing, a selection every one of whose cases is
deferred, and two cases sharing an `id`.

`--deferred include|only` governs the cases tagged `oracle-hermit`, whose
oracle of record is the HermiT differential rather than the pinned reasoner.
By default they are named and counted but not run, and cannot set the exit
code; `only` runs exactly them under the pinned reasoner anyway. The
`differential` subcommand ignores the tag entirely and includes every case,
because a differential that skipped the cases it is the oracle for would leave
them checked by nothing.

`--allow-indeterminate` exits 0 rather than 3 when the run holds an
Indeterminate and no Fail (spec 5.4). It can never suppress a Fail, and the
Indeterminates stay in the report either way. An Indeterminate caused by axiom
loss means the reasoner saw a weaker ontology than the one that ships, so reach
for this only when a genuine timeout is blocking you.

Cross-check against HermiT. Needs a JVM and a ROBOT 1.9.7 jar, which is why it
is its own subcommand rather than a flag on `run`:

```sh
cargo run -- differential --suite suites/sulo --ontology ../sulo/sulo.ttl \
                          --robot robot.jar \
                          --divergences suites/sulo.divergences
```

With `--divergences`: exit 5 on a divergence the pin does not describe, 4 on a
pinned divergence that no longer occurs, 3 on any question neither reasoner
could be asked, 2 on a configuration error, and 0 only when every question was
put to both reasoners, every answer matched or was pinned, and every pin was
confirmed. Without it, any divergence at all is exit 5.

`--filter` narrows the run the same way `run`'s does, but cannot be combined
with `--divergences`: a pin is a claim about a whole suite, and a filtered run
never asks the questions outside the filter, so it can neither confirm nor
refute those entries. `--format json` is for a machine consumer, and `--workdir`
says where the probe ontologies are kept: a divergence is only actionable if the
reader can open the probe that produced it.

Re-baseline the pin deliberately, exactly as `--accept-golden` re-baselines the
closure. It is refused if the run left any question unanswered, since a broken
jar would otherwise write an empty pin and leave a permanently green job:

```sh
cargo run -- differential --suite suites/sulo --ontology ../sulo/sulo.ttl \
                          --robot robot.jar \
                          --divergences suites/sulo.divergences \
                          --accept-divergences
```

Compare the inferred closure against the committed golden file, and re-baseline
it deliberately after a legitimate change:

```sh
cargo run -- golden --ontology ../sulo/sulo.ttl --golden suites/sulo.golden
cargo run -- golden --ontology ../sulo/sulo.ttl --golden suites/sulo.golden --accept-golden
```

Run the harness's own tests, including the mutation self-test:

```sh
cargo test
```

### In someone else's CI

```yaml
- uses: MaastrichtU-IDS/sulo-testharness@v0.1.0
  with: { ontology: sulo.ttl }
```

The release attaches a static `linux-x86_64` binary, a `macos-aarch64` binary,
and the tag's own suite, so a consumer needs no Rust toolchain and always gets
the cases and the engine that were tested together. No release is cut yet.

## Dependency pinning

`horned-owl` must be declared as a crates.io dependency with a
`[patch.crates-io]` redirect to the git rev that rustdl pins. Naming the git rev
directly in `[dependencies]` does not unify with rustdl's own dependency, and
cargo builds two copies of the crate whose `SetOntology` types cannot
interoperate. See the comments in `Cargo.toml`.

## Licence

Dual licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
