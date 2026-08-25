# suites/sulo/domains-ranges

Domain and range coverage for the 18 object properties plus
`hasValue`, driven from a bare property assertion between two untyped
skolemised individuals and checked as an entailed class assertion,
plus one counter-example proving a range axiom actually bites
(`range-violation-hasparticipant`).

## Six properties with no case: no domain or range axiom exists

`contains`, `isIn`, `isPartOf`, `hasPart`, `isDirectPartOf`, and
`hasDirectPart` carry NO `rdfs:domain` or `rdfs:range` triple anywhere
in `sulo.ttl`. Verified by reading the ontology directly, not assumed:
each of these six declarations lists only its property
characteristics (`owl:TransitiveProperty`, `owl:ReflexiveProperty`,
`rdfs:subPropertyOf`, `owl:inverseOf`) and nothing constraining who
may stand in the relation. `rdfs:domain`/`rdfs:range` is also not
something a `rdfs:subPropertyOf` axiom inherits downward by itself:
`isPartOf rdfs:subPropertyOf isIn` only licenses `x isPartOf y => x
isIn y`; since `isIn` itself has no domain or range, there is nothing
for that inferred triple to inherit. No case can be written for any of
these six without asserting something sulo.ttl does not, so none is
written. This mirrors `suites/sulo/restrictions/README.md`'s handling
of semantically inert axioms: a documented absence, not an oversight.

## `owl:Thing` sides are skipped as vacuous

Six more properties DO have a domain or range triple on one side, but
it is `owl:Thing`: `atTime`'s domain, `isTimeOf`'s range,
`isReferredToIn`'s domain, `refersTo`'s range, `hasItem`'s range, and
`isItemIn`'s domain. Every RDF term the reasoner can produce is
already `owl:Thing`, so asserting that side would be a permanent
tautology, provable against a declarations-only ontology, and no
mutation could ever flip it. Only the non-vacuous side of each of
these six properties gets a case.

## Redundant inverse pairs: a genuine mutation finding

Three property pairs carry the SAME domain/range shape mirrored across
an `owl:inverseOf` link: `atTime`/`isTimeOf` (range Time / domain
Time), `precedes`/`isPrecededBy` (domain+range Process both ways), and
`hasParticipant`/`isParticipantIn` (domain Process+range Object /
domain Object+range Process). For each pair, deleting ONLY one
member's own `rdfs:domain`/`rdfs:range` does not flip that member's
case: `ObjectPropertyDomain`/`ObjectPropertyRange` on one half of an
`InverseObjectProperties` pair is re-derivable from the other half's
domain/range plus the inverse axiom, by standard OWL 2 DL model
theory (not an RDFS-only inference, and not a reasoner quirk). This
was verified empirically during this task, capped at 60s per mutant
run; see the task-9 report for the specific mutant results. Both
axioms are still independently asserted in `sulo.ttl`, and both get
their own case, exactly as `properties/inverse-pairs` tests both
directions of every `owl:inverseOf` pair rather than assuming one
implies the other; the note lives here, and on each affected case,
so the redundancy reads as a finding, not a bug in the case.

## `instance_of_expr`'s `individual:` field takes a full `<IRI>`, not a CURIE

Discovered while writing `hasfeature.yaml` and `isfeatureof.yaml`.
Every other manifest field that names an entity (`prefixes`-driven
Turtle fragments, `entails_manchester`'s `sub_expr`/`sup_expr`,
`expect_rows`) resolves CURIEs against the shared prefix map (spec
7.2). `instance_of_expr`'s `individual:` does not: `suite::run_case`
passes it straight to `oracle::check_instance_expr`, which checks it
against the ontology's declared individuals with no prefix expansion
first. A CURIE there (`"ex:x"`) is checked verbatim against full IRIs
and never matches, so the case reports `Indeterminate("ex:x does not
appear as an individual in the ontology")` instead of running the
check at all. The manifest schema's own worked example in spec
section 7 writes `individual: "ex:encounter"` as a CURIE; as written,
that example would not run. `individual:` in this suite's data is
always written as a full `<IRI>` string to work around it.
