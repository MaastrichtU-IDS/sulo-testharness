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

## `instance_of_expr`'s `individual:` field: FIXED in `src/suite.rs`

Discovered while writing `hasfeature.yaml` and `isfeatureof.yaml`,
and fixed rather than worked around, per fix round 1 review:
`instance_of_expr`'s `individual:` now resolves through the same
prefix map every other entity-naming field uses (spec 7.2), via
`prefixes::expand` in `suite::run_case`, matching how the
`unsatisfiable` field is already handled. Before the fix,
`suite::run_case` passed `individual:` straight to
`oracle::check_instance_expr` with no prefix expansion, so a CURIE
there (`"ex:x"`) was checked verbatim against full IRIs and never
matched, and the manifest schema's own worked example in spec section
7 (`individual: "ex:encounter"`) would not have run as written.
`individual:` in this suite's data is a CURIE, like every other
entity-naming field, now that it resolves correctly.
