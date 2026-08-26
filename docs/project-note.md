# A regression test harness for SULO

*A short note. August 2026.*

## The problem

SULO's continuous integration checked two things: that `sulo.ttl` parses, and
that the ontology is consistent as a whole. Both are necessary. Neither is
sufficient.

An edit that deletes a subsumption, neuters a disjointness axiom, or breaks the
Process-Role-Object role chain leaves an ontology that still parses and still
comes back consistent. CI stays green. The regression ships.

This is not hypothetical. Removing the single axiom
`sulo:Object owl:disjointWith sulo:Process` from `sulo.ttl` changes nothing that
the old checks could see. The ontology is still well formed and still
consistent, because nothing in it now says those two classes cannot overlap.

## What the harness does

It asserts SULO's logical properties one at a time, so that losing any one of
them fails the build.

**66 cases across six groups:** taxonomy (22), properties (9), restrictions
(12), domains and ranges (14), and the paper's two design patterns, PRO (4) and
SOLID (5). Each case is a YAML manifest naming an expectation, with Turtle
fixtures and, for competency questions, a SPARQL query beside it.

The cases cover asserted and inferred subsumptions, deliberate
non-subsumptions, 14 disjointness counter-examples, covering axioms,
satisfiability, inverse pairs, transitivity and its absence, reflexivity,
functionality, `hasPart` propagation, existential restrictions, every object
property's domain and range, and both design patterns including two competency
questions run as real SPARQL over a materialised inference closure.

## The commitment: never overstate what was verified

The harness reasons with rustdl, which is sound but not complete. If it finds a
proof, the proof is real. If it does not find one, that may mean no proof
exists, or merely that the reasoner could not get there. "Not entailed" is an
absence of proof, not a proof of absence.

A two-state pass/fail harness has to lie about that distinction. So there are
four verdicts:

| Verdict | Meaning |
| --- | --- |
| `Pass` | Trustworthy, guaranteed by the reasoner's soundness |
| `UnrefutedPass` | A negative expectation the reasoner failed to refute. Does not fail the build, and is never reported as a pass |
| `Indeterminate` | A timeout, or an axiom the reasoner could not represent bearing on this query |
| `Fail` | Trustworthy failure |

On the current suite, 26 checks carry `UnrefutedPass`. Calling them passes would
be the single easiest way to make the harness dishonest.

The same care applies elsewhere. When an axiom cannot be converted for the
reasoner, every verdict resting on "no proof found" is downgraded to
`Indeterminate`, because reasoning over a subset of the axioms says nothing
about the whole. One case asserts a data range the pinned reasoner provably
cannot enforce; rather than reporting a failure that would blame SULO for a
reasoner limitation, it is named, counted, and deferred to a complete reasoner.

## Why a green suite is not evidence

A test suite that cannot fail is worse than no suite, because it produces
confident green.

So the harness carries 10 mutants, each a single documented edit to `sulo.ttl`.
Every mutation test requires both directions: the case must pass on clean SULO
**and** fail on the mutant. All six groups are covered, and both competency
questions are mutation proven.

The mutants are re-derived in Rust from a live read of `sulo.ttl` on every run
and compared byte for byte, so a SULO change the mutants do not reflect is a
build failure rather than a suite quietly testing a frozen ontology.

This discipline found real problems in the suite itself. Several cases that
looked like tests turned out to assert triples already present in their own
fixtures, or to hold against an empty ontology. Each was discovered by breaking
SULO on purpose and noticing that nothing went red.

## What it found in SULO

Building the suite required deciding, for every axiom, whether a missing test
was a gap or a property of the ontology. That produced findings worth carrying
back:

- Both `owl:AllDisjointClasses` axioms are entailed by the `owl:disjointUnionOf`
  axioms beside them. They also have no vocabulary entry in horned-owl, so some
  tools drop them silently.
- Four class-expression restrictions cannot be violated by any data. Two are
  tautologies; two are already entailed by other axioms. One of those was found
  by mutation rather than predicted.
- The parthood and containment pairs are mutually redundant across
  `owl:inverseOf`, so an edit to one side of either pair changes nothing. Not a
  defect, but it means any mutation-based validation must target both sides
  together.

Reported as [AIDAVA-DEV/sulo#5](https://github.com/AIDAVA-DEV/sulo/issues/5).

Transcribing the FOUST 2025 paper's examples into test cases also showed that
neither Turtle listing parsed as published, and that the PRO pattern in Figure 5
does not apply to Figure 7's data because the role individuals are never typed
`sulo:Role`. Those corrections are now in the manuscript.

## Status

Released as `v0.1.0`. Written in Rust, with no JVM on the default path. A
composite GitHub Action downloads a prebuilt static binary and the suite that
was tested with it, so a consumer needs no toolchain:

```yaml
- uses: MaastrichtU-IDS/sulo-testharness@v0.1.0
  with: { ontology: sulo.ttl }
```

## The second reasoner

The 26 unrefuted checks are the honest limit of one incomplete reasoner. Getting
past them needs a second one, so the harness now runs a HermiT differential.

HermiT is complete for OWL 2 DL, which makes it the oracle of record for exactly
the answers soundness cannot vouch for. Every question the harness answers out
of an ABSENCE of proof is put to it: the consistency gate in both directions,
the negative assertions and satisfiability checks whose verdict is
`UnrefutedPass`, and, less obviously, the positive assertions that FAILED. A
failing positive rests on absence of proof exactly as an unrefuted negative
does, so scoping the differential to the negatives would have left the harness
telling users that a `Fail` was settled elsewhere when nothing was settling it.

Everything reduces to a consistency question, because that is the one primitive
the two reasoners share. To ask whether SULO entails `C rdfs:subClassOf D`, the
harness merges SULO with a small probe ontology defining `C and not D` and an
individual of that class, and asks HermiT whether the result has a model. No
model means the entailment holds. Each encoding was verified in both directions
against real SULO before it was trusted, because the way a probe fails is not an
error: a misparsed probe comes back consistent, consistent reads as "not
entailed", and "not entailed" is what the first reasoner already said. A
differential that only ever confirmed the reassuring direction would agree with
itself forever while proving nothing.

Disagreement is its own outcome and is always loud. It does not mean SULO
regressed; it means one of the two reasoners is wrong about a specific query,
and the report names which one and in which direction. The reasoner the harness
runs by default is sound but incomplete, so it is always the outlier: it either
missed a proof HermiT found, which is an incompleteness, or claimed one HermiT
refutes, which would be an unsoundness and is the alarming direction.

This runs in CI only, weekly and on demand. It needs a JVM and a ROBOT jar, and
neither belongs on the path a contributor uses to check an edit.

One real disagreement exists today and is recorded rather than silenced. SULO
says a `sulo:TimeInstant` may only carry an `xsd:dateTime` or
`xsd:dateTimeStamp` value; the pinned reasoner's loader drops that axiom as an
unsupported data range, so it reports a `TimeInstant` with a string value
consistent, while HermiT finds the clash. The case is checked in with both
answers beside it, so a reader sees what the documented disagreement IS rather
than only that one exists. The two reasoners agree about every other question
the suite asks, so the job is green whenever the world matches the documented
state, which is what keeps it from becoming an alarm nobody reads.

The pin is diffed in both directions. A disagreement nobody has reviewed fails
the job, and so does a pinned disagreement that has STOPPED happening: the gap
closing means the reasoner gained a capability, or SULO changed, or the case
moved, and that is news the harness exists to deliver rather than to absorb.
Re-baselining is a deliberate act, and the checked-in pin is itself diffed
against a table in the ordinary test suite, so it cannot be quietly regenerated
and committed.

## What is still open

Two things, both stated rather than left to be discovered.

The golden closure, which diffs SULO's inferred entailments against a
checked-in baseline, is a real defence but a narrower one than its name
suggests. Measured against the ten mutants, it catches two. Its sensitivity is
exactly named class subsumption, satisfiability and equivalence, plus the named
property hierarchy; it is structurally blind to property characteristics,
property chains, domains and ranges, disjointness, covering axioms, and every
ABox-level entailment. The number is written down in the code beside the
measurement that produced it, and is to be re-measured when a mutant is added,
not reasoned about. Widening that surface is the main piece of unfinished work.

The consistency gate every case runs is unbounded. The pinned reasoner exposes
no deadline-bearing consistency check, so the gate cannot honour a case's
timeout and a pathological ontology would block the suite. Expressing the gate
as a bounded satisfiability probe was tried and rejected: it agrees on every
fixture here, but it skips the ABox pre-checks, and trading an unbounded gate
for one that might MISS an inconsistency is the worse deal, since a missed
inconsistency makes every check below it pass vacuously. It waits on rustdl
[#74](https://github.com/MaastrichtU-IDS/rustdl/issues/74).

## Links

- Harness: <https://github.com/MaastrichtU-IDS/sulo-testharness>
- Ontology: <https://github.com/AIDAVA-DEV/sulo>
- Manuscript: <https://github.com/MaastrichtU-IDS/sulo-foust2025-manuscript>
