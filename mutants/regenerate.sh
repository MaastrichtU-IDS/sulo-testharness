#!/usr/bin/env bash
# Regenerate every mutant in mutants/ from ../sulo/sulo.ttl.
#
# Run this from the repository root after a SULO bump:
#   ./mutants/regenerate.sh
#
# Each edit is documented in mutants/README.md. This script is the
# single source of truth for how each mutant is derived; the README
# describes the edits, this script performs them. tests/mutation.rs
# independently re-derives the same edits in Rust and compares against
# the committed mutant files (a staleness guard, not a duplicate of
# this script): if SULO changes and these files are not regenerated,
# that test fails loudly instead of quietly testing a stale ontology.
set -euo pipefail
cd "$(dirname "$0")/.."

SULO=../sulo/sulo.ttl
MUTANTS=mutants

if [ ! -f "$SULO" ]; then
    echo "error: $SULO not found (expected the sulo repo checked out as a sibling of sulo-testharness)" >&2
    exit 1
fi

# 1. Delete the PRO role chain. Not a naive `grep -v`: the
#    propertyChainAxiom line is the LAST triple of the hasParticipant
#    statement, terminated with `.`; deleting only that line leaves
#    the preceding line's trailing `;` dangling and produces invalid
#    Turtle (horned-owl panics on it). Rewrite the statement's tail
#    precisely instead.
python3 - "$SULO" "$MUTANTS/no-role-chain.ttl" <<'PY'
import sys
src_path, out_path = sys.argv[1], sys.argv[2]
src = open(src_path).read()
needle = '''    owl:inverseOf sulo:isParticipantIn ;
    owl:propertyChainAxiom ( sulo:hasParticipant [ owl:inverseOf sulo:hasFeature ] ) .'''
replacement = '    owl:inverseOf sulo:isParticipantIn .'
assert src.count(needle) == 1, f"expected exactly 1 occurrence in {src_path}, found {src.count(needle)}"
open(out_path, 'w').write(src.replace(needle, replacement, 1))
PY

# 2. Break BOTH halves of the parthood inverse pair's transitivity:
#    sulo:isPartOf AND its inverse sulo:hasPart. Removing only one
#    side is semantically inert: OWL DL entails that a property's
#    inverse is transitive whenever the property itself is, so the
#    untouched side re-derives the same closure. See mutants/README.md.
python3 - "$SULO" "$MUTANTS/no-transitive-parthood.ttl" <<'PY'
import sys
src_path, out_path = sys.argv[1], sys.argv[2]
src = open(src_path).read()

def strip_transitive(text, anchor):
    start = text.index(anchor)
    end = text.index('\n\n', start)
    block = text[start:end]
    patched = block.replace(
        'owl:ReflexiveProperty,\n        owl:TransitiveProperty ;',
        'owl:ReflexiveProperty ;'
    )
    assert patched != block, f"transitivity pattern not found for {anchor!r} in {src_path}"
    return text[:start] + patched + text[end:]

out = strip_transitive(src, 'sulo:isPartOf a owl:ObjectProperty')
out = strip_transitive(out, 'sulo:hasPart a owl:ObjectProperty')
open(out_path, 'w').write(out)
PY

# 3. Delete the Feature disjointUnionOf, leaving its AllDisjointClasses
#    in place. Only the covering case should react; the sibling
#    disjointness counter-examples must not.
grep -v "owl:disjointUnionOf ( sulo:Capability sulo:InformationObject sulo:Quality sulo:Role )" \
    "$SULO" > "$MUTANTS/no-feature-union.ttl"

# 4. Break BOTH halves of the parthood/containment subproperty pair:
#    isPartOf -> isIn AND its inverse-side counterpart
#    hasPart -> contains. Same redundancy as edit 2, one level over.
python3 - "$SULO" "$MUTANTS/no-subproperty-containment.ttl" <<'PY'
import sys
src_path, out_path = sys.argv[1], sys.argv[2]
src = open(src_path).read()

needle_isin = '    rdfs:subPropertyOf sulo:isIn .'
assert src.count(needle_isin) == 1, f"expected exactly 1 occurrence in {src_path}"
out = src.replace(needle_isin, '    a owl:ObjectProperty .', 1)

needle_contains = '''    rdfs:subPropertyOf sulo:contains ;
    owl:inverseOf sulo:isPartOf .'''
assert out.count(needle_contains) == 1, f"expected exactly 1 occurrence in {src_path}"
out = out.replace(needle_contains, '    owl:inverseOf sulo:isPartOf .', 1)

open(out_path, 'w').write(out)
PY

# 5. Delete Feature's own `rdfs:subClassOf sulo:Object` (a single,
#    non-redundant named-class axiom, distinct from the two blank-node
#    restrictions in the same list). Catches patterns/solid/typing-chain
#    and the patterns/solid/value-quality-unit competency question.
python3 - "$SULO" "$MUTANTS/no-feature-object.ttl" <<'PY'
import sys
src_path, out_path = sys.argv[1], sys.argv[2]
src = open(src_path).read()
needle = '''        [ a owl:Restriction ;
            owl:allValuesFrom sulo:Feature ;
            owl:onProperty sulo:hasPart ],
        sulo:Object ;'''
replacement = '''        [ a owl:Restriction ;
            owl:allValuesFrom sulo:Feature ;
            owl:onProperty sulo:hasPart ] ;'''
assert src.count(needle) == 1, f"expected exactly 1 occurrence in {src_path}, found {src.count(needle)}"
open(out_path, 'w').write(src.replace(needle, replacement, 1))
PY

# 6. Delete BOTH `hasPart only self` restrictions, on Feature AND on
#    InformationObject. Mutation-verified as needing both: either alone
#    still leaves the other class's restriction to propagate Feature-hood
#    onto the same individual (measurement is typed both Feature and
#    InformationObject). Catches patterns/solid/unit-forced-feature, and
#    also (verified) restrictions/hasPart-propagation-feature and
#    restrictions/hasPart-propagation-informationobject, since those two
#    cases test the exact same restrictions directly.
python3 - "$SULO" "$MUTANTS/no-selfpart-feature-and-informationobject.ttl" <<'PY'
import sys
src_path, out_path = sys.argv[1], sys.argv[2]
src = open(src_path).read()

needle_feature = '''        [ a owl:Restriction ;
            owl:allValuesFrom sulo:Feature ;
            owl:onProperty sulo:hasPart ],
        sulo:Object ;'''
assert src.count(needle_feature) == 1, f"expected exactly 1 occurrence in {src_path}"
out = src.replace(needle_feature, '        sulo:Object ;', 1)

needle_io = '''    rdfs:subClassOf [ a owl:Restriction ;
            owl:allValuesFrom sulo:InformationObject ;
            owl:onProperty sulo:hasPart ],
        [ a owl:Restriction ;
            owl:allValuesFrom rdfs:Literal ;
            owl:onProperty sulo:hasValue ],
        sulo:Feature .'''
assert out.count(needle_io) == 1, f"expected exactly 1 occurrence in {src_path}"
replacement_io = '''    rdfs:subClassOf [ a owl:Restriction ;
            owl:allValuesFrom rdfs:Literal ;
            owl:onProperty sulo:hasValue ],
        sulo:Feature .'''
out = out.replace(needle_io, replacement_io, 1)

open(out_path, 'w').write(out)
PY

# 7. Delete Process's `hasPart only Process` restriction entirely (its
#    only rdfs:subClassOf member, so the whole predicate-object pair is
#    removed, not just the blank node). Catches
#    restrictions/hasPart-propagation-process.
python3 - "$SULO" "$MUTANTS/no-selfpart-process.ttl" <<'PY'
import sys
src_path, out_path = sys.argv[1], sys.argv[2]
src = open(src_path).read()
needle = '''    rdfs:subClassOf [ a owl:Restriction ;
            owl:allValuesFrom sulo:Process ;
            owl:onProperty sulo:hasPart ] ;
'''
assert src.count(needle) == 1, f"expected exactly 1 occurrence in {src_path}"
open(out_path, 'w').write(src.replace(needle, '', 1))
PY

# 8. Delete Quantity's `hasPart some Unit` someValuesFrom restriction
#    (its only other rdfs:subClassOf member besides the named class
#    sulo:InformationObject). Catches
#    restrictions/quantity-haspart-some-unit. (Not
#    TimeInterval's identically-shaped restriction: restrictions/README.md
#    records that one as semantically inert, re-derived via TimeInterval
#    subClassOf Time subClassOf Quantity.)
python3 - "$SULO" "$MUTANTS/no-quantity-unit-somevaluesfrom.ttl" <<'PY'
import sys
src_path, out_path = sys.argv[1], sys.argv[2]
src = open(src_path).read()
needle = '''    rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty sulo:hasPart ;
            owl:someValuesFrom sulo:Unit ],
        sulo:InformationObject .'''
assert src.count(needle) == 1, f"expected exactly 1 occurrence in {src_path}"
open(out_path, 'w').write(src.replace(needle, '    rdfs:subClassOf sulo:InformationObject .', 1))
PY

# 9. Delete hasParticipant's own `rdfs:domain sulo:Process` AND its
#    inverse isParticipantIn's own `rdfs:range sulo:Process` together.
#    Single-axiom deletion here is inert (domains-ranges/README.md):
#    ObjectPropertyDomain(hasParticipant, Process) is re-derivable from
#    ObjectPropertyRange(isParticipantIn, Process) plus
#    InverseObjectProperties, and vice versa. Catches
#    domains-ranges/hasparticipant (and, verified, domains-ranges/
#    isparticipantin too, since both cases require "?p a Process").
python3 - "$SULO" "$MUTANTS/no-participant-domain-and-inverse-range.ttl" <<'PY'
import sys
src_path, out_path = sys.argv[1], sys.argv[2]
src = open(src_path).read()

needle_domain = '''    rdfs:domain sulo:Process ;
    rdfs:range sulo:Object ;
    owl:inverseOf sulo:isParticipantIn ;'''
assert src.count(needle_domain) == 1, f"expected exactly 1 occurrence in {src_path}"
out = src.replace(
    needle_domain,
    '    rdfs:range sulo:Object ;\n    owl:inverseOf sulo:isParticipantIn ;',
    1,
)

needle_range = '''    rdfs:domain sulo:Object ;
    rdfs:range sulo:Process .'''
assert out.count(needle_range) == 1, f"expected exactly 1 occurrence in {src_path}"
out = out.replace(needle_range, '    rdfs:domain sulo:Object .', 1)

open(out_path, 'w').write(out)
PY

echo "regenerated: no-role-chain.ttl, no-transitive-parthood.ttl, no-feature-union.ttl, no-subproperty-containment.ttl, no-feature-object.ttl, no-selfpart-feature-and-informationobject.ttl, no-selfpart-process.ttl, no-quantity-unit-somevaluesfrom.ttl, no-participant-domain-and-inverse-range.ttl"
