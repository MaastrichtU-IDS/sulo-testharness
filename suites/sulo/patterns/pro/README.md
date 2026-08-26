# suites/sulo/patterns/pro

`data/encounter.ttl` adapts the SULO paper's Figure 7 (the PRO
pattern: `hasParticipant o isFeatureOf -> hasParticipant`). It is NOT
a verbatim transcription; the paper's own listing does not work as
printed. Every repair is also recorded in spec 9.1 and, per-fact, in
`data/encounter.ttl`'s own header comment:

1. **Namespace.** The figure writes `http://w3id.org/sulo/`; the real
   namespace is `https://w3id.org/sulo/`.
2. **Not valid Turtle.** The `@prefix obo:` line has no terminating
   dot, a stray `.` after the `:encounter` type assertion orphans the
   following `sulo:hasParticipant` line, and `taxon:` is used but
   never declared (it appears only in the paper's own data lines,
   undeclared; the paper's inference block contains no taxon term at
   all).
3. **Wrong subject named in the paper's stated inference.** The paper
   says the inference gives `:visit_1 sulo:hasParticipant :alice,
   :drsmith`, but the data around it defines `:encounter`, not
   `:visit_1`. This suite uses `:encounter` throughout, matching the
   data.
4. **Roles never typed `sulo:Role`, needed for Figure 5's pattern
   class expression.** The paper types its two role individuals only
   as OMRSE classes (`OMRSE_00000011`, `OMRSE_00000012`, both kept in
   this fixture alongside the added typing, not replaced by it). OMRSE
   is not imported into `sulo.ttl`, so nothing makes either individual
   a `sulo:Role` on its own. **Corrected in fix round 1:** this repair
   is NOT about `hasParticipant`'s `propertyChainAxiom`, which carries
   no class conditions on any position and fires on the paper's data
   exactly as printed (Figure 6). It is about Figure 5's pattern class
   expression (`Role and isFeatureOf some Object`), which needs the
   `sulo:Role` typing to be satisfiable at all. An earlier version of
   this README attributed the repair to the wrong figure; see
   `pattern-membership.yaml` for where the distinction actually
   matters, and spec 9.1 (corrected in commit `ee2042d`) for the same
   fix applied there.

`role-chain.yaml` is the mutation target named in spec section 10:
deleting `hasParticipant`'s `owl:propertyChainAxiom` from a scratch
copy of `sulo.ttl` must flip it. Verified.

## `pattern-membership.yaml`, rewritten in fix round 1

The original version typed `alice` and `drsmith` `sulo:Object`
directly. Review found this made every conjunct of `Process and
hasParticipant some (Role and isFeatureOf some Object)` a directly
asserted triple, so the case held against an EMPTY TBox: it tested
nothing about `sulo.ttl` at all, only that the data file parsed. Fixed
by restoring the paper's own typing, `alice`/`drsmith` as
`sulo:SpatialObject` (never `sulo:Object` directly), so `isFeatureOf
some Object` now requires `sulo.ttl`'s own `SpatialObject
rdfs:subClassOf Object` axiom to fire.

That alone fixes the empty-TBox vacuousness, but review also asked for
genuine dependence on `hasParticipant`'s `propertyChainAxiom`
specifically, which needs the OUTER `hasParticipant` restriction to
target the chain-derived participant (`alice`/`drsmith`), not the
directly-asserted one (the `Role` individual). Tried and empirically
confirmed NOT to work with the pinned reasoner:
`check_instance_expr`'s someValuesFrom-over-a-nominal probe
(`oracle::entailed_via_satisfiability_probe`) cannot combine a
`propertyChainAxiom`-derived property assertion with ANY nested
restriction, in every shape tried (`hasParticipant some
(SpatialObject and ...)`, `hasParticipant some SpatialObject` alone,
`hasParticipant value ex:alice`), even though the exact same
chain-derived fact is directly provable via `entails:`
(`role-chain.yaml` already does, successfully). This is a genuine
reasoner-technique gap, not a authoring mistake, worth recording
alongside `claim.rs`'s existing "Note on PropertyAssertion" and "Note
on ClassAssertion": `check_instance_expr`'s nominal-based reduction and
`oracle::check`'s `instances`/`instances-expr`-based entailment path
disagree on this shape, and only the latter can see through a property
chain.

So `pattern-membership.yaml` now checks two things together, in one
case, deliberately split across check kinds:

- `entails:` proves `encounter hasParticipant alice, drsmith` (the
  same chain-derived fact `role-chain.yaml` proves), which makes the
  case's overall verdict depend on the `propertyChainAxiom`.
- `instance_of_expr` proves each `Role` individual satisfies Figure
  5's pattern expression, `Role and isFeatureOf some Object`, which
  now requires genuine TBox inference (`SpatialObject rdfs:subClassOf
  Object`) rather than an asserted triple.

Verified both properties directly: the case flips (`Pass` to `Fail`)
when `hasParticipant`'s `propertyChainAxiom` is deleted, and it FAILS
(not `Pass`) when run against a bare, axiom-free TBox carrying only
declarations, confirming it is no longer vacuous either way.

## Adaptations beyond the errata (fix round 1)

Beyond the repairs listed above, this fixture deviates from Figure 7
in ways not previously recorded. The free ones (cost nothing, remove
no coverage) are restored; the rest are documented:

- **`ex:alice`/`ex:drsmith` retyped from `SpatialObject` to `Object`.**
  Load-bearing, not cosmetic: see "`pattern-membership.yaml`, rewritten"
  above. Now restored to `SpatialObject`, matching the paper.
- **`ex:encounter a obo:OGMS_0000097` was dropped, restored.** Additive
  alongside `sulo:Process`; costs nothing, and no case depends on its
  absence.
- **The roles' OMRSE typings were REMOVED rather than augmented**,
  even though point 4 above (before this fix round) read as additive
  ("types both role individuals `sulo:Role` explicitly"). Now genuinely
  additive: both `obo:OMRSE_00000011` and `obo:OMRSE_00000012` are kept
  alongside `sulo:Role`.
- **The role individuals were renamed** (`ex:patient-role`,
  `ex:doctor-role`) rather than kept under whatever identifiers the
  paper's own Figure 7 uses. Not restored: the paper's exact
  identifiers were not available while writing this fixture. Left
  documented rather than guessed at.
- **`obo:NCBITaxon_9606` does not, in fact, match "the paper's own
  inference example".** An earlier version of this README claimed it
  did; that was wrong. The paper's inference block contains no taxon
  term at all; `taxon:` appears only in the paper's DATA lines,
  undeclared (point 2 above). `obo:NCBITaxon_9606` is this suite's own
  choice of a real, resolvable OBO term for "human", not something
  carried over from the paper's inference.
