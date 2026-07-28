//! Orchestrates Part 2 (meta-shapes) and Part 3 (the D14/D2/structural
//! lints) into the single [`crate::validate`] entry point Part 4 asks for.

use shacl_rust::parser::parse_shapes;
use shacl_rust::rdf::read_graph_from_string;
use shacl_rust::validation::dataset::ValidationDataset;
use shacl_rust::validation::report::ValidationResult;
use shacl_rust::validation::validate as shacl_validate;

use crate::error::{Violation, Violations};
use crate::lint;
use crate::model::{self, Declaration};

const META_SHAPES_TTL: &str = include_str!("meta_shapes.ttl");

fn violation_from_meta_shape_result(result: &ValidationResult<'_>) -> Violation {
    let focus = result.focus_node().to_string();
    let mut message = if !result.messages().is_empty() {
        result.messages().join("; ")
    } else if let Some(detail) = result.constraint_detail() {
        detail.to_string()
    } else {
        format!(
            "meta-shape violation (source shape {})",
            result.source_shape()
        )
    };
    if let Some(path) = result.result_path() {
        message = format!("{message} (at path {path})");
    }
    Violation::meta_shape(Some(focus), message)
}

/// Validates `turtle` as a connector declaration: Part 2's meta-shapes,
/// then the D14 (`sh:xone` cross-exclusion), D2 (`core:` namespace),
/// structural and D15 (`contreforts:configField`) lints from Part 3, all
/// run to completion and combined --
/// never short-circuited on the first failure, per Part 4's requirement
/// that both real callers (a connector's `build.rs`, and the C3
/// aggregator's `build.rs`) see every violation, not just the first.
///
/// Malformed Turtle is reported the same way, as a single [`Violation`] in
/// the returned [`Violations`] -- not a panic surfacing from inside
/// `shacl-rust` or `oxigraph`.
///
/// Note on the signature: the issue's Part 4 sketches this as
/// `Result<Declaration, Vec<Violation>>`. This crate returns
/// `Result<Declaration, Violations>` instead -- a thin newtype wrapper
/// around exactly that `Vec<Violation>` (see [`Violations::into_vec`] to
/// get it back) -- because `Vec<Violation>` cannot itself implement
/// `Display` (`Vec` and `Display` are both foreign to this crate), and
/// Part 4 explicitly asks for "the `Display` of a violation set" to be
/// something a `panic!` can print directly:
/// `panic!("{}", declaration.unwrap_err())`.
pub fn validate(turtle: &str) -> Result<Declaration, Violations> {
    let declaration_graph = match read_graph_from_string(turtle, "turtle") {
        Ok(graph) => graph,
        Err(e) => {
            return Err(Violations::new(vec![Violation::turtle(format!(
                "the declaration is not valid Turtle: {e}"
            ))]));
        }
    };

    let mut violations = Vec::new();

    // Part 2: meta-shapes.ttl, with the declaration as data.
    match run_meta_shapes(&declaration_graph) {
        Ok(meta_violations) => violations.extend(meta_violations),
        Err(v) => violations.push(v),
    }

    // Part 3: the D14/D2/structural lints, over the declaration's own
    // parsed shapes.
    let own_shapes = match parse_shapes(&declaration_graph) {
        Ok(shapes) => Some(shapes),
        Err(e) => {
            violations.push(Violation::structural(format!(
                "declaration's own shapes graph could not be parsed as SHACL: {e}"
            )));
            None
        }
    };

    if let Some(shapes) = &own_shapes {
        violations.extend(lint::structural::check(shapes));
        violations.extend(lint::xone::check(shapes));
        violations.extend(lint::config_field::check(shapes, &declaration_graph));
    }
    violations.extend(lint::core_ns::check(&declaration_graph));

    if !violations.is_empty() {
        return Err(Violations::new(violations));
    }

    // Every check passed -- own_shapes is Some (the structural lint above
    // would already have failed otherwise, since it required a parse to
    // even run) and there's at least one sh:NodeShape with sh:targetClass.
    let shapes = own_shapes.expect("own_shapes is Some when no violations were collected");
    match model::build_declaration(&shapes, &declaration_graph) {
        Ok(declaration) => Ok(declaration),
        Err(v) => Err(Violations::new(vec![v])),
    }
}

fn run_meta_shapes(
    declaration_graph: &oxigraph::model::Graph,
) -> Result<Vec<Violation>, Violation> {
    let meta_shapes_graph = read_graph_from_string(META_SHAPES_TTL, "turtle").map_err(|e| {
        Violation::structural(format!(
            "the crate's own meta_shapes.ttl failed to parse -- this is a bug in \
             contreforts-declaration itself, not in the declaration being validated: {e}"
        ))
    })?;
    let meta_shapes = parse_shapes(&meta_shapes_graph).map_err(|e| {
        Violation::structural(format!(
            "the crate's own meta_shapes.ttl is not well-formed SHACL -- this is a bug in \
             contreforts-declaration itself: {e}"
        ))
    })?;

    // Cloned, not moved: `meta_shapes` above borrows `meta_shapes_graph`,
    // and both it and the dataset need to be alive at once for
    // `shacl_validate` below (mirrors the spike's own
    // `ValidationDataset::from_graphs(data_graph, shapes_graph.clone())`).
    let dataset =
        ValidationDataset::from_graphs(declaration_graph.clone(), meta_shapes_graph.clone())
            .map_err(|e| {
                Violation::structural(format!(
                    "failed to build the meta-shapes validation dataset: {e}"
                ))
            })?;

    let report = shacl_validate(&dataset, &meta_shapes);
    Ok(report
        .get_results()
        .iter()
        .map(violation_from_meta_shape_result)
        .collect())
}
