//! Term-level comparison of expected and actual competency-question rows.
//!
//! Spec 7.3 is binding. Comparison is by RDF term, never by string: a bare
//! literal is an `xsd:string` and does not equal the "same" value typed as
//! something else, because value-space equality would hide serialisation
//! regressions, which is a thing this harness exists to catch. Leniency
//! here is the failure mode this module exists to prevent, not a
//! convenience to add: about sixty suite cases are written on top of it,
//! and each one is only as strict as this comparison is.

use std::collections::BTreeMap;
use std::fmt;

use curie::PrefixMapping;
use oxrdf::{Literal, NamedNode, Term};

use crate::prefixes::{self, PrefixError};

/// A parsed `expect_rows` cell: either a bound RDF term or an explicit
/// requirement that the variable be unbound in that row.
///
/// `Unbound` is deliberately distinct from the key being absent from the
/// row map: the former asserts unboundedness, the latter yields a row
/// that can never match. An expected row must name EVERY variable the
/// query projects, because [`compare`] compares whole rows by
/// `BTreeMap` equality and an actual row always carries one key per
/// projected variable; see [`compare`]'s own doc comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expected {
    Bound(Term),
    Unbound,
}

#[derive(Debug, thiserror::Error)]
pub enum RowError {
    #[error(transparent)]
    Prefix(#[from] PrefixError),
    #[error("{0}")]
    Syntax(String),
    #[error(
        "blank node '{0}' cannot appear in expect_rows: blank nodes never \
         compare equal across runs; use a skolemised IRI instead"
    )]
    BlankNode(String),
}

/// Parse one `expect_rows` cell into an [`Expected`].
///
/// Recognised forms, in order: `None` (a YAML `null`) means the variable
/// must be unbound; a token starting `"` is a literal, with an optional
/// `^^datatype` (itself resolved through `pm`) or `@lang` suffix,
/// defaulting to `xsd:string`; a token starting `_:` is a configuration
/// error, because blank nodes never compare equal across runs; anything
/// else is either a CURIE or a full `<IRI>`, both resolved via
/// [`prefixes::expand`].
pub fn parse_expected(token: Option<&str>, pm: &PrefixMapping) -> Result<Expected, RowError> {
    let Some(raw) = token else {
        return Ok(Expected::Unbound);
    };
    let t = raw.trim();

    if t.starts_with('"') {
        return Ok(Expected::Bound(parse_literal(t, pm)?));
    }

    if let Some(id) = t.strip_prefix("_:") {
        return Err(RowError::BlankNode(id.to_string()));
    }

    let iri = prefixes::expand(pm, t)?;
    let node = NamedNode::new(iri.clone())
        .map_err(|e| RowError::Syntax(format!("'{iri}' is not a valid IRI: {e}")))?;
    Ok(Expected::Bound(Term::NamedNode(node)))
}

/// Parse a quoted literal token: `"value"`, `"value"^^datatype`, or
/// `"value"@lang`. `t` must start with `"`.
fn parse_literal(t: &str, pm: &PrefixMapping) -> Result<Term, RowError> {
    let mut value = String::new();
    let mut chars = t.char_indices();
    chars.next(); // the opening quote, already checked by the caller

    let mut escaped = false;
    let mut close_at = None;
    for (i, c) in chars {
        if escaped {
            match c {
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                'n' => value.push('\n'),
                't' => value.push('\t'),
                'r' => value.push('\r'),
                other => {
                    return Err(RowError::Syntax(format!(
                        "'\\{other}' is not a recognised escape in literal {t}: use \\\" \\\\ \\n \\t \\r"
                    )));
                }
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            close_at = Some(i + c.len_utf8());
            break;
        } else {
            value.push(c);
        }
    }

    let close_at =
        close_at.ok_or_else(|| RowError::Syntax(format!("unterminated literal: {t}")))?;
    let suffix = &t[close_at..];

    if suffix.is_empty() {
        return Ok(Term::Literal(Literal::new_simple_literal(value)));
    }

    if let Some(datatype_token) = suffix.strip_prefix("^^") {
        let iri = prefixes::expand(pm, datatype_token)?;
        let datatype = NamedNode::new(iri.clone())
            .map_err(|e| RowError::Syntax(format!("'{iri}' is not a valid datatype IRI: {e}")))?;
        return Ok(Term::Literal(Literal::new_typed_literal(value, datatype)));
    }

    if let Some(lang) = suffix.strip_prefix('@') {
        let literal = Literal::new_language_tagged_literal(value, lang)
            .map_err(|e| RowError::Syntax(format!("'{lang}' is not a valid language tag: {e}")))?;
        return Ok(Term::Literal(literal));
    }

    Err(RowError::Syntax(format!(
        "unexpected trailing content after literal in '{t}': '{suffix}'"
    )))
}

/// Compare `actual` rows against `expect_rows`, per spec 7.3.
///
/// Row equality is a map equality over `BTreeMap<String, Option<Term>>`:
/// every key present in `expected` must be present in `actual` with the
/// identical term (`None` meaning explicitly unbound), and vice versa,
/// since `BTreeMap`'s `PartialEq` already requires equal key sets.
///
/// `exact` and `ordered` are independent axes, as in spec 7.3, with one
/// combination refused rather than decided: `ordered: true, exact: false`
/// is genuinely ambiguous under spec 7.3 (it equally supports "expected is
/// a contiguous prefix of actual" and "expected is a non-contiguous ordered
/// subsequence of actual, with extras allowed anywhere"), so it is a
/// configuration error rather than a silently picked reading. Use
/// `ordered: true, exact: true` for an exact sequence, or `ordered: false,
/// exact: false` for an unordered subset.
///
/// That guard is DEFENCE-IN-DEPTH, not the primary one, and it is not
/// dead code. `manifest::load_case` rejects the same combination as a
/// `ManifestError` before any ontology is loaded, so no manifest-driven
/// run can reach it; it stays here for direct library callers, which
/// construct `expected`/`actual` themselves and never pass through a
/// manifest, and it has its own test
/// (`tests/rows.rs::ordered_true_with_exact_false_is_a_configuration_error`)
/// that calls it directly. That reference is a path and a name, so a rename
/// orphans it silently; it is kept as a pointer rather than made robust
/// because nothing here can enforce it, and the test is findable by what it
/// calls rather than by what it is named (`grep -rn ', false, true)'
/// tests/rows.rs`, the one call site passing this combination) even after
/// one. Note the asymmetry with the OTHER manifest-level
/// `cq` guard: an empty `expected` with `exact: false` is a perfectly
/// meaningful request of this function ("check nothing"), so it is refused
/// at the manifest layer only, where "a case that asserts nothing" is the
/// thing being refused.
///
/// When `ordered` (and therefore also `exact`, per the above), expected
/// row `i` must equal actual row `i`, in order; a missing position is an
/// error, and once every expected row has matched positionally, any
/// leftover actual rows are also an error. Otherwise (`ordered: false`)
/// rows are compared as a multiset: each expected row is removed from a
/// working copy of `actual`, so a duplicate expected row must be matched
/// once per occurrence, and `exact` then governs whether leftover actual
/// rows are tolerated (`exact: false`) or an error (`exact: true`). Never
/// the reverse: `expected` must always be fully accounted for in `actual`.
///
/// Returns `Err` naming the first missing or mismatched expected row, or,
/// for `exact`, the count and first example of the unmatched actual rows.
pub fn compare(
    expected: &[BTreeMap<String, Option<Term>>],
    actual: &[BTreeMap<String, Option<Term>>],
    exact: bool,
    ordered: bool,
) -> Result<(), String> {
    if ordered && !exact {
        return Err(
            "ordered: true with exact: false is not defined: spec 7.3 does not say \
             whether an unmatched actual row may appear before, between, or only \
             after the expected sequence, so this combination (ordered=true, \
             exact=false) is refused rather than guessed. Use ordered: true, \
             exact: true for an exact sequence, or ordered: false, exact: false \
             for an unordered subset."
                .to_string(),
        );
    }

    if ordered {
        for (i, e) in expected.iter().enumerate() {
            match actual.get(i) {
                Some(a) if a == e => {}
                Some(a) => {
                    return Err(format!(
                        "ordered comparison: row {i} mismatch: expected {} but got {}",
                        describe_row(e),
                        describe_row(a)
                    ));
                }
                None => {
                    return Err(format!(
                        "ordered comparison: missing expected row {i}: {}",
                        describe_row(e)
                    ));
                }
            }
        }
        if exact && actual.len() > expected.len() {
            return Err(format!(
                "exact comparison: {} extra actual row(s) beyond expect_rows, e.g. {}",
                actual.len() - expected.len(),
                describe_row(&actual[expected.len()])
            ));
        }
        return Ok(());
    }

    let mut remaining: Vec<&BTreeMap<String, Option<Term>>> = actual.iter().collect();
    for e in expected {
        match remaining.iter().position(|a| *a == e) {
            Some(pos) => {
                remaining.remove(pos);
            }
            None => {
                return Err(format!("missing expected row: {}", describe_row(e)));
            }
        }
    }

    if exact && !remaining.is_empty() {
        return Err(format!(
            "exact comparison: {} extra actual row(s) not in expect_rows, e.g. {}",
            remaining.len(),
            describe_row(remaining[0])
        ));
    }

    Ok(())
}

fn describe_row(row: &BTreeMap<String, Option<Term>>) -> String {
    struct Row<'a>(&'a BTreeMap<String, Option<Term>>);
    impl fmt::Display for Row<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{{")?;
            for (i, (var, val)) in self.0.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                match val {
                    Some(t) => write!(f, "?{var} = {t}")?,
                    None => write!(f, "?{var} = (unbound)")?,
                }
            }
            write!(f, "}}")
        }
    }
    Row(row).to_string()
}
