# Mutants

Each `no-*.ttl` file is real, unmodified `../sulo/sulo.ttl` with exactly
one axiom concept removed or weakened. `tests/mutation.rs` asserts
that every mutant is caught by a specific named case, and that the
same case passes on clean SULO.

A mutant nothing catches is a coverage hole in the suite, reported as
such. These are the harness's own regression tests: they are what
distinguishes a suite that guards the ontology from a suite that is
merely green.

## Why this file used to point at a doctored `clean.ttl`, and no longer does

Real SULO permanently carries two `SubClassOf` axioms using data
ranges that horned-owl's RDF reader parses fine but that rustdl's IR
conversion cannot represent and drops (reported by
`owl_dl_reasoner::dropped_axioms`, kind `"SubClassOf: unsupported data
range"`, count 2): `sulo:TimeInstant`'s restriction using
`owl:unionOf (xsd:dateTime xsd:dateTimeStamp)`, and
`sulo:InformationObject`'s restriction using `owl:allValuesFrom
rdfs:Literal` on `sulo:hasValue`. This is a real, permanent, upstream
reasoner-expressivity gap (the pinned rustdl's own comments describe
its data-range handling as "Phase 3 minimal ... Phase 7 full concrete
domains", i.e. not yet built), not a defect in SULO's axiomatisation.

Before this fix round, `suite::downgrade_for_loss` treated ANY
reported loss anywhere in the ontology as a reason to distrust a
positive-expectation `Fail`, converting it to
`Indeterminate(AxiomLoss)`, regardless of whether that loss had
anything to do with the check at hand. Because real `sulo.ttl` always
carried this loss, EVERY case run against it would eventually resolve
to `Indeterminate`, forever, independent of mutations. This was fixed
by not lowering a doctored copy of SULO, but by fixing `src/load.rs`
directly: it now recognises this exact, known, permanent shape as a
baseline (`KNOWN_BASELINE_KIND` = `"SubClassOf: unsupported data
range"`, `KNOWN_BASELINE_COUNT` = 2) and reports it through
`Loaded::baseline_loss` instead of `Loaded::loss`. `baseline_loss` is
still surfaced (a warning is printed on every load that matches it,
and it is inspectable on `Loaded`), but `downgrade_for_loss` is never
given it, so it can never downgrade a verdict. Any loss that does NOT
match this exact allowlist entry (different kind, different count, or
extra kinds alongside it) still lands in `Loaded::loss` and downgrades
exactly as before: the allowlist is explicit and auditable, not a
threshold, so a genuinely new or different drop stays just as loud as
it always was.

With that fixed at the source, `tests/mutation.rs`'s `CLEAN` points at
the real `../sulo/sulo.ttl`, and every mutant below is generated
directly from it. `mutants/clean.ttl` is gone.

## `owl:AllDisjointClasses` recovery

Separately, real SULO also carries two `owl:AllDisjointClasses`
axioms that horned-owl's RDF reader has no vocabulary entry for at
all (unlike `owl:AllDifferent`, which it handles). Before this task,
those landed silently in `IncompleteParse` and were reported as parse
loss. `src/load.rs::recover_all_disjoint_classes` reconstructs them
from the `IncompleteParse` leftovers into proper `DisjointClasses`
axioms, so this is no longer loss at all (not baseline, not beyond
baseline; simply gone) for `sulo.ttl` (see that function's doc comment
for the matching strategy and its one honest limitation: it cannot
prove which member-list belongs to which declaration from horned-owl's
public API, so it recovers only when the count of candidate
declarations and candidate lists agree exactly, which is true for
`sulo.ttl`, both before and after every mutation below removes an
unrelated axiom).

This recovery is why `no-feature-union.ttl` (below) is meaningfully
different from simply having no disjointness at all: the
`AllDisjointClasses` covering the same four classes is still present
and now actually reaches the reasoner, so only the covering case
reacts to that mutant, exactly as documented below.

## The mutants

| File | Edit | Case that must fail |
| --- | --- | --- |
| `no-role-chain.ttl` | removes `owl:propertyChainAxiom` on `hasParticipant` | `suites/proof/role-chain.yaml` |
| `no-transitive-parthood.ttl` | removes `owl:TransitiveProperty` from BOTH `isPartOf` and its inverse `hasPart`, keeps reflexivity on both | `suites/proof/transitivity-ispartof.yaml` |
| `no-feature-union.ttl` | removes `Feature`'s `disjointUnionOf`, keeps its `AllDisjointClasses` | `suites/proof/covering-feature.yaml` only |
| `no-subproperty-containment.ttl` | removes BOTH `isPartOf rdfs:subPropertyOf isIn` and its inverse-side counterpart `hasPart rdfs:subPropertyOf contains` | `suites/proof/subproperty-isin.yaml` |

`no-role-chain.ttl` is not a naive `grep -v` of the
`owl:propertyChainAxiom` line: that line is the last triple of the
`sulo:hasParticipant` statement, terminated with `.`; deleting only
that line leaves the preceding line's trailing `;` dangling into the
next statement and produces invalid Turtle (confirmed: horned-owl's
reader panics on it rather than returning a parse error). The mutant
instead rewrites the statement's tail from
`owl:inverseOf sulo:isParticipantIn ; owl:propertyChainAxiom (...) .`
to `owl:inverseOf sulo:isParticipantIn .`, removing the same one axiom
without breaking syntax.

Note on `no-feature-union.ttl`: the sibling disjointness counter-examples
must NOT react to it, because the redundant `AllDisjointClasses` axiom
still asserts pairwise disjointness (and, after the recovery above,
actually reaches the reasoner). An earlier version of this table
claimed otherwise, and was only ever right by accident, because
horned-owl drops `AllDisjointClasses` silently.

## Recorded finding: SULO's parthood/containment axioms are mutually redundant across the inverse pair

`no-transitive-parthood.ttl` and `no-subproperty-containment.ttl` each
started as a narrower, single-axiom mutant (`no-transitive-ispartof.ttl`,
removing `owl:TransitiveProperty` from `isPartOf` only;
`no-subproperty-isin.ttl`, removing `isPartOf rdfs:subPropertyOf isIn`
only). Both were semantically inert: verified empirically (`run_case`
resolved to `Pass`, not `Fail`, on both), not just argued.

Diagnosis: `sulo:hasPart` is declared `owl:inverseOf sulo:isPartOf`,
and independently carries its own `owl:ReflexiveProperty,
owl:TransitiveProperty`. OWL DL entails that a property's inverse is
transitive whenever the property itself is, so removing only
`isPartOf`'s own `owl:TransitiveProperty` removed nothing reachable:
`isPartOf`'s transitivity was still fully entailed via `hasPart`. The
same shape holds one level over: `sulo:contains` is `owl:inverseOf
sulo:isIn`, and `sulo:hasPart rdfs:subPropertyOf sulo:contains`, so
even with the direct `isPartOf rdfs:subPropertyOf isIn` link cut, `a
isPartOf b` still gives `b hasPart a` (inverse) gives `b contains a`
(subproperty) gives `a isIn b` (inverse again), and symmetrically for
`b`/`c`, closed by `isIn`'s own untouched transitivity.

Both corrected mutants now break both halves of their respective
inverse pair and are caught by their named cases (see
`tests/mutation.rs`). The underlying observation is recorded here
because it is a real property of SULO's current axiomatisation worth
carrying upstream: `{isPartOf, hasPart}` and `{isIn, contains}` are
each declared with enough redundancy (each pair related by
`inverseOf`, each side independently carrying the properties that
matter for parthood/containment reasoning) that a single-axiom edit
to only one side of either pair changes nothing entailment-wise. That
redundancy is not a bug in SULO, and arguably a reasonable defensive
choice, but it does mean a mutation-testing strategy that targets "one
axiom" must know to target both sides of these two pairs together.
