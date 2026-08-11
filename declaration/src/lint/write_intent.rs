//! D8's `contreforts:writeIntent` checks that no meta-shape can express
//! (contreforts/contreforts-workspace#19, "D8 amended -- 2026-08-11").
//!
//! One rule, and it is existential in the way this crate's other Rust lints are: **a class that
//! has a shape for one write intent must have one for the other**. A SHACL constraint is
//! universally quantified over its resolved target set, so an intent with *no* applicable shape
//! resolves to zero focus nodes and conforms -- vacuously, for every instance. That is the
//! failure the two-shape design introduces: a declaration that writes its create shape and
//! forgets its update shape does not reject updates, it accepts every one of them unchecked.
//! Exactly the class of rule `lint::structural` and `lint::config_field` already exist for; see
//! `lint/mod.rs`.
//!
//! The value check (`"create"`/`"update"`, at most one) is *also* run here, and is *also* a
//! meta-shape (META-6). That overlap is deliberate and is argued at `write_intent.rs`'s
//! `scope_of`: `ConnectorValidator` never runs the meta-shapes, so the Rust reader has to reject
//! a bad value on its own account, and a lint that shares the reader gets the check for free.
//! A connector's `build.rs` therefore sees one mistake reported twice -- accepted, over a server
//! that starts with a shape scoped to something it could not parse.

use oxigraph::model::Graph;
use shacl_rust::Shape;

use crate::error::Violation;
use crate::write_intent;

/// Runs both `contreforts:writeIntent` checks over the declaration's own parsed shapes and
/// graph (the graph is needed for the same reason `lint::config_field` needs it:
/// `contreforts:writeIntent` is not a SHACL term, so `shacl-rust`'s typed `Shape` has no field
/// for it).
pub(crate) fn check(shapes: &[Shape<'_>], graph: &Graph) -> Vec<Violation> {
    let mut violations = Vec::new();

    for shape in shapes {
        if let Err(message) = write_intent::scope_of(graph, shape.node) {
            violations.push(Violation::d8_write_intent(Some(shape.get_name()), message));
        }
    }

    for gap in write_intent::coverage_gaps(shapes, graph) {
        violations.push(Violation::d8_write_intent(
            Some(gap.class_iri.clone()),
            gap.message(),
        ));
    }

    violations
}
