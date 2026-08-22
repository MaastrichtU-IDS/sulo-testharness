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

echo "regenerated: no-role-chain.ttl, no-transitive-parthood.ttl, no-feature-union.ttl, no-subproperty-containment.ttl"
