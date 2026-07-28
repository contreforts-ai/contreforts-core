//! The typed view of a validated declaration (Part 4 of
//! contreforts/contreforts-core#14).
//!
//! Built from two sources deliberately, not one:
//!   - `shacl-rust`'s typed `Shape` API for everything SHACL Core itself
//!     models (targets, `sh:path`, `sh:name`, `sh:description`,
//!     `sh:datatype`, `sh:minCount`/`sh:maxCount`).
//!   - Direct graph reads for everything else: the three
//!     `contreforts:`-namespaced predicates (not part of SHACL, so the
//!     validator's typed API has no field for them at all), and `sh:group`
//!     / `sh:order` / `sh:defaultValue` specifically, which contreforts/
//!     contreforts-workspace#21 found are declared by SHACL but *not*
//!     surfaced by any working validator's typed `Shape` -- shacl-rust
//!     included. That is expected, not a workaround: see `presentation()`
//!     below, which runs one SPARQL query over the shapes graph for them.

use std::collections::HashMap;

use oxigraph::model::{NamedNodeRef, NamedOrBlankNodeRef};
use oxigraph::store::Store;
use shacl_rust::Shape;
use shacl_rust::core::constraints::Constraint;
use shacl_rust::core::path::PathElement;
use shacl_rust::core::target::Target;
use shacl_rust::utils::{get_boolean_value, get_string_value};

use crate::error::Violation;

// The three vocabulary.ttl predicates that carry application data SHACL
// has no term for at all, so the typed `Shape` API cannot see them under
// any name -- read directly off the graph instead. Compile-time IRI
// constants, not composed at runtime, so no allocation is needed to hand
// `shacl-rust`'s graph lookups a `NamedNodeRef<'static>`.
const SECRET_PREDICATE: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("https://contreforts.ds-labs.org/ontologies/declaration#secret");
const CATEGORY_PREDICATE: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("https://contreforts.ds-labs.org/ontologies/declaration#category");
const UI_SHAPE_PREDICATE: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("https://contreforts.ds-labs.org/ontologies/declaration#uiShape");
// The fourth vocabulary.ttl predicate (contreforts-core#16, D15), added
// the same way as the three above: SHACL has no term for it, so the typed
// `Shape` API cannot see it either.
const CONFIG_FIELD_PREDICATE: NamedNodeRef<'static> = NamedNodeRef::new_unchecked(
    "https://contreforts.ds-labs.org/ontologies/declaration#configField",
);

/// One `sh:property` value shape, as a connector's `build.rs` or a config
/// form generator would consume it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyShape {
    /// The predicate this field lives under (`sh:path`'s value, when it is
    /// a single plain IRI -- the only path shape any real declaration uses
    /// today).
    pub path: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub datatype: Option<String>,
    pub min_count: Option<i32>,
    pub max_count: Option<i32>,
    /// D8: write-only, must never be served back. Defaults to `false` when
    /// `contreforts:secret` is absent, per the vocabulary's own comment.
    pub secret: bool,
    /// D9: names the registered UI component for a non-trivial field.
    pub ui_shape: Option<String>,
    /// `sh:group`'s value (a `sh:PropertyGroup` IRI), read via SPARQL --
    /// not part of the typed `Shape` API. `None` when the property shape
    /// declares no group.
    pub group: Option<String>,
    /// `sh:order`'s value, read via SPARQL for the same reason as `group`.
    pub order: Option<i64>,
    /// `sh:defaultValue`'s value, serialized as its RDF term (e.g.
    /// `"300"^^<http://www.w3.org/2001/XMLSchema#integer>` or a plain
    /// quoted string) rather than further typed here -- a consumer that
    /// cares about its datatype already has `datatype` above, and neither
    /// real declaration this crate was built against uses
    /// `sh:defaultValue` at all, so this is exercised only by this crate's
    /// own unit test against a synthetic shape (mirroring
    /// contreforts-workspace#27's O365 spike, which found the same thing:
    /// O365ConnectorConfig has no defaulted field for real).
    pub default_value: Option<String>,
    /// `contreforts:configField`'s value (contreforts-core#16, D15):
    /// names the field of the connector's runtime config struct this
    /// property supplies. `None` when the property shape carries no
    /// annotation -- legal, and the common case today: it means this
    /// property is identity, storage-only, or presentation metadata
    /// rather than part of the runtime config surface. See the D15 lints
    /// in `lint::config_field` for the two existential rules this crate
    /// enforces about it that a meta-shape cannot.
    pub config_field: Option<String>,
}

/// A connector's self-description, once it has passed meta-shape
/// validation and the D14/D2 lints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// The node shape's own identifier (an IRI, or a blank node label --
    /// real declarations always use an IRI here, e.g.
    /// `https://contreforts.ds-labs.org/ontologies/o365#O365ConnectorShape`).
    pub shape: String,
    /// `sh:targetClass`'s value: the RDF class this connector's config
    /// instances are `rdf:type` of.
    pub target_class: String,
    /// `contreforts:category`'s value on the connector's node shape.
    /// Present after a successful `validate()`, because meta-shape META-1
    /// requires it -- kept `Option` rather than unwrapped internally so a
    /// meta-shapes bug fails loud (a `Violation`) instead of panicking.
    pub category: Option<String>,
    /// `contreforts:uiShape`'s value on the connector's node shape itself
    /// (not on a property shape) -- present only for a connector whose
    /// non-trivial structure lives at the node level, like O365's
    /// `sh:xone` tagged union. `None` for a connector like Forgejo, whose
    /// only non-trivial field carries its own `contreforts:uiShape`
    /// instead (see that field on `PropertyShape`).
    pub ui_shape: Option<String>,
    /// The connector's own top-level fields: the direct `sh:property`
    /// children of `shape`. Deliberately does NOT walk into `sh:xone`
    /// alternatives or `sh:node`-nested shapes (e.g.
    /// `forgejo:GroupMappingShape`) -- those are validation structure, not
    /// this connector's flat, renderable field list. A form generator that
    /// needs a nested shape's own fields follows `sh:node` itself.
    pub properties: Vec<PropertyShape>,
}

/// Presentation metadata not modeled by SHACL Core's typed `Shape` API,
/// keyed by each property shape's own node (as its `Display`/SPARQL term
/// string -- an IRI in angle brackets, or `_:label` for a blank node).
struct Presentation {
    group: Option<String>,
    order: Option<i64>,
    default_value: Option<String>,
}

fn presentation_by_node(
    graph: &oxigraph::model::Graph,
) -> Result<HashMap<String, Presentation>, Violation> {
    let store = Store::new().map_err(|e| {
        Violation::structural(format!(
            "failed to build an in-memory store for the sh:group/sh:order/sh:defaultValue SPARQL query: {e}"
        ))
    })?;
    for triple in graph.iter() {
        store
            .insert(oxigraph::model::QuadRef::new(
                triple.subject,
                triple.predicate,
                triple.object,
                oxigraph::model::GraphNameRef::DefaultGraph,
            ))
            .map_err(|e| {
                Violation::structural(format!(
                    "failed to load the declaration graph into the SPARQL store: {e}"
                ))
            })?;
    }

    // sh:group, sh:order and sh:defaultValue are ordinary SHACL predicates
    // -- declared by the spec -- but contreforts-workspace#21 found no
    // working validator's typed Shape API surfaces them. This is the one
    // query that reads all three back from the shapes graph directly, as
    // #21's follow-up note said to: "build it that way deliberately".
    let query = r#"
        PREFIX sh: <http://www.w3.org/ns/shacl#>
        SELECT ?prop ?group ?order ?default WHERE {
            ?prop sh:path ?p .
            OPTIONAL { ?prop sh:group ?group }
            OPTIONAL { ?prop sh:order ?order }
            OPTIONAL { ?prop sh:defaultValue ?default }
        }
    "#;

    let prepared = oxigraph::sparql::SparqlEvaluator::new()
        .parse_query(query)
        .map_err(|e| {
            Violation::structural(format!(
                "sh:group/sh:order/sh:defaultValue SPARQL query failed to parse: {e}"
            ))
        })?;

    let mut result = HashMap::new();
    match prepared.on_store(&store).execute() {
        Ok(oxigraph::sparql::QueryResults::Solutions(solutions)) => {
            for solution in solutions {
                let solution = solution.map_err(|e| {
                    Violation::structural(format!(
                        "sh:group/sh:order/sh:defaultValue SPARQL query failed: {e}"
                    ))
                })?;
                let Some(prop) = solution.get("prop") else {
                    continue;
                };
                let key = prop.to_string();
                // sh:group's value is a sh:PropertyGroup IRI in every real
                // use; report the bare IRI (matching `category`/`target_class`
                // elsewhere), not oxigraph's bracketed term syntax.
                let group = solution.get("group").and_then(|t| match t {
                    oxigraph::model::Term::NamedNode(n) => Some(n.as_str().to_string()),
                    _ => None,
                });
                let order = solution.get("order").and_then(|t| match t {
                    oxigraph::model::Term::Literal(lit) => lit.value().parse::<i64>().ok(),
                    _ => None,
                });
                let default_value = solution.get("default").map(|t| t.to_string());
                result.insert(
                    key,
                    Presentation {
                        group,
                        order,
                        default_value,
                    },
                );
            }
        }
        Ok(_) => {
            return Err(Violation::structural(
                "sh:group/sh:order/sh:defaultValue query did not return solutions (expected SELECT)",
            ));
        }
        Err(e) => {
            return Err(Violation::structural(format!(
                "sh:group/sh:order/sh:defaultValue SPARQL query failed to execute: {e}"
            )));
        }
    }
    Ok(result)
}

/// A property shape's `sh:path`, when it is a single plain IRI (the only
/// path shape any real declaration uses today). `None` for anything more
/// exotic (inverse/sequence/alternative paths) -- those have no single
/// predicate to report here.
fn simple_path_iri(shape: &Shape<'_>) -> Option<String> {
    let path = shape.path.as_ref()?;
    match path.get_elements() {
        [PathElement::Iri(iri)] => Some(iri.as_str().to_string()),
        _ => None,
    }
}

fn build_property_shape(
    shape: &Shape<'_>,
    graph: &oxigraph::model::Graph,
    presentation: &HashMap<String, Presentation>,
) -> Option<PropertyShape> {
    let path = simple_path_iri(shape)?;

    let mut datatype = None;
    let mut min_count = None;
    let mut max_count = None;
    for constraint in &shape.constraints {
        match constraint {
            Constraint::Datatype(d) => datatype = Some(d.0.as_str().to_string()),
            Constraint::MinCount(c) => min_count = Some(c.0),
            Constraint::MaxCount(c) => max_count = Some(c.0),
            _ => {}
        }
    }

    let secret = get_boolean_value(graph, shape.node, SECRET_PREDICATE).unwrap_or(false);
    let ui_shape = get_string_value(graph, shape.node, UI_SHAPE_PREDICATE);
    let config_field = get_string_value(graph, shape.node, CONFIG_FIELD_PREDICATE);

    let key = shape.node.to_string();
    let (group, order, default_value) = match presentation.get(&key) {
        Some(p) => (p.group.clone(), p.order, p.default_value.clone()),
        None => (None, None, None),
    };

    Some(PropertyShape {
        path,
        name: shape.name.clone(),
        description: shape.description.clone(),
        datatype,
        min_count,
        max_count,
        secret,
        ui_shape,
        group,
        order,
        default_value,
        config_field,
    })
}

/// The connector's own node shape: the sole `sh:NodeShape` carrying
/// `sh:targetClass`. `validate()` has already run the structural lint
/// requiring at least one to exist, and meta-shape META-1 requiring
/// exactly one `contreforts:category` on it, before this is called -- so
/// this only has to pick one when (in principle) more than one exists,
/// which no real declaration does today. Picks the first in parse order,
/// deterministically, and documents that choice here rather than silently.
fn find_connector_shape<'a, 'g>(shapes: &'a [Shape<'g>]) -> Option<&'a Shape<'g>> {
    shapes
        .iter()
        .find(|s| s.is_node_shape() && s.targets.iter().any(|t| matches!(t, Target::Class(_))))
}

fn target_class_iri(shape: &Shape<'_>) -> Option<String> {
    shape.targets.iter().find_map(|t| match t {
        Target::Class(NamedOrBlankNodeRef::NamedNode(n)) => Some(n.as_str().to_string()),
        _ => None,
    })
}

pub(crate) fn build_declaration(
    shapes: &[Shape<'_>],
    graph: &oxigraph::model::Graph,
) -> Result<Declaration, Violation> {
    let connector_shape = find_connector_shape(shapes).ok_or_else(|| {
        Violation::structural(
            "no sh:NodeShape with sh:targetClass found when building the Declaration -- \
             the structural lint should have already rejected this declaration before \
             reaching model construction",
        )
    })?;

    let target_class = target_class_iri(connector_shape).ok_or_else(|| {
        Violation::structural(format!(
            "connector shape {} has no plain-IRI sh:targetClass",
            connector_shape.node
        ))
    })?;

    let category = get_string_value(graph, connector_shape.node, CATEGORY_PREDICATE);
    let ui_shape = get_string_value(graph, connector_shape.node, UI_SHAPE_PREDICATE);

    let presentation = presentation_by_node(graph)?;
    let properties = connector_shape
        .property_shapes
        .iter()
        .filter_map(|p| build_property_shape(p, graph, &presentation))
        .collect();

    Ok(Declaration {
        shape: connector_shape.node.to_string(),
        target_class,
        category,
        ui_shape,
        properties,
    })
}
