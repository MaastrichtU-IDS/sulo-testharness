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
   never declared.
3. **Wrong subject named in the paper's stated inference.** The paper
   says the inference gives `:visit_1 sulo:hasParticipant :alice,
   :drsmith`, but the data around it defines `:encounter`, not
   `:visit_1`. This suite uses `:encounter` throughout, matching the
   data.
4. **Roles never typed `sulo:Role`, so the chain cannot fire as
   printed.** The paper types its two role individuals only as OMRSE
   classes (`OMRSE_00000011`, `OMRSE_00000012`). OMRSE is not imported
   into `sulo.ttl`, so nothing makes either individual a `sulo:Role`,
   and `hasParticipant`'s `propertyChainAxiom` requires the middle
   term of the chain (via `isFeatureOf`) to originate from something
   already known to be `sulo:Role` for the pattern's own
   `instance_of_expr` check to hold; more basically, without
   `sulo:Role` typing the paper's example does not actually
   demonstrate the SULO-native chain at all. This suite types both
   role individuals `sulo:Role` explicitly. This is a substantive
   repair, not a typo, and arguably means the paper's own example does
   not work as printed.

`role-chain.yaml` is the mutation target named in spec section 10:
deleting `hasParticipant`'s `owl:propertyChainAxiom` from a scratch
copy of `sulo.ttl` must flip it.
