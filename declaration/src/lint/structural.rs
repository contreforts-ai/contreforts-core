//! Graph-wide existence checks that are not expressible as per-node SHACL
//! constraints: SHACL constraints are universal over a *resolved* target
//! set, so an absence of any matching node resolves to zero focus nodes
//! and vacuously conforms -- not a violation. "At least one X exists" has
//! to be asked in Rust instead.

use shacl_rust::Shape;
use shacl_rust::core::target::Target;

use crate::error::Violation;

/// Part 2's first candidate: "a declaration has at least one sh:NodeShape
/// with a sh:targetClass". Confirmed against both real declarations --
/// each has exactly one (o365:O365ConnectorShape,
/// forgejo:ForgejoConnectorShape) -- and required generically: a
/// declaration with none describes no connector at all, and
/// `model::build_declaration` has nothing to build.
pub(crate) fn check(shapes: &[Shape<'_>]) -> Vec<Violation> {
    let has_target_class_shape = shapes
        .iter()
        .any(|s| s.is_node_shape() && s.targets.iter().any(|t| matches!(t, Target::Class(_))));

    if has_target_class_shape {
        Vec::new()
    } else {
        vec![Violation::structural(
            "declaration defines no sh:NodeShape carrying sh:targetClass -- \
             a declaration must say what RDF class it targets",
        )]
    }
}
