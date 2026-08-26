//! The set of cases the default run does NOT execute.
//!
//! `suite::DEFERRED_TAG` suppresses a case: a tagged case is named and
//! counted but never run, and can never set the exit code. That makes
//! the tag a way to silence a failing case, which is this project's
//! recurring defect shape (a check that cannot fail) with a manifest
//! key instead of a bad assertion.
//!
//! So the tagged set is pinned here and diffed against a live scan of
//! the suite in BOTH directions, exactly as the six group tests diff
//! their `EXPECTED` tables against the directory listing. Adding the
//! tag to any further case fails this test until someone updates
//! `DEFERRED` deliberately, and removing the tag from a case listed
//! here fails it too.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sulo_testharness::manifest::load_case;
use sulo_testharness::suite::{DEFERRED_TAG, discover};

const SUITE: &str = "suites/sulo";

/// Every case in the SULO suite carrying [`DEFERRED_TAG`], and the
/// reason each one is there.
///
/// `timeinstant-datarange` asserts `TimeInstant subClassOf hasValue
/// only (xsd:dateTime or xsd:dateTimeStamp)`, a data-range
/// `allValuesFrom` the pinned reasoner provably cannot enforce; the
/// ontology logs baseline loss for that exact axiom on every load.
/// Spec line 746 puts such a case in the CI differential (5.3).
const DEFERRED: &[&str] = &["timeinstant-datarange"];

fn tagged_on_disk() -> BTreeSet<String> {
    let paths: Vec<PathBuf> =
        discover(Path::new(SUITE)).expect("the SULO suite should be discoverable");
    paths
        .iter()
        .filter_map(|p| {
            let case = load_case(p).unwrap_or_else(|e| panic!("{} should parse: {e}", p.display()));
            case.tags
                .iter()
                .any(|t| t == DEFERRED_TAG)
                .then(|| case.id.clone())
        })
        .collect()
}

#[test]
fn every_deferred_case_on_disk_is_pinned_here() {
    let on_disk = tagged_on_disk();
    let pinned: BTreeSet<String> = DEFERRED.iter().map(|s| (*s).to_string()).collect();

    let untabled: Vec<_> = on_disk.difference(&pinned).collect();
    assert!(
        untabled.is_empty(),
        "case(s) carry the `{DEFERRED_TAG}` tag but are not pinned in DEFERRED: {untabled:?}. \
         The tag stops a case from ever running or failing, so adding it must be a deliberate, \
         reviewed act, not a quiet way to silence a red case."
    );
}

#[test]
fn every_pinned_case_still_carries_the_tag() {
    let on_disk = tagged_on_disk();
    let pinned: BTreeSet<String> = DEFERRED.iter().map(|s| (*s).to_string()).collect();

    let stale: Vec<_> = pinned.difference(&on_disk).collect();
    assert!(
        stale.is_empty(),
        "DEFERRED pins case(s) that no longer carry the `{DEFERRED_TAG}` tag: {stale:?}. \
         If the reasoner has caught up, move the case into its group's EXPECTED table; \
         do not leave a stale pin behind."
    );
}

/// The pinned set is not empty, so the two diffs above cannot both
/// hold vacuously by comparing two empty sets.
#[test]
fn the_pinned_set_is_not_empty() {
    assert!(
        !DEFERRED.is_empty(),
        "an empty DEFERRED would make both direction tests pass against an empty scan"
    );
}
