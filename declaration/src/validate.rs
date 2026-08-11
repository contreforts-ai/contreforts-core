//! Orchestrates Part 2 (meta-shapes) and Part 3 (the D14/D2/structural/D15/
//! entityKind lints) into the single [`crate::validate`] entry point Part 4
//! asks for.

use shacl_rust::parser::parse_shapes;
use shacl_rust::rdf::read_graph_from_string;
use shacl_rust::validation::dataset::ValidationDataset;
use shacl_rust::validation::report::ValidationResult;
use shacl_rust::validation::validate as shacl_validate;

use crate::error::{Violation, Violations};
use crate::lint;
use crate::model::{self, Declaration};

const META_SHAPES_TTL: &str = include_str!("meta_shapes.ttl");

/// Core's canonical SKOS concept scheme (contreforts/contreforts-workspace#83, E2). Unioned
/// into [`run_meta_shapes`]'s own data graph, alongside the declaration graph, so a
/// connector's `skos:exactMatch core:customer` resolves against a real `skos:Concept`
/// definition instead of a stub the connector wrote itself.
///
/// Deliberately NOT unioned into `run_pipeline`'s own `declaration_graph` variable: that graph
/// is also what `lint::core_ns::check` and `lint::entity_kind::check` (Part 3) run against, and
/// neither is unioned with anything else (verified by reading this file end to end before
/// wiring this in -- contreforts/contreforts-workspace#83's own instruction). If core's 15
/// concept-defining triples reached that graph, they would look exactly like a connector
/// defining a `core:`-namespaced `skos:Concept`, and the D2 carve-out tightened by this same
/// issue (reject a `core:` subject that is DEFINED, not just referenced) would reject every
/// declaration outright.
const CONCEPTS_TTL: &str = include_str!("concepts.ttl");

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
/// structural, D15 (`contreforts:configField`), entityKind
/// (`contreforts:entityKind` duplicate-value, contreforts-kg#30) and D8
/// (`contreforts:writeIntent` create/update coverage,
/// contreforts-workspace#19) lints
/// from Part 3, all run to completion and combined --
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
    run_pipeline(turtle, model::build_declaration)
}

/// The per-kind alternative to [`validate`] (contreforts/contreforts-config-api#27 item 3):
/// runs the identical Part 2/Part 3 pipeline, but returns one [`Declaration`] per qualifying
/// `sh:NodeShape` in `turtle` instead of aggregating them into one. For a `turtle` describing
/// exactly one connector -- every real declaration.ttl in isolation -- this returns a
/// single-element `Vec` equivalent to [`validate`]'s own `Declaration`. For a graph unioning
/// several connectors' own shapes (`contreforts-product`'s `PRODUCT_GRAPH_TTL` is the only one
/// today), this is what a caller that needs the result split back out by connector kind should
/// use -- `validate`'s own `Declaration` cannot express that; see its own doc comment.
pub fn declarations(turtle: &str) -> Result<Vec<Declaration>, Violations> {
    run_pipeline(turtle, model::build_declarations)
}

/// [`declarations`], plus each connector's own [`model::NodeShapeFacts`] --
/// the identical Part 2/Part 3 pipeline, run once, over the same parse
/// (contreforts/contreforts-core#34, phase F item W4).
///
/// Crate-internal, and the reason [`crate::form_schemas`] does not have to
/// re-read the Turtle a second time to see `sh:closed`, `sh:deactivated`, the
/// node-level constraint list, or the `sh:property`/`sh:xone` counts that
/// `Declaration` does not carry: re-parsing would mean re-deriving which
/// parsed `Shape` belongs to which `Declaration` by IRI, and a join that
/// silently misses is precisely the failure mode W4 exists to make
/// impossible. See `model::NodeShapeFacts`.
pub(crate) fn declarations_with_facts(
    turtle: &str,
) -> Result<Vec<(Declaration, model::NodeShapeFacts)>, Violations> {
    run_pipeline(turtle, model::build_declarations_with_facts)
}

/// Shared by [`validate`] and [`declarations`]: Part 2 (meta-shapes.ttl) then Part 3 (the
/// D14/D2/structural lints), run to completion and combined exactly as `validate`'s own doc
/// comment describes, before handing the parsed shapes and graph to `build` -- either
/// `model::build_declaration` or `model::build_declarations`, so both entry points share one
/// pipeline rather than drifting apart under separate maintenance.
fn run_pipeline<T>(
    turtle: &str,
    build: impl FnOnce(&[shacl_rust::Shape<'_>], &oxigraph::model::Graph) -> Result<T, Violation>,
) -> Result<T, Violations> {
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

    // Part 3: the D14/D2/structural/D15 lints, over the declaration's own
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
        // D8 amended (contreforts/contreforts-workspace#19, 2026-08-11). Runs over the same
        // `own_shapes`/`declaration_graph` pair as the two lints above and for the same reason
        // they are Rust rather than SHACL -- the rule is about the *other* shapes targeting one
        // class, which no per-node constraint can see. Note that it is the only lint here whose
        // absence is silently permissive rather than silently restrictive: a missing update
        // shape does not reject updates, it accepts them all unchecked. See
        // `lint::write_intent`.
        violations.extend(lint::write_intent::check(shapes, &declaration_graph));
    }
    violations.extend(lint::core_ns::check(&declaration_graph));
    violations.extend(lint::entity_kind::check(&declaration_graph));

    if !violations.is_empty() {
        return Err(Violations::new(violations));
    }

    // Every check passed -- own_shapes is Some (the structural lint above
    // would already have failed otherwise, since it required a parse to
    // even run) and there's at least one sh:NodeShape with sh:targetClass.
    let shapes = own_shapes.expect("own_shapes is Some when no violations were collected");
    match build(&shapes, &declaration_graph) {
        Ok(result) => Ok(result),
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

    let concepts_graph = read_graph_from_string(CONCEPTS_TTL, "turtle").map_err(|e| {
        Violation::structural(format!(
            "the crate's own concepts.ttl failed to parse -- this is a bug in \
             contreforts-declaration itself, not in the declaration being validated: {e}"
        ))
    })?;

    // The data graph for THIS validation only: the declaration plus core's own concept
    // scheme, so a connector's skos:exactMatch/closeMatch/inScheme resolves against a real
    // skos:Concept. `declaration_graph` itself is left untouched -- Part 3's `core_ns` and
    // `entity_kind` lints (run_pipeline, below) must never see concepts.ttl's own triples, or
    // core's 15 concepts would look like a connector-defined core: subject. See CONCEPTS_TTL's
    // own doc comment above for why.
    let mut data_graph = declaration_graph.clone();
    for triple in concepts_graph.iter() {
        data_graph.insert(triple);
    }

    // Cloned, not moved: `meta_shapes` above borrows `meta_shapes_graph`,
    // and both it and the dataset need to be alive at once for
    // `shacl_validate` below (mirrors the spike's own
    // `ValidationDataset::from_graphs(data_graph, shapes_graph.clone())`).
    let dataset =
        ValidationDataset::from_graphs(data_graph, meta_shapes_graph.clone()).map_err(|e| {
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
