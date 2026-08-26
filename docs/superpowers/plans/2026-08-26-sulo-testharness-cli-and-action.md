# Plan: the `run` subcommand, machine-readable output, and the composite Action

Spec: `docs/superpowers/specs/2026-08-21-sulo-testharness-design.md`.
Covers spec phase 8, plus the precondition phase 8 assumes but no phase owns.

## Why this plan exists

Spec section 6 lists `report.rs # pretty stdout, --json, --junit` and
`action.yml # composite GitHub Action for consumer CI`, and section 11 shows the
consumer snippet. But `src/main.rs` has exactly one subcommand, `golden`.
`suite::run_case` and `report::render` are reachable from tests and nothing
else. There is no suite discovery at all.

Consequences today:

* Exit codes `1` (Fail), `3` (Indeterminate) and `5` (Divergence) are documented
  in spec 5.4, mapped by `verdict::exit_code`, and **unreachable from the
  binary**. `tests/verdict.rs` pins the mapping function, not the program.
* The 66-case suite runs only under `cargo test`, so the premise recorded in
  spec section 4 ("consumer CI needs no toolchain") does not hold.

This fell between the engine plan and the suite plan rather than being deferred
deliberately.

## Non-goals

* The HermiT differential (spec 5.3, phase 7). Exit code `5` therefore stays
  unreachable at the end of this plan, and Task 3 asserts that it is unreachable
  rather than pretending otherwise.
* Cutting a GitHub release or opening the consumer pull request against
  `AIDAVA-DEV/sulo`. Both are outward-facing and need the user's word. This plan
  builds and tests the workflow and the action, and stops there.

## Rulings made in advance

1. **Discovery walks a suite root recursively for `*.yaml`, sorted, and skips
   `data/` and `queries/` subdirectories.** Those hold fixtures, not cases. Sort
   so output is deterministic and diffable.
2. **A run that discovers zero cases is a configuration error (exit 2), never a
   green pass.** A suite root that silently matches nothing is the project's
   recurring defect shape at the program level.
3. **`--filter` that matches zero cases is likewise exit 2.** Same reason.
4. **A manifest that fails to load aborts the run with exit 2**, rather than
   being reported as a failing case. A malformed manifest is not evidence about
   the ontology, and `ManifestError` already means exit 2.
5. **`--format text|json|junit`, default `text`.** One flag, not two boolean
   flags, so the formats cannot be requested together.
6. **The JSON payload reports `rests_on_absence` and `baseline_loss`
   per case.** A machine consumer must be able to see the same honesty the text
   report carries; dropping them in the machine format would quietly restore the
   overstatement the whole design exists to prevent.
7. **JUnit maps the four verdicts as: `Pass` and `UnrefutedPass` to a passing
   testcase, `Fail` to `<failure>`, `Indeterminate` to `<skipped>`.**
   `Indeterminate` is not a failure and must not turn a consumer's build red on
   a reasoner timeout, but it must not read as a plain pass either.
   `UnrefutedPass` carries its distinction in the `name` and in a `<system-out>`
   line, since JUnit has no fifth state.

## Task 1: suite discovery

`src/suite.rs`: `pub fn discover(root: &Path) -> Result<Vec<PathBuf>, SuiteError>`.

* Recursive, `*.yaml` only, skipping any path component named `data` or
  `queries`.
* Sorted by path.
* `Err` when the root does not exist, is not a directory, or yields zero cases.

Tests: a fixture tree proving each of the four discovery rules, including that a
`data/*.yaml` is NOT discovered and that an empty root is an error. Prove
sortedness with a tree whose filesystem order differs from sorted order.

## Task 2: `run` subcommand and the run loop

`src/main.rs`: `run --suite <dir> [--ontology <ttl>] [--filter <substr>]
[--format text|json|junit]`.

Loop: discover, filter, `load_case` each (abort on error per ruling 4),
`run_case` each, aggregate every case's verdict with `verdict::aggregate`, render
in the requested format, exit with `verdict::exit_code`.

`src/report.rs` gains `render_json` and `render_junit` beside `render`.

Tests: the aggregation is over ALL cases, not the last one; a filter narrowing to
one case still exits on that case's verdict; JSON round-trips through
`serde_json::Value` with the fields ruling 6 requires; JUnit output parses as XML
and maps all four verdicts per ruling 7.

## Task 3: exit codes reachable from the binary, proven

`tests/cli.rs`, invoking `env!("CARGO_BIN_EXE_sulo-testharness")`.

| Exit | How this test reaches it |
| ---: | --- |
| 0 | the real suite against clean SULO |
| 1 | the real suite against a mutant that a case catches |
| 2 | a suite root with no cases; a filter matching nothing; a malformed manifest |
| 3 | a case whose check is `Indeterminate` |
| 4 | `golden` against a mutant that the closure catches (`no-feature-object`) |
| 5 | NOT reachable. Assert it is produced by no path, and state why. |

This is the task that matters most. The exit-code contract has been documented
and unit-tested since the engine plan while being unreachable from the program,
which is this project's recurring defect shape wearing a different hat. Each row
must be observed, not argued.

## Task 4: release workflow and `action.yml`

* `.github/workflows/release.yml`: on tag, build `linux-x86_64` and
  `macos-aarch64`, attach both to the release.
* `action.yml`: composite action taking `ontology`, `suite`, `format`, and a
  pinned `version`; downloads the binary for that tag and runs it.
* Do not cut a release and do not open the consumer pull request.

Tests: `action.yml` and `release.yml` parse as YAML and declare the inputs
section 11 uses. A workflow file that does not parse is the one failure mode
worth catching locally.

## Task 5: spec and docs reconciliation

* Spec section 11's consumer snippet says `AIDAVA-DEV/sulo-testharness`; the
  repository is `MaastrichtU-IDS/sulo-testharness`. Fix.
* Spec section 6's file tree lists `action.yml`, which will now exist. Confirm
  the rest of that tree matches reality and correct it where it does not.
* Add the CLI surface to the spec, since it was only ever implied.
* README: replace the "Not yet done" paragraph about the missing CLI with the
  real usage, and keep exit code `5` honestly marked unreachable until phase 7.

## Rulings made during execution

8. **The release attaches a third asset, `sulo-suite.tar.gz`, built from the
   tag's own `suites/` tree.** Task 4 as written did not work: the action
   downloads a binary, but the cases live in this repository, so spec section
   11's `with: { ontology: sulo.ttl }` would discover zero cases and hit ruling
   2's exit 2 on every consumer run. Bundling the suite from the same tag also
   guarantees the cases and the engine that runs them are the pair that were
   tested together.

9. **Ruling 7 governs the JUnit verdict mapping only, not the process exit
   code.** Spec 5.4 governs the exit code, and it says `3` on any Indeterminate.
   The composite action therefore fails the step on exit 3. An Indeterminate is
   a result the harness explicitly refuses to vouch for, and its dominant cause
   here is axiom loss rather than a flaky timeout: mapping it to success would
   turn the loss-downgrade machinery, which exists to stop an unearned green,
   into an unearned green.

10. **`--allow-indeterminate` gets implemented, because spec 5.4 already
    promises it.** With ruling 9 in force, a consumer hitting a genuine timeout
    otherwise has no supported way to proceed. The flag is added to `run` in
    Task 2 and surfaced as an action input in Task 4. It must never suppress a
    `Fail`: it lowers `3` to `0` only when no `Fail` is present, and the
    Indeterminates stay visible in the report either way.

11. **musl, not glibc, for `linux-x86_64`.** A glibc binary built on
    `ubuntu-latest` carries that image's glibc symbol versions and will not start
    on an older glibc, which includes `ubuntu-22.04` runners and most `container:`
    images. musl's allocator is slower under a tableau reasoner's allocation
    pattern, and that cost is accepted: slower is a cost, will-not-start is an
    outage. The release asserts staticness with `ldd` so a silent fall back to
    dynamic linking fails the release rather than shipping.

12. **`run` defers `oracle-hermit` cases by default, and this is not a new
    decision.** `run --suite suites/sulo` exits 1 on healthy SULO because
    `timeinstant-datarange` asserts a data-range `allValuesFrom` the pinned
    reasoner provably cannot enforce. Spec line 746 already says such cases
    carry `oracle: hermit` and run "only in the CI differential (5.3)", the case
    already carries the `oracle-hermit` tag, and `tests/restrictions.rs` already
    excludes it by a named `EXCLUDED` constant. Only the CLI failed to honour
    it. Deferred cases are named in the report and counted, never silently
    dropped, and are excluded from aggregation so they do not set the exit code.
    Reporting that case as `Fail` would itself be an overstatement: it says SULO
    regressed when in fact the reasoner cannot see the axiom, and the ontology
    logs baseline loss for that exact axiom on every load.

    Guard, because a tag that suppresses a case is a way to silence a failure:
    the set of deferred cases is pinned in a test and diffed BOTH ways against a
    live scan of the suite for the tag, so adding the tag to any further case is
    a visible, reviewed act.

13. **A `*.yml` file in the suite tree is refused, not skipped.** Discovery
    matches `*.yaml`; one stray `.yml` among 66 `.yaml` files would be ignored
    with no message, which is this project's recurring defect shape exactly.
    Refusing loudly (exit 2, "rename to .yaml") keeps one convention and makes
    the silent skip impossible. This follows the precedent set for the ambiguous
    `ordered`/`exact` combination: refuse rather than guess, and refuse rather
    than silently ignore.

14. **The JUnit/exit-code asymmetry is deliberate and stays.** JUnit maps
    `Indeterminate` to `<skipped>` while the process exits 3, so a consumer sees
    a failing job whose report shows skips rather than failures. That is the
    honest rendering in both channels: JUnit has no fifth state, and `<failure>`
    would claim the ontology regressed when the reasoner merely could not
    answer. The report carries a caveat line saying so.
