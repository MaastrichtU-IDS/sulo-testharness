# Plan: the HermiT differential (spec 5.3, phase 7)

Spec: `docs/superpowers/specs/2026-08-21-sulo-testharness-design.md`, sections
5.3, 5.4 and the note at the end of 9.

## Why

rustdl is sound but incomplete, so it cannot certify a non-entailment. Today the
harness is honest about that: a negative expectation it fails to refute is
`UnrefutedPass`, never `Pass`, and 26 checks on the real suite carry that label.
HermiT is complete for OWL 2 DL, so it is the oracle of record for exactly those
answers.

Two things follow. Disagreement between the reasoners is its own outcome,
`Divergence`, exit 5, which is currently unreachable from the binary. And the
one case rustdl provably cannot decide, `timeinstant-datarange`, is deferred and
therefore checked by nothing.

The JVM stays out of the default and local path. This is a CI job.

## What was measured before writing this

Everything below was established by prototyping against ROBOT 1.9.7 and
OpenJDK 21, not assumed. Two of the four results contradicted the obvious
approach.

1. **The two-step ROBOT invocation is mandatory.** Chaining
   `merge --input A --input B reason --reasoner hermit --output out.ttl`
   reports the data-range case CONSISTENT. Merging to a file first and then
   running `reason` on that file reports it INCONSISTENT, which is the correct
   answer. Deterministic: five runs each way, no variation. A differential built
   the chained way would be permanently, silently green.

2. **Exit code alone is not the answer; the message is.** `reason` exits 1 both
   when the ontology is inconsistent and when the invocation errors. Passing
   `--output /dev/null` is an "unknown format" error that exits 1 and looks
   exactly like a detected inconsistency. Classification must grep for
   `ontology is inconsistent` and treat any other non-zero exit as an ERROR,
   never as a verdict.

3. **HermiT decides the case rustdl cannot.** With the two-step invocation,
   `TimeInstant subClassOf (hasValue only (dateTime or dateTimeStamp))` plus a
   `TimeInstant` carrying an `xsd:string` value is INCONSISTENT, as the case has
   always claimed. rustdl reports it consistent because horned-owl drops that
   axiom as an unsupported data range.

4. **Non-entailment has a working encoding, verified in both directions.**
   To ask whether `O` entails `C subClassOf D`, merge `O` with a probe
   ontology defining `probe = C and not D` plus an individual `witness a
   probe`, then reason. INCONSISTENT means entailed; CONSISTENT means not
   entailed. All four non-subsumptions in `non-subsumptions.yaml` come back NOT
   entailed, and two controls that SULO really does assert come back ENTAILED.

## Non-goals

* Running HermiT locally or by default. CI only.
* Replacing the golden closure diff or the mutation suite.
* Retiring SULO's own `reasoning.yml`. Spec section 11 argues it stays
  permanently, and that argument is unaffected by this work.

## Rulings made in advance

1. **The differential is a separate subcommand, not a flag on `run`.** It needs
   a JVM and a `robot.jar` path, neither of which may leak into the default
   path. `differential --suite <dir> --ontology <ttl> --robot <jar>`.
2. **Divergence is reported per question, and the run exits 5 if any question
   diverges.** Exit 5 outranks nothing else here because a differential run
   makes no other claim: it is not asserting SULO is correct, only that the two
   reasoners agree.
3. **A question HermiT cannot answer is `Indeterminate`, never agreement.** A
   ROBOT error, a timeout, or an unparseable probe must never be silently
   counted as "the reasoners agree". This is the defect shape this project keeps
   finding, and the differential is the single easiest place to reintroduce it.
4. **The differential must prove it can detect disagreement before it is
   trusted.** It ships with a self-test that feeds it a question whose two
   answers are known to differ, and asserts `Divergence`. A differential that
   has never been seen to diverge is not evidence of agreement.
5. **Classification is by message, not exit code** (measured result 2).
6. **Every ROBOT call uses the two-step form** (measured result 1), and a test
   pins that, because the chained form is the one a future maintainer would
   naturally write.

## Task 1: the ROBOT driver

`src/hermit.rs`: spawn ROBOT, classify the result.

* `pub enum HermitAnswer { Consistent, Inconsistent, Error(String) }`
* `pub fn consistency(robot: &Path, ontology: &Path, extra: &[PathBuf], workdir: &Path) -> HermitAnswer`
  Two-step: `merge` every input to a temp file, then `reason --reasoner hermit`
  on that file. Classify by the `ontology is inconsistent` message.
* A deadline, so a pathological question cannot hang CI.

Tests, all requiring `robot.jar` and skipping cleanly when absent: consistent
ontology, a known clash, the data-range case, and an induced ROBOT error
(a nonexistent input) proving `Error` is distinguished from `Inconsistent`.

## Task 2: questions

`src/differential.rs`: turn a case into questions HermiT can answer.

* `expect_inconsistent` and the consistency gate map directly onto
  `consistency`.
* `not_entails` / `not_entails_manchester` map onto the probe encoding from
  measured result 4.
* `satisfiable_expr` maps onto a probe with a witness of the expression alone.

Each question carries: the case id, the check name, rustdl's answer, and enough
provenance to name the disagreement in a report.

Tests: each question shape builds the probe ontology it should, pinned as text,
so a silent change to the encoding fails.

## Task 3: comparison and the Divergence verdict

* `Agree`, `Divergence { rustdl, hermit }`, `Indeterminate { reason }`.
* A `Divergence` is reported with BOTH answers and the question, because the
  point is that one of the two reasoners is wrong and the reader must be able to
  tell which.
* Ruling 3: a ROBOT `Error` becomes `Indeterminate`, never `Agree`.

Tests: the three outcomes, and specifically that an `Error` does not become
agreement.

## Task 4: the `differential` subcommand and exit 5

Wire into `main.rs`. Exit 5 on any divergence, 3 on any indeterminate, 2 on
configuration error, 0 otherwise. `tests/cli.rs` currently asserts exit 5 is
UNREACHABLE and is written to break when this lands; that test must be replaced
with a real observation of divergence from the binary.

## Task 5: the self-test (ruling 4)

A fixture ontology and question where rustdl and HermiT are known to differ:
`timeinstant-datarange` is exactly that case today, since rustdl says consistent
and HermiT says inconsistent. Assert the differential reports `Divergence` on
it, and `Agree` on a case where both reasoners answer the same way.

This is the test that proves the differential can fail.

## Task 6: CI job

`.github/workflows/differential.yml`: download `robot.jar`, run the differential
over the suite, upload the report. Not part of the default `ci.yml`, and not
required for the harness's own tests to pass.

## Task 7: the deferred case comes back

With the differential in place, `timeinstant-datarange` is no longer "checked by
nothing". Update `DEFERRED_REASON`, `suites/sulo/restrictions/README.md`, and
the `tests/deferred.rs` pin to say the differential is where it is decided.

## Rulings made during execution

7. **The question set widens to FAILING POSITIVE assertions.** `oracle::verdict_for`
   tells the user, on a positive assertion that found no proof, "Incompleteness
   is a possible cause; the CI differential settles it". That `Fail` rests on
   absence of proof exactly as a negative `UnrefutedPass` does. Scoping the
   differential to negatives and the gate would have shipped that message as a
   falsehood. The message is right and the plan's scope was too narrow.

   This is also the most valuable direction: a positive `Fail` HermiT cannot
   prove either is a real SULO regression, while one HermiT CAN prove is a
   rustdl incompleteness bug. That second outcome is precisely the signal spec
   5.3 calls the most valuable either reasoner could produce.

8. **A run that asks no questions is a configuration error, never agreement.**
   A case carrying only positive assertions that all passed yields zero
   questions, so a filter or suite selecting only such cases would report a
   green differential having asked nothing. Same shape as the three guards
   `run` already has, and refused the same way (exit 2).

9. **CI must fail when the jar is missing, not skip.** The jar-gated tests skip
   cleanly when `SULO_ROBOT_JAR` is unset, which is right for a laptop and
   wrong for the differential job: a typo in the workflow step would produce a
   permanently green job that asserted nothing. The CI job sets a second
   variable that turns the skip itself into a failure.

10. **Probe terms live in a namespace nothing else can bind.** The prototype
    used `http://example.org/`, which is exactly what every suite fixture binds
    `ex:` to. A collision would not error; it would silently make the witness an
    individual the data already constrains, and the probe would answer a
    question nobody asked. `http://sulo-testharness.invalid/differential#`
    instead.

11. **An unsupported class-expression shape is `Unencodable`, never
    approximated.** Rendering an unknown shape as `owl:Thing` builds a probe
    that cannot clash, which is a question that cannot fail. It is reported as
    `Indeterminate` naming the variant.
