# Mutants

Each `no-*.ttl` file is real, unmodified `../sulo/sulo.ttl` with exactly
one axiom CONCEPT removed or weakened. "Concept", not "axiom": four of
the ten mutants remove a PAIR of axioms, because removing either one
alone is semantically inert. Three of those four are inverse pairs
(the untouched half plus the `owl:inverseOf` link fully re-derives the
conclusion): `no-transitive-parthood.ttl`,
`no-subproperty-containment.ttl`, and
`no-participant-domain-and-inverse-range.ttl`. The fourth,
`no-selfpart-feature-and-informationobject.ttl`, is a different cause,
subsumption-chain redundancy on a doubly-typed individual, not an
inverse pair. The other six mutants are single-axiom deletions,
each verified effective on its own. Every claim of inertness below is
an empirical trace (`run_case` observed to return `Pass`, not `Fail`,
on the narrower mutant), not an argument. `tests/mutation.rs` asserts
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
baseline and reports it through `Loaded::baseline_loss` instead of
`Loaded::loss`. `baseline_loss` is still surfaced (a warning is
printed once per process, and it is inspectable on `Loaded` and
`suite::CaseResult`), but `downgrade_for_loss` is never given it, so
it can never downgrade a verdict.

The allowlist has two independent parts, both required:
`KNOWN_BASELINE_KIND` (`"SubClassOf: unsupported data range"`) and
`KNOWN_BASELINE_COUNT` (2) describe the loss's aggregate SHAPE;
`has_known_baseline_axioms` confirms the two specific, named axioms
(`sulo:TimeInstant`'s `hasValue` range restriction and
`sulo:InformationObject`'s) are actually still present in the parsed
ontology. Shape alone was tried first and found insufficient (fix
round 1) and then fixed (fix round 2, after review) precisely because
shape does not imply identity: any loss that does not match the shape
(different kind, different count, or extra kinds alongside it) still
lands in `Loaded::loss` and downgrades exactly as before, an
ADDITIONAL drop stays exactly as loud as it always was, but a loss
that matches the shape by coincidence, or because a SULO revision
SUBSTITUTED one of the two named axioms for a different one while
keeping the aggregate count and kind unchanged, would have been
silently exempted by shape alone. The identity check closes both: an
unrelated file (any `ontology:`/`imports:`/`data:` load, not just
SULO) that happens to drop exactly two same-kind axioms is not
exempted, because neither of the two named axioms is present in it;
and a future SULO edit that keeps the count and kind the same but
changes WHICH axioms are dropped is not exempted either, because the
identity check looks for the specific two axioms, not just their
count.

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
public API, so it only recovers when the pairing is provably total,
which is true for `sulo.ttl`, both before and after every mutation
below removes an unrelated axiom).

Fix round 2 tightened that guard after review: it originally compared
the count of `AllDisjointClasses`-shaped declarations against the
count of CANDIDATE (all-IRI) leftover lists, which is not quite the
same as "every leftover list is accounted for". If some unrelated
leftover list contained a bnode (making it not a candidate) while
some other unrelated all-IRI list happened to match the declaration
count by coincidence, the mismatch would go undetected and an
unrelated list could be materialised into a `DisjointClasses` axiom
that was never in the source, the dangerous direction, since
fabricated disjointness can manufacture entailments and
inconsistencies the real ontology does not have. The guard now also
requires every leftover `bnode_seq` entry, not just the candidates, to
be accounted for.

This recovery is why `no-feature-union.ttl` (below) is meaningfully
different from simply having no disjointness at all: the
`AllDisjointClasses` covering the same four classes is still present
and now actually reaches the reasoner, so only the covering case
reacts to that mutant, exactly as documented below.

## The mutants

| File | Edit | Case that must fail |
| --- | --- | --- |
| `no-role-chain.ttl` | removes `owl:propertyChainAxiom` on `hasParticipant` | `suites/proof/role-chain.yaml`, `suites/sulo/patterns/pro/role-chain.yaml`, `suites/sulo/patterns/pro/pattern-membership.yaml`, `suites/sulo/patterns/pro/who-participated.yaml` |
| `no-transitive-parthood.ttl` | removes `owl:TransitiveProperty` from BOTH `isPartOf` and its inverse `hasPart`, keeps reflexivity on both | `suites/proof/transitivity-ispartof.yaml`, `suites/sulo/properties/transitivity-ispartof.yaml`, `suites/sulo/properties/transitivity-haspart.yaml` |
| `no-feature-union.ttl` | removes `Feature`'s `disjointUnionOf`, keeps its `AllDisjointClasses` | `suites/proof/covering-feature.yaml`, `suites/sulo/taxonomy/covering-feature.yaml`; the other three `suites/proof` cases must still pass on it |
| `no-subproperty-containment.ttl` | removes BOTH `isPartOf rdfs:subPropertyOf isIn` and its inverse-side counterpart `hasPart rdfs:subPropertyOf contains` | `suites/proof/subproperty-isin.yaml`, `suites/sulo/properties/subproperty-axioms.yaml` |
| `no-feature-object.ttl` | removes `Feature rdfs:subClassOf sulo:Object` (a single named-class axiom, not a blank-node restriction) | `suites/sulo/patterns/solid/typing-chain.yaml`, `suites/sulo/patterns/solid/value-quality-unit.yaml` |
| `no-selfpart-feature-and-informationobject.ttl` | removes BOTH `Feature rdfs:subClassOf (hasPart only Feature)` and `InformationObject rdfs:subClassOf (hasPart only InformationObject)` | `suites/sulo/patterns/solid/unit-forced-feature.yaml`, `suites/sulo/restrictions/hasPart-propagation-feature.yaml`, `suites/sulo/restrictions/hasPart-propagation-informationobject.yaml` |
| `no-selfpart-process.ttl` | removes `Process rdfs:subClassOf (hasPart only Process)` entirely (its only `rdfs:subClassOf` member) | `suites/sulo/restrictions/hasPart-propagation-process.yaml` |
| `no-quantity-unit-somevaluesfrom.ttl` | removes `Quantity rdfs:subClassOf (hasPart some Unit)` | `suites/sulo/restrictions/quantity-haspart-some-unit.yaml` |
| `no-participant-domain-and-inverse-range.ttl` | removes BOTH `hasParticipant rdfs:domain Process` and its inverse `isParticipantIn rdfs:range Process` | `suites/sulo/domains-ranges/hasparticipant.yaml`, `suites/sulo/domains-ranges/isparticipantin.yaml` |
| `no-object-process-disjoint.ttl` | removes `Object owl:disjointWith Process`, the only Object/Process disjointness axiom in SULO | `suites/sulo/taxonomy/disjoint-object-process.yaml` |

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

## `no-object-process-disjoint.ttl`: the first disjointness mutant

The taxonomy group holds 14 disjointness counter-examples, and until
this mutant they had ZERO committed mutant coverage. They were
flip-verified during design with scratch mutants that were deleted
before commit, so that verification was neither repeatable nor in CI,
and the one taxonomy mutant that did exist (`no-feature-union.ttl`)
asserts the opposite: that they must NOT react, because
`AllDisjointClasses` keeps pairwise disjointness alive after the
covering half is deleted. That left 21% of the suite, all one
structural shape, resting on a claim that existed only in prose.

Where the axiom lives, checked rather than assumed. Object/Process
disjointness in `sulo.ttl` is one triple, `sulo:Object owl:disjointWith
sulo:Process`, inside the `sulo:Object` statement. It is NOT covered by
either `owl:AllDisjointClasses` list (those hold `{Capability,
InformationObject, Quality, Role}` and `{Duration, TimeInstant,
TimeInterval}`), and no `owl:disjointUnionOf` mentions the pair, so
the `AllDisjointClasses` recovery described above does not apply here
and nothing else re-asserts it.

Whether one deletion suffices, checked empirically. There is a
plausible route by which it could have been inert: `sulo:Object` also
carries `rdfs:subClassOf [ owl:complementOf [ hasPart some Process ] ]`,
and `sulo:hasPart` is `owl:ReflexiveProperty`, so an individual typed
both `Object` and `Process` should have `x hasPart x` with `x` a
`Process`, contradicting the complement. Observed behaviour of the
pinned reasoner on the mutant says otherwise: `run_case` on
`suites/sulo/taxonomy/disjoint-object-process.yaml` (whose data is a
single individual typed both classes) returns

    Fail("expected inconsistent, but the reasoner found it consistent; ...")

so that route is not taken and the single deletion is effective. The
same probe run across all 22 taxonomy cases showed exactly one
reacting: every other case, including the other 13 disjointness
counter-examples, kept its clean verdict (`Pass`, or `UnrefutedPass`
for the three negative-expectation cases), confirming the edit did not
spill into an unrelated axiom.

This mutant proves the disjointness SHAPE is load-bearing and wired
into CI for one pair. It does not prove it for the other 13 pairs
individually; extending it is a matter of adding more mutants, not of
reading this note as broader than it is.

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

## Task 10: extending coverage to the taxonomy/properties/restrictions/domains-ranges/patterns groups

The four mutants above were built for the predecessor plan's `proof/`
group and, before this task, caught nothing outside it: every suite
group added since (taxonomy, properties, restrictions, domains-ranges,
patterns/pro, patterns/solid) had zero caught mutants. This task closes
that gap two ways: reusing three of the four existing mutants where
they already exercise the identical axiom a new-group case depends on
(see the table above; `no-role-chain.ttl`, `no-transitive-parthood.ttl`,
`no-subproperty-containment.ttl`, and `no-feature-union.ttl` each now
catch at least one case outside `proof/`, verified individually), and
adding five brand-new mutants for axiom shapes the predecessor plan
never touched: a single named-class `subClassOf`, a `hasPart
only self` restriction pair, a standalone `hasPart only self`
restriction, a `someValuesFrom` restriction, and a domain/inverse-range
pair. Every group now has at least one caught mutant.

`no-feature-object.ttl` corrects a diagnosis from Task 9's report,
which claimed `patterns/solid/typing-chain` needed a concept-level
(inverse-pair) mutant. It does not: `Feature rdfs:subClassOf
sulo:Object` is a single, non-redundant named-class axiom (distinct
from the two blank-node restrictions in the same
`rdfs:subClassOf` list), and deleting it alone is sufficient, verified.

`no-selfpart-feature-and-informationobject.ttl` needs both halves for
the same reason `no-transitive-parthood.ttl` and
`no-subproperty-containment.ttl` need both halves of their inverse
pairs, but for a different underlying cause: not inverse-pair
redundancy, but because the individual under test
(`unit-forced-feature`'s measurement) is entailed both `Feature` and
`InformationObject` at once via the typing chain, so either class's
restriction alone still propagates Feature-hood onto its `hasPart`
value. Verified during design (see `suites/sulo/patterns/solid/
unit-forced-feature.yaml`'s own description) and re-verified here.
The same mutant, not by design but discovered while verifying it,
also catches `restrictions/hasPart-propagation-feature` and
`restrictions/hasPart-propagation-informationobject` directly, since
those two cases test exactly these two restrictions on their own.

`no-participant-domain-and-inverse-range.ttl` is the domains-ranges
counterpart to the two redundant-inverse-pair mutants above:
`domains-ranges/README.md` documents, as a mutation finding from Task
9, that `ObjectPropertyDomain`/`ObjectPropertyRange` on one member of
an `owl:inverseOf` pair is re-derivable from the other member's
domain/range plus the inverse axiom (standard OWL 2 DL model theory).
A single-axiom deletion of `hasParticipant`'s own `rdfs:domain
sulo:Process` is therefore inert; both it and `isParticipantIn`'s
`rdfs:range sulo:Process` must go together. Verified to also catch
`domains-ranges/isparticipantin.yaml`, not just `hasparticipant.yaml`,
since both cases require the identical `?p a Process` fact, reached
from opposite directions.

Every `(mutant, case)` pair listed in the table above was verified
individually: Pass on clean SULO and Fail on the mutant. This task
found no new coverage hole: no mutant added here goes uncaught by the
case it names.

## Regenerating after a SULO bump

`./mutants/regenerate.sh` performs all ten edits above from the
repository root, reading `../sulo/sulo.ttl` and overwriting every file
in this directory except `README.md` and itself. Run it whenever the
sibling `sulo` repo advances.

Regenerating is not optional bookkeeping: `tests/mutation.rs`'s
`mutants_are_not_stale_against_current_sulo` independently re-derives,
in Rust, what each mutant file should contain from whatever
`../sulo/sulo.ttl` currently holds, and fails the build if a committed
mutant no longer matches. Without that check, a SULO edit could go
unreflected in these files indefinitely: `assert_caught`'s "clean"
half would read the new ontology while its "mutant" half kept reading
a frozen old one, and every `assert_caught` test could stay green
while proving nothing about the ontology actually shipping, exactly
the "green while testing nothing" failure mode this whole mechanism
exists to catch, one level up from where it was originally built to
catch it.
