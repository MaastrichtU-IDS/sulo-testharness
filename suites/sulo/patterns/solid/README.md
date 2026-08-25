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

## `hasPart`'s reflexive self-loop reaches the CQ store

`value-quality-unit.rq` filters out
`?unit = <http://example.org/measurement>`. Discovered while writing
this case, not predicted going in: `hasPart` is `owl:ReflexiveProperty`,
and `materialize` injects the self-loop `x hasPart x` for every named
individual (spec section 8 step 6). Without the filter, the query's
`?unit` binding also matches the measurement itself via that
self-loop, alongside the intended `ex:unit`, and an `exact: true`
`expect_rows` correctly fails on the extra row. Any competency
question phrased over `hasPart` (or `isPartOf`) needs the same
guard unless the reflexive self-loop is actually wanted in the
result.
