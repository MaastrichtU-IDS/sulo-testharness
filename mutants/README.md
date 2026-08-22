# Mutants

Each `no-*.ttl` file is `clean.ttl` (see below) with exactly one
documented axiom removed or weakened. `tests/mutation.rs` asserts that
every mutant is caught by a specific named case, and that the same
case passes on clean SULO.

A mutant nothing catches is a coverage hole in the suite, reported as
such. These are the harness's own regression tests: they are what
distinguishes a suite that guards the ontology from a suite that is
merely green.

## `clean.ttl`: why it is not literally `../sulo/sulo.ttl`

Real SULO permanently carries three `SubClassOf` axioms using data
ranges that horned-owl's RDF reader parses fine but that rustdl's IR
conversion cannot represent and drops (reported as `dropped_axioms`,
kind `"SubClassOf: unsupported data range"`):

- `sulo:TimeInstant`'s restriction using `owl:unionOf (xsd:dateTime
  xsd:dateTimeStamp)`.
- `sulo:InformationObject`'s restriction using `owl:allValuesFrom
  rdfs:Literal` on `sulo:hasValue`.
- `sulo:Duration`'s restriction using an `xsd:decimal` facet
  (`owl:withRestrictions ( [ xsd:minInclusive 0.0 ] )`). Measured:
  this one is NOT actually dropped at the pinned rustdl tag (decimal
  is a recognised datatype bucket independent of the facet), so it is
  left untouched in `clean.ttl`; only the first two are removed.

This is a real, permanent, upstream reasoner-expressivity gap, not a
bug in this harness, and not something `src/load.rs` can recover the
way it recovers `owl:AllDisjointClasses` (see below): the axioms
reach rustdl intact, rustdl's own internal conversion is what cannot
represent them.

It matters here because `suite::downgrade_for_loss` is, correctly,
conservative: ANY reported loss anywhere in the ontology downgrades
every "no proof was found" `Fail` to `Indeterminate(AxiomLoss)`,
regardless of whether that loss has anything to do with the check at
hand (see `tests/suite.rs`,
`a_generous_timeout_ms_yields_a_real_verdict_not_a_timeout`, which
documents and relies on exactly this for a different, unrelated
claim). Real `sulo.ttl` always carries this loss, so a
positive-expectation case run against it can never resolve to a
trustworthy `Fail`, not because of anything a mutant does, but
because of these two permanently-dropped, semantically irrelevant
axioms. That would make every mutation test in this file fail for a
reason that has nothing to do with whether the mutation was caught.

`clean.ttl` is real `sulo.ttl` with exactly those two restrictions
(`TimeInstant`, `InformationObject`) replaced by the plain
`sulo:Time` (respectively `sulo:Feature`) disjunct they were conjoined
with, which changes nothing rustdl's reasoner ever saw: it already
discarded the removed restrictions outright. Confirmed by
measurement: `load_file("../sulo/sulo.ttl")` reports one loss entry,
`"conversion: 2 dropped (SubClassOf: unsupported data range x2)"`;
`load_file("mutants/clean.ttl")` reports zero loss. `Duration`'s facet
restriction is deliberately left untouched in `clean.ttl` (measured:
it is not one of the two dropped axioms, so removing it would only
make `clean.ttl` diverge from real SULO for no reason). Every mutant
below is generated from `clean.ttl`, not raw SULO, so both halves of
`assert_caught` (clean must Pass, mutant must Fail) are measuring the
axiom under test, not this unrelated gap.

## `owl:AllDisjointClasses` recovery

Separately, real SULO also carries two `owl:AllDisjointClasses`
axioms that horned-owl's RDF reader has no vocabulary entry for at
all (unlike `owl:AllDifferent`, which it handles). Before this task,
those landed silently in `IncompleteParse` and were reported as parse
loss. `src/load.rs::recover_all_disjoint_classes` now reconstructs
them from the `IncompleteParse` leftovers into proper
`DisjointClasses` axioms, so this is no longer loss for `sulo.ttl`
(see that function's doc comment for the matching strategy and its
one honest limitation: it cannot prove which member-list belongs to
which declaration from horned-owl's public API, so it recovers only
when the count of candidate declarations and candidate lists agree
exactly, which is true for `sulo.ttl` and `clean.ttl`, both before and
after every mutation below removes an unrelated axiom).

This recovery is why `no-feature-union.ttl` (below) is meaningfully
different from simply having no disjointness at all: the
`AllDisjointClasses` covering the same four classes is still present
and now actually reaches the reasoner, so only the covering case
reacts to that mutant, exactly as documented below.

## The mutants

| File | Edit | Case that must fail |
| --- | --- | --- |
| `no-role-chain.ttl` | removes `owl:propertyChainAxiom` on `hasParticipant` | `suites/proof/role-chain.yaml` |
| `no-transitive-ispartof.ttl` | removes `owl:TransitiveProperty` from `isPartOf`, keeps reflexivity | `suites/proof/transitivity-ispartof.yaml` (KNOWN COVERAGE HOLE, see below) |
| `no-feature-union.ttl` | removes `Feature`'s `disjointUnionOf`, keeps its `AllDisjointClasses` | `suites/proof/covering-feature.yaml` only |
| `no-subproperty-isin.ttl` | removes `isPartOf rdfs:subPropertyOf isIn` | `suites/proof/subproperty-isin.yaml` (KNOWN COVERAGE HOLE, see below) |

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

## KNOWN COVERAGE HOLE: `no-transitive-ispartof.ttl`

`suites/proof/transitivity-ispartof.yaml` does NOT catch this mutant.
Verified empirically: `run_case` resolves to `Pass`, not `Fail`, on
`no-transitive-ispartof.ttl`.

Diagnosis: `sulo:hasPart` is declared `owl:inverseOf sulo:isPartOf`
AND, independently, `owl:ReflexiveProperty, owl:TransitiveProperty`.
This mutant removes only `isPartOf`'s own `owl:TransitiveProperty`
declaration. But OWL DL entails that a property's inverse is
transitive whenever the property itself is, so `isPartOf`'s
transitivity is still fully entailed via `hasPart`, completely
independent of whether `isPartOf` carries its own copy of the axiom.
The axiom this mutant removes is redundant given the rest of the
ontology, as authored: no data pattern over `isPartOf` chains can
distinguish "declared transitive directly" from "transitive via
`hasPart`'s inverse", because those two axioms are logically coupled
by the `inverseOf` link for every chain, not just this test's.

This is not a defect in the test case that a differently-shaped
`entails:` claim could fix; it is the mutant's target axiom being
genuinely redundant in context. Catching it would require a mutant
that also breaks `hasPart`'s transitivity or the `inverseOf` link,
which is a different (and larger) edit than "remove one axiom", so it
is left as-is and reported rather than silently swapped for an edit
that would pass. `tests/mutation.rs`'s
`dropping_ispartof_transitivity_breaks_the_transitivity_case` fails
on purpose, documenting this hole rather than hiding it.

## KNOWN COVERAGE HOLE: `no-subproperty-isin.ttl`

`suites/proof/subproperty-isin.yaml` does NOT catch this mutant
either, for the same shape of reason. Verified empirically: `run_case`
resolves to `Pass`, not `Fail`.

Diagnosis: this mutant removes the direct `isPartOf rdfs:subPropertyOf
isIn` link. But `sulo:contains` is declared `owl:inverseOf sulo:isIn`,
and `sulo:hasPart rdfs:subPropertyOf sulo:contains`, and (as above)
`sulo:hasPart owl:inverseOf sulo:isPartOf`. So for the test data (`a
isPartOf b`, `b isPartOf c`), the direct route this mutant cuts
(`isPartOf` ⊑ `isIn`, then `isIn`'s transitivity) has a parallel route
through the inverse side that survives untouched: `a isPartOf b`
gives `b hasPart a` (inverse) gives `b contains a` (subproperty) gives
`a isIn b` (inverse again); symmetrically `b isIn c`; then `isIn`'s
own `owl:TransitiveProperty` (also untouched) closes `a isIn c`. Same
conclusion as the transitivity hole: SULO declares the
`{isPartOf, hasPart}` and `{isIn, contains}` pairs with enough
redundancy (each pair related by `inverseOf`, each side carrying its
own copy of the properties that matter) that several single-axiom
mutations targeting only one side are absorbed by the other.
`tests/mutation.rs`'s
`deleting_the_ispartof_isin_subproperty_axiom_breaks_the_isin_case`
fails on purpose for the same reason and is left that way.
