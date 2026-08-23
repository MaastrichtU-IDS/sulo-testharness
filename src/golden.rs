//! The golden inference closure.
//!
//! A regression harness needs to detect that the answer CHANGED, not
//! to know absolute truth. That is why this works despite the
//! reasoner being incomplete: both sides of the diff come from the
//! same oracle at the same version, so whatever it cannot see is held
//! constant and cancels out. What it guards is exactly the surface
//! enumerated under "What this closure can and cannot see" below, and
//! no more: an earlier revision of this sentence claimed the closure
//! guards "every entailment in the closure, not only the ones somebody
//! thought to assert", which the spec has since retracted as false.
//! Read that section for the measured number before relying on this
//! file for anything.
//!
//! The header pins the reasoner version AND the completeness flag
//! (see below). Either changing legitimately moves the closure, so
//! both are reported as "re-baseline required" rather than as drift
//! or as a pass: the "same oracle, held constant, cancels out"
//! argument holds only while the oracle's strength is itself
//! constant, and a version bump is not the only thing that can change
//! it (a flag flip from `completeness_guaranteed=true` to `false`, for
//! example, is a genuine weakening of the oracle even at a fixed
//! version).
//!
//! A MISSING golden file is never silently written except when
//! `--accept-golden` is passed explicitly. Falling back to "write it
//! and pass" on a merely-absent file would make a wrong path, or a
//! checkout missing `suites/`, silently disable the harness's primary
//! defence and still exit 0: the exact "green while testing nothing"
//! failure `tests/mutation.rs`'s module doc exists to rule out,
//! reintroduced in the backstop meant to catch it. See `check_golden`.
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
//! changed. The completeness flag is not just recorded, it is
//! COMPARED (see `check_golden`): recording a certainty signal and
//! then ignoring it when checking for drift would reproduce the same
//! silence one level up.
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
//! drift is visible only in the property hierarchy.
//!
//! ASYMMETRY worth stating, because the two sections do not record the
//! same thing: the class section records the full transitive closure
//! of `subClassOf` (`is_subclass` over every ordered pair), whereas
//! both property sections record only `direct_subsumptions()`, the
//! direct edges. So dropping an INTERMEDIATE property axiom out of a
//! chain `p ⊑ q ⊑ r` shows up in this file as the two direct edges
//! changing, but the derived `p ⊑ r` was never recorded and cannot
//! move. That is a narrower sensitivity than the class side's, and it
//! is what `direct_subsumptions()` offers at the pinned version.
//!
//! # What this closure can and cannot see (state the number, do not oversell it)
//!
//! The closure's sensitivity surface is exactly: named class
//! subsumption, named class satisfiability, named class equivalence,
//! named object/data property subsumption and equivalence, and the
//! undecided-pair set. It is structurally BLIND to: property
//! characteristics (transitive, reflexive, functional, ...), property
//! chains (`owl:propertyChainAxiom`), domains and ranges, class and
//! property disjointness, covering axioms (`owl:disjointUnionOf`), and
//! every ABox-level entailment (instances, property values). None of
//! those participate in `Classification`'s class matrix or in
//! `classify_object_property_hierarchy`/`classify_data_property_hierarchy`'s
//! materialised edges.
//!
//! Measured directly against this repository's four checked-in
//! mutants (`mutants/README.md`): this closure catches exactly ONE of
//! four. `no-subproperty-containment.ttl` is caught, via the property
//! hierarchy above. `no-role-chain.ttl` is NOT caught:
//! `materialize_subobjectproperty_axioms` explicitly skips a chain
//! sub-expression, so a lost `owl:propertyChainAxiom` never reaches
//! either hierarchy this closure reads. `no-transitive-parthood.ttl`
//! is NOT caught: `owl:TransitiveProperty` is not a component kind
//! either hierarchy matches, and removing it moves neither the class
//! matrix nor the property edges. `no-feature-union.ttl` is NOT
//! caught: it removes a `disjointUnionOf` covering axiom, but the four
//! `rdfs:subClassOf sulo:Feature` edges it would otherwise imply are
//! ALSO asserted directly elsewhere in SULO, and pairwise disjointness
//! among the four survives in the redundant `owl:AllDisjointClasses`
//! axiom, so nothing in the class matrix moves either. The other
//! three mutants are caught by `tests/mutation.rs`'s case-based suite,
//! a different, complementary defence layer; this golden closure does
//! not duplicate that coverage and is not a substitute for it.
//!
//! DEFERRED (not built here): three of spec 5.2's five closure
//! components, inferred class assertions, inferred property
//! assertions, and inferred disjointness, are absent. That is where
//! the other three mutants live. Closing that gap needs a fixed probe
//! ABox, since `sulo.ttl` itself declares no individuals; that is a
//! subsystem, not a fix, and belongs in the follow-on plan.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use horned_owl::model::RcStr;
use horned_owl::ontology::set::SetOntology;

/// The reasoner version this closure was produced with.
///
/// HAND-MAINTAINED, and deliberately so: `owl-dl-reasoner` exposes no
/// version constant, and `env!("CARGO_PKG_VERSION")` here would report
/// THIS crate's version, not the reasoner's. The binding that matters
/// is therefore a convention, not a compiler check: this literal must
/// be edited in the same commit as the `owl-dl-reasoner` `rev` in
/// `Cargo.toml`. Forget it and a dependency bump that legitimately
/// moves the closure surfaces as `Drift` (exit 4, "the ontology
/// regressed") instead of `RebaselineRequired` (exit 4, "the oracle
/// changed, review and re-accept"), which sends the reader looking for
/// an ontology defect that does not exist. `check_golden` compares
/// this against the golden file's header for exactly that reason.
pub const REASONER_VERSION: &str = "rustdl v0.4.22";

/// A golden file's parsed header: the reasoner version it was
/// produced with, and whether that run's completeness was guaranteed.
/// Both fields are compared in `check_golden`; either changing is
/// treated as re-baseline required, not as drift and not as a pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenHeader {
    pub reasoner_version: String,
    pub completeness_guaranteed: bool,
}

/// Outcome of comparing a closure to its golden file.
#[derive(Debug, PartialEq, Eq)]
pub enum GoldenOutcome {
    Match,
    Drift(String),
    Rebaselined,
    RebaselineRequired(String),
    /// A harness or IO failure: the closure could not be computed, the
    /// golden file could not be read or written, or (with `accept`
    /// false) no golden file exists at all. Never a verdict about the
    /// ontology's entailments, so it must never be conflated with
    /// `Drift`: an unreadable file or a reasoner error is not a
    /// regression, and reporting it as one trains an operator to
    /// reach for `--accept-golden`, which is exactly the wrong reflex
    /// for a permissions problem or an inconsistent ontology.
    Error(String),
}

/// Serialise the full inferred closure, sorted and canonical.
///
/// One `classify` call computes the whole entailment matrix; every
/// accessor below (`is_subclass`, `equivalent_classes`,
/// `unsatisfiable_classes`, `undecided_pairs`,
/// `completeness_guaranteed`) is a cheap read of that matrix, not a
/// fresh reasoning call.
pub fn closure(onto: &SetOntology<RcStr>) -> Result<String, String> {
    // `classify` is called with NO timeout and NO global deadline: it
    // is fully unbounded. The 24-minute-hang precedent in `oracle.rs`
    // therefore applies in principle to this call too, though nothing
    // in real SULO's 17 named classes has yet approached it (a full
    // run here takes well under a second). If it is ever bounded, the
    // ONLY honest option is `classify_with_global_deadline`:
    // `classify_with_timeout` (`classify.rs:840`) defaults a timed-out
    // pair to "not subsumed" and bumps `timed_out_pairs`, and does NOT
    // populate `undecided_pairs`, so it would silently drop timed-out
    // subsumptions out of this closure with no record in the file at
    // all, which is precisely the failure the `undecided` lines below
    // exist to prevent. Bounding with `classify_with_global_deadline`
    // instead would make `undecided_pairs` non-empty in a
    // wall-clock-dependent way,
    // and therefore make the `undecided` lines in this closure flaky
    // (the same run could report a different undecided set depending
    // on machine load), which is a real cost against a real safety
    // benefit; that trade-off is deliberately not made here, and
    // should be made explicitly, not rediscovered, if this ever times
    // out on a future SULO revision. Consequence of staying unbounded:
    // `undecided_pairs` is inert today, always empty, because nothing
    // ever times out. It is kept anyway: it is honest (it will start
    // reporting the moment a bound is ever introduced) and it costs
    // nothing while empty.
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
    // See the comment on the `classify` call above: always empty
    // today, since `classify` is unbounded here.
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

/// Parse a closure's two header lines. `None` if either is missing or
/// malformed: a golden file this cannot parse is never trusted enough
/// to compare against, so callers treat `None` as re-baseline
/// required, the same as a genuine version or completeness mismatch.
fn parse_header(text: &str) -> Option<GoldenHeader> {
    let reasoner_version = text
        .lines()
        .find_map(|l| l.strip_prefix("# reasoner: "))?
        .to_string();
    let completeness_guaranteed = text
        .lines()
        .find_map(|l| l.strip_prefix("# completeness_guaranteed: "))
        .and_then(|v| v.parse::<bool>().ok())?;
    Some(GoldenHeader {
        reasoner_version,
        completeness_guaranteed,
    })
}

/// Best-effort absolute form of `path`, for error messages only. Falls
/// back to the given path unchanged if it cannot be resolved (for
/// example on an exotic filesystem); never fails the caller over a
/// cosmetic detail in an already-erroring message.
fn display_path(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Compare against the golden file, optionally re-baselining.
///
/// With `accept` true, ALWAYS (re)writes the file and returns
/// `Rebaselined`: this is the one deliberate, explicit way to accept a
/// new closure. With `accept` false and no file present, this returns
/// `Error`, never a silent `Rebaselined`: falling back to "write it
/// and exit 0" on a merely-absent path would let a wrong `--golden`
/// argument, or a checkout missing `suites/`, silently disable the
/// harness's primary defence while still reporting success.
pub fn check_golden(onto: &SetOntology<RcStr>, path: &Path, accept: bool) -> GoldenOutcome {
    let current = match closure(onto) {
        Ok(c) => c,
        Err(e) => return GoldenOutcome::Error(format!("could not compute closure: {e}")),
    };

    if accept {
        return match std::fs::write(path, &current) {
            Ok(()) => GoldenOutcome::Rebaselined,
            Err(e) => GoldenOutcome::Error(format!("could not write golden file: {e}")),
        };
    }

    if !path.exists() {
        return GoldenOutcome::Error(format!(
            "no golden file at {}; run with --accept-golden to create one deliberately",
            display_path(path).display()
        ));
    }

    let golden = match std::fs::read_to_string(path) {
        Ok(g) => g,
        Err(e) => return GoldenOutcome::Error(format!("could not read golden file: {e}")),
    };

    // `current` was just produced by `closure` above, which always
    // writes both header lines, so this parse cannot fail.
    let current_header = parse_header(&current).expect("closure() always writes both header lines");

    let golden_header = match parse_header(&golden) {
        Some(h) => h,
        None => {
            return GoldenOutcome::RebaselineRequired(
                "golden file header is missing or malformed (expected '# reasoner: ...' \
                 and '# completeness_guaranteed: ...' lines)"
                    .to_string(),
            );
        }
    };

    if golden_header.reasoner_version != current_header.reasoner_version {
        return GoldenOutcome::RebaselineRequired(format!(
            "golden file was produced with {}, running {}. \
             A reasoner change legitimately moves the closure; \
             review and re-run with --accept-golden.",
            golden_header.reasoner_version, current_header.reasoner_version
        ));
    }

    if golden_header.completeness_guaranteed != current_header.completeness_guaranteed {
        return GoldenOutcome::RebaselineRequired(format!(
            "golden file was produced with completeness_guaranteed={}, this run computed \
             completeness_guaranteed={}. The oracle's completeness changed, a genuine \
             weakening or strengthening of what was verified even at the same reasoner \
             version; review and re-run with --accept-golden.",
            golden_header.completeness_guaranteed, current_header.completeness_guaranteed
        ));
    }

    match diff(&current, &golden) {
        None => GoldenOutcome::Match,
        Some(d) => GoldenOutcome::Drift(d),
    }
}
