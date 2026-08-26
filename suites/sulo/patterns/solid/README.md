# suites/sulo/patterns/solid

`data/measurement.ttl` uses Figure 4's SOLID pattern (`hasValue` plus
`refersTo` plus `hasPart` on a `Quantity`), with one repair recorded
in spec 9.1: **the figure puts the unit and quality in blank nodes.**
rustdl's `inferred_object_property_values` (the source `materialize`
uses for the competency-question store, spec section 8 step 6) covers
named individuals only, so a blank-node value would be invisible to
every `cq:` query in this directory. This suite uses skolemised IRIs
(`ex:unit`, `ex:height-quality`) throughout instead. The namespace
repair from spec 9.1 (`http://w3id.org/sulo/` to
`https://w3id.org/sulo/`) applies here too, though Figure 4 itself
carries no syntax errors the way Figure 7 does.

`single-value.yaml` is the mutation target named in spec section 10:
dropping `owl:FunctionalProperty` from `hasValue` in a scratch copy of
`sulo.ttl` must flip it.

`hasPart`'s reflexive self-loop reaching the CQ store (and needing a
`FILTER` in `value-quality-unit.rq`, below) is documented once, in
`src/cq.rs`'s module doc, where a competency-question author will find
it before writing a query rather than after being surprised by one.

## `value-quality-unit.rq` now depends on the typing chain (fix round 1)

The query's first triple was `<http://example.org/measurement> a
sulo:Object`. This is deliberate, not decoration, added in fix round 1
after review found the original query's three property joins
(`hasValue`, `refersTo`, `hasPart`) were all directly asserted in the
data, so the CQ could not be refuted by any ontology mutation: it
tested `materialize`'s pass-through of asserted triples, a real and
worthwhile regression test, but not a test of the pattern's own claim.
`a sulo:Object` is entailed only through the named subsumption chain
`Quantity` to `InformationObject` to `Feature` to `Object`
(`typing-chain.yaml`), so this row now depends on that chain closing.
Verified: deleting `Feature rdfs:subClassOf sulo:Object` from a scratch
copy of `sulo.ttl` flips both `typing-chain.yaml` and this CQ.

## Mutation results, corrected in fix round 1

The original report filed both of these under "did not flip", with
the wrong cause in each case:

- **`typing-chain` DOES flip under a single-axiom mutant.** The
  fixture's `measurement` individual has exactly one derivation for
  `sulo:Object`: `Feature rdfs:subClassOf sulo:Object`. Deleting that
  one triple flips the case; no concept-level (multi-axiom) mutant is
  needed. (The original report reached for a concept-level mutant on
  `Quantity rdfs:subClassOf sulo:InformationObject` instead, which
  does not touch this axiom at all and so, correctly, did not flip
  anything; that was the wrong target, not evidence the case was
  unflippable.)
- **`unit-forced-feature` needs a genuine two-axiom mutant**, and nothing
  smaller: BOTH `Feature rdfs:subClassOf (hasPart only Feature)` AND
  `InformationObject rdfs:subClassOf (hasPart only InformationObject)`
  must be removed together, since either alone still propagates
  Feature-hood onto `ex:unit` from `ex:measurement` (which is both
  Feature and InformationObject via the typing chain). Same shape as
  the domain/range suite's inverse-pair concept mutants. Verified:
  removing either restriction alone leaves the case passing; removing
  both together flips it.

`measurement a sulo:InformationObject` is itself over-determined: it
has FOUR independent derivations in this fixture (the `Quantity`
subsumption chain, `hasValue`'s domain, `refersTo`'s domain, and
`isReferredToIn`'s range via `owl:inverseOf`), plus a fifth redundancy
that does not even need a fixture: `Feature owl:disjointUnionOf (...)`
re-derives `InformationObject rdfs:subClassOf Feature` on its own.
None of that affects `sulo:Object`, which is why `typing-chain` and
the CQ above both flip cleanly on the single `Feature rdfs:subClassOf
sulo:Object` deletion regardless.

## Adaptations beyond the errata (fix round 1)

Beyond the repairs spec 9.1 lists, this fixture also deviates from
Figure 4 in ways not previously recorded:

- **`ex:alice sulo:hasFeature ex:measurement` was missing, restored in
  fix round 1.** The paper's own prose describes the SOLID pattern as
  reusing four relations; an earlier version of this fixture carried
  only three (`hasValue`, `refersTo`, `hasPart`) and silently dropped
  this one. Restored in `data/measurement.ttl`; it does not affect any
  existing case (no case asserts anything about `ex:alice`).
- **The subject was changed from temperature to height** (a
  `"170"^^xsd:decimal` height measurement rather than the paper's
  temperature example). A modelling choice, not an error, but
  undocumented until now.
- **The quality individual is typed `sulo:Quality`, not the paper's
  PATO term.** This contradicts this suite's own stated method for the
  unit (`obo:UO_0000027`, left external and untyped by any SULO class,
  "exactly as the paper does") applied inconsistently to the quality.
  Left as `sulo:Quality` rather than guessed at with an unverified PATO
  id, since no case here depends on the quality's specific type and a
  wrong id would be a worse error than an honestly documented
  inconsistency; restoring the paper's exact PATO term is left to
  whoever has the paper text in front of them.
