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

Complete and passing, 198 tests: the engine (manifest parsing, hermetic Turtle
loading with axiom-loss detection, typed claims, the reasoner oracle, Manchester
class-expression checks, a consistency gate, a golden inference-closure diff),
the competency-question path (SPARQL over a materialised inference closure), and
the SULO suite itself.

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

### Not yet done

The CLI ships only the `golden` subcommand. The 66-case suite runs under
`cargo test`, so a consumer needs this repository and a Rust toolchain rather
than a pinned binary. Spec section 6 calls for a `run` subcommand with `--json`
and `--junit` over `report::render`, plus an `action.yml`; none of the three
exist yet, and exit codes `1`, `3`, and `5` are therefore unreachable from the
binary today. That is the precondition for the composite GitHub Action and the
consumer workflow in `AIDAVA-DEV/sulo`.

Also outstanding: the HermiT differential (spec 5.3), and three of the five
golden-closure components, which need a probe ABox since `sulo.ttl` declares no
individuals.

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

The suite runs as tests, and expects a SULO checkout as a sibling directory:

```sh
git clone https://github.com/AIDAVA-DEV/sulo ../sulo
cargo test
```

Compare the inferred closure against the committed golden file:

```sh
cargo run -- golden --ontology ../sulo/sulo.ttl --golden suites/sulo.golden
```

Re-baseline it deliberately after a legitimate change:

```sh
cargo run -- golden --ontology ../sulo/sulo.ttl --golden suites/sulo.golden --accept-golden
```

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
