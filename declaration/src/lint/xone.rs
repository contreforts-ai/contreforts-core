//! D14 -- the reason this crate exists (Part 3 of
//! contreforts/contreforts-core#14).
//!
//! contreforts/contreforts-workspace#27 established, by running it, that a
//! `sh:xone` union whose alternatives list only their own fields silently
//! accepts an instance mixing both variants: `conforms=true, violations=0`.
//! A SHACL property shape ignores predicates it does not mention, so the
//! contaminating fields are invisible to the alternative, exactly one
//! alternative conforms on its own terms, and `sh:xone` ("exactly one
//! conforms") is satisfied. Under D8 that is not cosmetic: it leaves an
//! invisible stale credential in the config store.
//!
//! SHACL cannot express the fix as a constraint on itself -- it would need
//! to reason about *other* alternatives' fields, which no SHACL construct
//! can reach into. This is a Rust analysis over the parsed declaration
//! graph instead, run at build time.
//!
//! The algorithm, verbatim from the issue: for each node shape `S`
//! carrying `sh:xone (A₁ … Aₙ)`,
//!   1. For each alternative `Aᵢ`, `pos(Aᵢ)` is the set of `sh:path`
//!      values of its property shapes that do NOT carry `sh:maxCount 0`.
//!   2. `excl(Aᵢ) = pos(Aᵢ) \ ⋃ⱼ≠ᵢ pos(Aⱼ)` -- the predicates exclusive to
//!      that alternative. (A discriminator like `authMode`, present in
//!      every alternative, is correctly not exclusive.)
//!   3. Require: for every pair `i ≠ j` and every `p ∈ excl(Aⱼ)`,
//!      alternative `Aᵢ` contains a property shape with `sh:path p` and
//!      `sh:maxCount 0`.
//!
//! `sh:or` and `sh:and` are out of scope: `sh:or` means "at least one", and
//! contamination from an unlisted alternative is inherent to that meaning,
//! not a defect the way it is for `sh:xone`'s "exactly one".

use std::collections::{BTreeSet, HashMap};

use shacl_rust::Shape;
use shacl_rust::core::constraints::Constraint;
use shacl_rust::core::path::PathElement;

use crate::error::Violation;

/// One alternative's positive predicate set, plus enough identity to name
/// it in a violation message.
struct Alternative<'a, 'g> {
    shape: &'a Shape<'g>,
    pos: BTreeSet<String>,
}

fn shape_label(shape: &Shape<'_>) -> String {
    shape.get_name()
}

/// `sh:path` as a single plain IRI (the only path shape any real
/// declaration uses today). Property shapes with a complex path are
/// skipped by the D14 algorithm entirely -- they cannot be named as a
/// single "exclusive predicate" the way the algorithm requires.
fn simple_path_iri(shape: &Shape<'_>) -> Option<String> {
    let path = shape.path.as_ref()?;
    match path.get_elements() {
        [PathElement::Iri(iri)] => Some(iri.as_str().to_string()),
        _ => None,
    }
}

fn has_max_count_zero(shape: &Shape<'_>) -> bool {
    shape
        .constraints
        .iter()
        .any(|c| matches!(c, Constraint::MaxCount(m) if m.0 == 0))
}

fn positive_predicates(alternative: &Shape<'_>) -> BTreeSet<String> {
    alternative
        .property_shapes
        .iter()
        .filter(|p| !has_max_count_zero(p))
        .filter_map(simple_path_iri)
        .collect()
}

/// Does `alternative` already contain a property shape with `sh:path p`
/// and `sh:maxCount 0`?
fn has_exclusion_for(alternative: &Shape<'_>, predicate: &str) -> bool {
    alternative.property_shapes.iter().any(|p| {
        has_max_count_zero(p)
            && simple_path_iri(p)
                .map(|path| path == predicate)
                .unwrap_or(false)
    })
}

/// Runs the D14 cross-exclusion check on every `sh:xone` found among
/// `shapes` (the declaration's own parsed shapes, not the meta-shapes).
/// Returns one [`Violation`] per (alternative, missing predicate) pair, in
/// a deterministic order, so the same input always produces the same
/// message.
pub(crate) fn check(shapes: &[Shape<'_>]) -> Vec<Violation> {
    let mut violations = Vec::new();

    for shape in shapes {
        for constraint in &shape.constraints {
            let Constraint::Xone(xone) = constraint else {
                continue;
            };
            let node_shape_name = shape_label(shape);

            let alternatives: Vec<Alternative<'_, '_>> = xone
                .0
                .iter()
                .map(|alt| Alternative {
                    shape: alt,
                    pos: positive_predicates(alt),
                })
                .collect();

            // excl(Ai) = pos(Ai) \ union of pos(Aj) for j != i
            let mut exclusive: HashMap<usize, BTreeSet<String>> = HashMap::new();
            for (i, ai) in alternatives.iter().enumerate() {
                let mut excl = ai.pos.clone();
                for (j, aj) in alternatives.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    for p in &aj.pos {
                        excl.remove(p);
                    }
                }
                exclusive.insert(i, excl);
            }

            for (j, aj) in alternatives.iter().enumerate() {
                let excl_j = &exclusive[&j];
                if excl_j.is_empty() {
                    continue;
                }
                for (i, ai) in alternatives.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    for predicate in excl_j {
                        if !has_exclusion_for(ai.shape, predicate) {
                            violations.push(Violation::d14_xone(
                                Some(node_shape_name.clone()),
                                format!(
                                    "sh:xone alternative \"{}\" is missing the required \
                                     cross-exclusion for predicate <{predicate}>, which is \
                                     exclusive to alternative \"{}\" -- add \
                                     `sh:property [ sh:path <{predicate}> ; sh:maxCount 0 ]` \
                                     to \"{}\", or the two alternatives can silently accept an \
                                     instance carrying fields from both variants (D8: an \
                                     invisible stale credential).",
                                    shape_label(ai.shape),
                                    shape_label(aj.shape),
                                    shape_label(ai.shape),
                                ),
                            ));
                        }
                    }
                }
            }
        }
    }

    violations
}
