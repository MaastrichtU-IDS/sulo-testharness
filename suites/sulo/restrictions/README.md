# suites/sulo/restrictions

Covers 12 of the 16 class-expression restriction axioms in `sulo.ttl`
(rdfs:subClassOf axioms whose filler is a blank-node restriction, as
opposed to the 15 named-class subClassOf axioms covered under
`suites/sulo/taxonomy`):

- 5 `hasPart` propagation cases, one per `C rdfs:subClassOf (hasPart
  only C)`, for `Object`, `Process`, `SpatialObject`, `Feature`, and
  `InformationObject`.
- 5 object `someValuesFrom` cases, as `entails_manchester`:
  `Quantity subClassOf (hasPart some Unit)`, `Feature subClassOf
  (isFeatureOf some (Object or Process))`, and `TimeInterval`'s three
  (`hasDirectPart some StartTime`, `hasDirectPart some EndTime`,
  `hasPart some Duration`). `TimeInterval`'s fourth, `hasPart some
  Unit`, has no case; see the fourth entry below.
- `duration-nonnegative`, an `expect_inconsistent: true` counter-
  example for the `Duration` non-negative `xsd:decimal` facet.
- `timeinstant-datarange`, tagged `oracle-hermit` and excluded from
  `tests/restrictions.rs`'s enforced `EXPECTED` table; see below.

## The four axioms with no case

Four of the 16 restriction axioms are semantically inert: no data
fixture can make them bite, because each is already entailed by other
axioms already present in `sulo.ttl`, or is a tautology independent of
`sulo.ttl` altogether. Recorded here so their absence reads as a
decision, not an oversight; a test cannot fail on a tautology, and
writing one that pretends otherwise would be exactly the kind of
false confidence this project keeps finding and removing.

1. **`Collection rdfs:subClassOf (hasItem only owl:Thing)`.** Every
   RDF term the reasoner can produce as a filler is an `owl:Thing`;
   there is no possible value of `hasItem` that could violate this.
   The restriction restates the top of the class hierarchy under a
   different name.

2. **`InformationObject rdfs:subClassOf (hasValue only rdfs:Literal)`.**
   `hasValue` is declared `owl:DatatypeProperty`, and every value of a
   datatype property is, by construction, a literal. No object-typed
   filler is even syntactically possible for `hasValue`, so this
   restriction cannot be violated either.

3. **`Object rdfs:subClassOf (not (hasPart some Process))`.** This one
   is not a syntactic tautology but a semantic one: `Object
   rdfs:subClassOf (hasPart only Object)` already forces every part of
   an `Object` to be an `Object`, and `Object owl:disjointWith
   Process` already forbids anything from being both. Together they
   already entail that no `Object` can have a `Process` part, which is
   exactly what this axiom states directly. A case built to exercise
   it would in fact be re-testing `hasPart-propagation-object` plus
   `disjoint-object-process` (`suites/sulo/taxonomy`) under a
   different name, not testing anything this axiom alone contributes.

4. **`TimeInterval rdfs:subClassOf (hasPart some Unit)`.** Discovered
   by mutation testing, not predicted going in: removing only this
   axiom from a scratch copy of `sulo.ttl` does not flip
   `quantity-haspart-some-unit`-shaped verification, because
   `TimeInterval rdfs:subClassOf Time`, `Time rdfs:subClassOf
   Quantity`, and `Quantity rdfs:subClassOf (hasPart some Unit)`
   (asserted separately, and load-bearing on its own, per
   `quantity-haspart-some-unit`) already entail it. Same shape as
   entry 3 above, and ruled the same way for consistency: a case built
   to exercise it would be re-testing `quantity-haspart-some-unit`
   under a different name, not testing anything this axiom alone
   contributes, so it gets no case. If `Quantity`'s own `hasPart some
   Unit` restriction is ever removed from `sulo.ttl`, this axiom
   becomes load-bearing for `TimeInterval` and the case should come
   back.

## `timeinstant-datarange` and the HermiT gap

`TimeInstant rdfs:subClassOf (hasValue only (xsd:dateTime or
xsd:dateTimeStamp))` is a real, non-tautological restriction: giving a
`TimeInstant` an `xsd:string` value ought to be inconsistent. The
pinned reasoner (rustdl v0.4.22) cannot enforce a data-range
`allValuesFrom` at all; loading `sulo.ttl` already logs this as a
known baseline loss ("unsupported data range x2"), covering exactly
two axioms: this one and `InformationObject`'s `allValuesFrom
rdfs:Literal` restriction on `hasValue` (see `load::KNOWN_BASELINE_KIND`
and the two named axioms `load::has_known_baseline_axioms` checks
for). `Duration`'s non-negative facet is a DIFFERENT construct
(`someValuesFrom` a facet-restricted `xsd:decimal`, not an
`allValuesFrom` data-range union or class) and is NOT part of that
dropped pair: it converts and is enforced correctly, which is exactly
why `duration-nonnegative` gets a trustworthy `Pass` and needed no
exclusion. The result for `timeinstant-datarange`:
`expect_inconsistent: true` is the true expectation, but the case
currently gets `Fail` from `run_case`'s consistency gate, because the
reasoner reports the ontology consistent instead.

The case is written anyway, and tagged `oracle-hermit`, because
writing it and marking it honestly is the point: an absent case reads
as "nothing to test here", while a present, correctly-failing case
reads as "the reasoner cannot see this yet". `tests/restrictions.rs`
excludes it by name from the `EXPECTED` table it enforces, so it does
not fail CI over a known, already-documented reasoner gap. It is
meant to be picked back up once a HermiT differential lands
(referenced elsewhere in this project's plan as the mechanism for
catching exactly this class of soundness/completeness gap); at that
point this case should move into the enforced table as a genuine
`Pass`.
