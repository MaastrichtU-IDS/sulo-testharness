//! The golden inference closure.
//!
//! A regression harness needs to detect that the answer CHANGED, not
//! to know absolute truth. That is why this works despite the
//! reasoner being incomplete: both sides of the diff come from the
//! same oracle at the same version, so whatever it cannot see is held
//! constant and cancels out. It therefore guards every entailment in
//! the closure, not only the ones somebody thought to assert.
//!
//! The header pins the reasoner version. A version change legitimately
//! moves the closure, so it is reported as "re-baseline required"
//! rather than as drift or as a pass.
//!
//! The closure is built from a SINGLE `owl_dl_reasoner::classify` call.
//! `Classification` precomputes the full pairwise entailment matrix, so
//! `is_subclass` is a matrix lookup, not a fresh reasoning call: one
//! reasoner invocation over the whole ontology, not one per ordered
//! pair. This matters for more than speed: an unbounded per-pair
//! reasoner call has previously hung for over 24 minutes on real SULO
//! (see `oracle.rs`'s module doc), so multiplying unbounded calls by
//! n-squared pairs would be reckless. `classify` also exposes what it
//! could NOT decide (`undecided_pairs`) and whether completeness is
//! guaranteed (`completeness_guaranteed`); both are recorded here,
//! because a golden file that reports only what was decided implies
//! more certainty than the run actually had, and drift in the
//! undecided set is itself meaningful: it means the oracle's reach
//! changed.
//!
//! The closure also records the named property hierarchy
//! (`classify_object_property_hierarchy` /
//! `classify_data_property_hierarchy`), each a single cheap
//! structural pass over asserted and materialised axioms, not a
//! per-pair tableau call. Without this, the closure is blind to a
//! real regression: removing SULO's `isPartOf rdfs:subPropertyOf
//! isIn` / `hasPart rdfs:subPropertyOf contains` pair
//! (`mutants/no-subproperty-containment.ttl`) leaves the named-class
//! subsumption matrix byte-identical, since none of those four
//! properties appears in any class-defining restriction in SULO; the
//! drift is visible only in the property hierarchy. Verified
//! empirically before adding this: with only the class hierarchy, all
//! four checked-in mutants left `closure` byte-identical to clean
//! SULO, which would have made the golden diff blind to every
//! regression the mutation suite actually exercises. Since the golden
//! file's entire justification is guarding every entailment the
//! oracle can report, not only the ones a class query happens to
//! touch, the property hierarchy belongs in the closure alongside the
//! class hierarchy.

use std::collections::BTreeSet;
use std::path::Path;

use horned_owl::model::RcStr;
use horned_owl::ontology::set::SetOntology;

/// The reasoner version this closure was produced with.
pub const REASONER_VERSION: &str = "rustdl v0.4.22";

/// The reasoner version pinned in a golden file's header.
#[derive(Debug, PartialEq, Eq)]
pub struct GoldenHeader {
    pub reasoner_version: String,
}

/// Outcome of comparing a closure to its golden file.
#[derive(Debug, PartialEq, Eq)]
pub enum GoldenOutcome {
    Match,
    Drift(String),
    Rebaselined,
    RebaselineRequired(String),
}

/// Serialise the full inferred closure, sorted and canonical.
///
/// One `classify` call computes the whole entailment matrix; every
/// accessor below (`is_subclass`, `equivalent_classes`,
/// `unsatisfiable_classes`, `undecided_pairs`,
/// `completeness_guaranteed`) is a cheap read of that matrix, not a
/// fresh reasoning call.
pub fn closure(onto: &SetOntology<RcStr>) -> Result<String, String> {
    let classification = owl_dl_reasoner::classify(onto).map_err(|e| e.to_string())?;

    let unsatisfiable: BTreeSet<&str> =
        classification.unsatisfiable_classes().into_iter().collect();

    let mut lines: BTreeSet<String> = BTreeSet::new();

    for class in classification.classes() {
        let sat = !unsatisfiable.contains(class.as_str());
        lines.insert(format!("satisfiable\t{class}\t{sat}"));

        // Every entailed named subsumption, not only the direct ones,
        // so a lost intermediate axiom still shows as drift. This is
        // a matrix lookup per pair, not a reasoning call per pair.
        for other in classification.classes() {
            if class == other {
                continue;
            }
            if classification.is_subclass(class, other) {
                lines.insert(format!("subClassOf\t{class}\t{other}"));
            }
        }

        // Named equivalences, excluding the reflexive self-pair.
        for equiv in classification.equivalent_classes(class) {
            if equiv != class.as_str() {
                lines.insert(format!("equivalentClass\t{class}\t{equiv}"));
            }
        }
    }

    // What the oracle could not decide. Recorded in the body (so it
    // is sorted like everything else, and so drift in the undecided
    // set is caught the same way drift in a decided entailment is).
    for (sub, sup) in classification.undecided_pairs() {
        lines.insert(format!("undecided\t{sub}\t{sup}"));
    }

    // The named property hierarchy. See the module doc: this is what
    // makes the closure sensitive to a subproperty regression that
    // never touches any class-defining restriction, and therefore
    // never moves the class matrix above.
    let object_props =
        owl_dl_reasoner::classify_object_property_hierarchy(onto).map_err(|e| e.to_string())?;
    for (sub, sup) in object_props.direct_subsumptions() {
        lines.insert(format!("subObjectPropertyOf\t{sub}\t{sup}"));
    }
    for group in object_props.equivalent_groups() {
        for a in group {
            for b in group {
                if a != b {
                    lines.insert(format!("equivalentObjectProperty\t{a}\t{b}"));
                }
            }
        }
    }

    let data_props =
        owl_dl_reasoner::classify_data_property_hierarchy(onto).map_err(|e| e.to_string())?;
    for (sub, sup) in data_props.direct_subsumptions() {
        lines.insert(format!("subDataPropertyOf\t{sub}\t{sup}"));
    }
    for group in data_props.equivalent_groups() {
        for a in group {
            for b in group {
                if a != b {
                    lines.insert(format!("equivalentDataProperty\t{a}\t{b}"));
                }
            }
        }
    }

    let mut out = String::new();
    out.push_str(&format!("# reasoner: {REASONER_VERSION}\n"));
    out.push_str(&format!(
        "# completeness_guaranteed: {}\n",
        classification.completeness_guaranteed()
    ));
    out.push_str("# generated by sulo-testharness; regenerate with --accept-golden\n");
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }

    Ok(out)
}

/// Line-level diff. `None` means identical.
#[must_use]
pub fn diff(current: &str, golden: &str) -> Option<String> {
    let cur: BTreeSet<&str> = current.lines().filter(|l| !l.starts_with('#')).collect();
    let gold: BTreeSet<&str> = golden.lines().filter(|l| !l.starts_with('#')).collect();

    if cur == gold {
        return None;
    }

    let mut out = String::new();
    for line in gold.difference(&cur) {
        out.push_str(&format!("- {line}\n"));
    }
    for line in cur.difference(&gold) {
        out.push_str(&format!("+ {line}\n"));
    }
    Some(out)
}

fn golden_reasoner_version(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("# reasoner: "))
        .map(str::to_string)
}

/// Compare against the golden file, optionally re-baselining.
pub fn check_golden(onto: &SetOntology<RcStr>, path: &Path, accept: bool) -> GoldenOutcome {
    let current = match closure(onto) {
        Ok(c) => c,
        Err(e) => return GoldenOutcome::Drift(format!("could not compute closure: {e}")),
    };

    if accept || !path.exists() {
        if let Err(e) = std::fs::write(path, &current) {
            return GoldenOutcome::Drift(format!("could not write golden file: {e}"));
        }
        return GoldenOutcome::Rebaselined;
    }

    let golden = match std::fs::read_to_string(path) {
        Ok(g) => g,
        Err(e) => return GoldenOutcome::Drift(format!("could not read golden file: {e}")),
    };

    match golden_reasoner_version(&golden) {
        Some(v) if v != REASONER_VERSION => {
            return GoldenOutcome::RebaselineRequired(format!(
                "golden file was produced with {v}, running {REASONER_VERSION}. \
                 A reasoner change legitimately moves the closure; \
                 review and re-run with --accept-golden."
            ));
        }
        None => {
            return GoldenOutcome::RebaselineRequired(
                "golden file has no reasoner version header".to_string(),
            );
        }
        Some(_) => {}
    }

    match diff(&current, &golden) {
        None => GoldenOutcome::Match,
        Some(d) => GoldenOutcome::Drift(d),
    }
}
