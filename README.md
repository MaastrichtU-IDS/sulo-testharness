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

The **engine** is complete: manifest parsing, hermetic Turtle loading with
axiom-loss detection, typed claims, the reasoner oracle, Manchester
class-expression checks, a consistency gate, a mutation self-test, and a golden
inference-closure diff. 111 tests.

The **SULO suite itself is not written yet**. Today the repository contains four
proof cases, which exist to prove the mutation suite bites, not to provide
coverage. See `docs/superpowers/plans/2026-08-23-sulo-testharness-cq-and-suite.md`
for the plan that adds the competency-question path and the roughly 70-case
suite.

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

Tests expect a SULO checkout as a sibling directory:

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
