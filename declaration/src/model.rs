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

use oxigraph::model::vocab::{rdf, rdfs};
use oxigraph::model::{NamedNodeRef, NamedOrBlankNodeRef, TermRef};
use oxigraph::store::Store;
use shacl_rust::Shape;
use shacl_rust::core::constraints::{Constraint, NodeKind};
use shacl_rust::core::path::PathElement;
use shacl_rust::core::target::Target;
use shacl_rust::utils::{get_boolean_value, get_integer_value, get_string_value};
use shacl_rust::vocab::sh;

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
    /// `sh:in`'s value list (contreforts/contreforts-config-api#27 item 1):
    /// the property's enumerated legal values, in declaration order, e.g.
    /// `["basic", "bearer"]` for caldav's `authMode`. `None` when the
    /// property shape carries no `sh:in` -- legal, and the common case.
    /// Previously dropped entirely by the constraint match's `_ => {}`
    /// arm; a form generator has no way to render a picker without it.
    pub in_values: Option<Vec<String>>,
    /// `sh:pattern`'s value (contreforts/contreforts-config-api#27 item 1),
    /// e.g. forgejo:instanceUrl's `"^https?://[^\\s]+$"`. `None` when
    /// absent. Same previously-dropped-by-`_ => {}` defect as `in_values`.
    pub pattern: Option<String>,
    /// `sh:minInclusive`'s value, as its literal's lexical form (e.g.
    /// `"1"`) rather than a parsed number -- this crate does not know
    /// whether a given property's range is integer, decimal or something
    /// else, and re-parsing it here would silently narrow that. A caller
    /// that wants a typed number (a JSON response, say) parses this
    /// itself, knowing its own target type. `None` when absent. Same
    /// previously-dropped-by-`_ => {}` defect as `in_values`/`pattern`.
    pub min_inclusive: Option<String>,
    /// `sh:maxInclusive`'s value, same shape and reasoning as
    /// `min_inclusive` above.
    pub max_inclusive: Option<String>,
    /// `sh:minLength`'s value (contreforts/contreforts-workspace#60, phase F
    /// item W1), e.g. forgejo:label's real `sh:minLength 1`. `None` when
    /// absent. Same previously-dropped-by-`_ => {}` defect as `in_values`/
    /// `pattern`/`min_inclusive`/`max_inclusive` above.
    ///
    /// `shacl_rust::core::constraints::MinLengthConstraint` wraps an `i32`,
    /// not a `u32` -- SHACL's own spec requires `sh:minLength` to be a
    /// non-negative integer, but shacl-rust's parser does not itself
    /// enforce that at parse time, so a hand-authored graph could still
    /// carry a negative one. Rather than `as u32`, which would silently
    /// wrap a negative value into some large, wrong, unsigned number, the
    /// constraint match below uses `u32::try_from`, which fails (leaving
    /// this `None`) instead. No real declaration has ever carried a
    /// negative `sh:minLength` (it would be invalid SHACL), so this is
    /// exercised only in reasoning, not by any fixture.
    pub min_length: Option<u32>,
    /// `sh:nodeKind`'s value (contreforts/contreforts-workspace#60, phase F
    /// item W1), reported as the full SHACL vocabulary IRI (e.g.
    /// `"http://www.w3.org/ns/shacl#Literal"`), matching how `datatype`
    /// and `category` etc. are reported elsewhere in this struct. `None`
    /// when absent. Same previously-dropped-by-`_ => {}` defect as
    /// `min_length` above.
    ///
    /// `shacl_rust::core::constraints::NodeKindConstraint` wraps its own
    /// `NodeKind` enum, not the IRI it was parsed from -- the library's own
    /// parser (`parser/constraints/node_kind.rs`) matches the raw term
    /// against `shacl_rust::vocab::sh::{IRI,LITERAL,..}` and then discards
    /// it, keeping only the enum variant. `node_kind_iri` below maps the
    /// enum back to a string through those same `sh::` constants, rather
    /// than hardcoding the IRIs a second time.
    pub node_kind: Option<String>,
    /// The discriminant name (e.g. `"MaxLength"`) of every constraint this
    /// property shape carries that this crate has not grown a dedicated
    /// field for (contreforts/contreforts-workspace#60, phase F item W1).
    /// Empty when every constraint present has a field above.
    ///
    /// This is the point of W1, not incidental to it: previously, an
    /// unrecognised SHACL construct fell through the constraint match's
    /// bare `_ => {}` arm and vanished with no trace at all -- exactly the
    /// "absence presenting as success" defect class this epic exists to
    /// close. W4's later *total* digest depends on being able to name a
    /// dropped construct here rather than silently rendering an
    /// incomplete form.
    pub unhandled: Vec<String>,
}

/// Maps a parsed `sh:nodeKind` value back to the SHACL vocabulary IRI it
/// was parsed from. `shacl_rust`'s `NodeKind` enum (unlike, say, its
/// `DatatypeConstraint`, which keeps the original `NamedNodeRef`) carries no
/// field or method exposing that IRI -- only a `Display` impl that prints
/// the bare variant name ("Literal", "IRI", ...), not a usable vocabulary
/// term. This maps back through the exact same `shacl_rust::vocab::sh::*`
/// constants the library's own parser
/// (`parser/constraints/node_kind.rs::parse_node_kind`) matched the
/// original term against, so it stays in lockstep with upstream's own
/// vocabulary rather than hardcoding IRI strings a second time.
fn node_kind_iri(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::IRI => sh::IRI.as_str(),
        NodeKind::BlankNode => sh::BLANK_NODE.as_str(),
        NodeKind::Literal => sh::LITERAL.as_str(),
        NodeKind::BlankNodeOrIRI => sh::BLANK_NODE_OR_IRI.as_str(),
        NodeKind::BlankNodeOrLiteral => sh::BLANK_NODE_OR_LITERAL.as_str(),
        NodeKind::IRIOrLiteral => sh::IRI_OR_LITERAL.as_str(),
    }
}

/// The discriminant name of a `Constraint` variant this crate has not
/// grown a dedicated `PropertyShape` field for (contreforts/
/// contreforts-workspace#60, phase F item W1's own point -- see
/// `PropertyShape::unhandled`'s doc comment).
///
/// Every `Constraint` variant wraps exactly one inner value (e.g.
/// `MaxLength(MaxLengthConstraint(10))`), so its derived `Debug` output is
/// always `"VariantName(...)"`. Taking the text before the first `(`
/// yields the bare variant name without an exhaustive match this crate
/// would otherwise have to keep in sync by hand every time shacl-rust
/// grows a new constraint kind -- itself a form of the very "silently
/// missing" failure mode this field exists to prevent, just moved from
/// runtime data into unreviewed source.
fn constraint_variant_name(constraint: &Constraint<'_>) -> String {
    let debug = format!("{constraint:?}");
    match debug.split_once('(') {
        Some((name, _)) => name.to_string(),
        None => debug,
    }
}

/// One `sh:xone` alternative of a connector whose node shape carries a
/// tagged union (contreforts/contreforts-config-api#27 item 2; today, that
/// is exactly caldav's and o365's own auth unions, both flagged with
/// `contreforts:uiShape "discriminated-union"` on the connector's node
/// shape). `Declaration.properties` deliberately never walks into
/// `sh:xone` -- see its own doc comment -- so this is the one place an
/// alternative's own field set, and its own real `sh:minCount`
/// (invisible at the flat level; see `Declaration.properties`'s doc
/// comment and `merge_variant_requiredness` below), are exposed at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationVariant {
    /// The `sh:path` of the discriminator property itself (e.g.
    /// `"https://contreforts.ds-labs.org/ontologies/caldav#authMode"`) --
    /// the property `discriminant_value` below was read off, reported as a
    /// full IRI exactly like [`PropertyShape::path`]
    /// (contreforts/contreforts-core#34, phase F item W4).
    ///
    /// Recorded here rather than left for a consumer to re-derive: the only
    /// other way to recover it is to scan `properties` for the one whose
    /// own `in_values` happens to be a single-element list equal to
    /// `discriminant_value`, which is a *guess* that silently picks the
    /// wrong property the moment two of an alternative's fields are both
    /// pinned to one value -- and which cannot recover it at all if the
    /// discriminator also carries `sh:maxCount 0` and is therefore filtered
    /// out of `properties` (see that field's own doc comment). Since
    /// `discriminant_value` is already read off exactly one property shape
    /// in `discriminant_value_and_path`, naming that property here costs
    /// nothing and removes the guess.
    pub discriminant_path: String,
    /// The single legal value of the discriminator property (e.g.
    /// caldav:authMode) that selects this alternative -- read off the
    /// alternative's own copy of that property, which restricts `sh:in`
    /// to exactly that one value (e.g. `caldav:BasicAuthShape` restricts
    /// `authMode` to `sh:in ( "basic" )`). Every real `sh:xone` alternative
    /// this crate has seen carries exactly one such property; an
    /// alternative with none is dropped rather than fabricating a
    /// discriminant (see `build_variants`).
    pub discriminant_value: String,
    /// This alternative's own `sh:property` children, built the same way
    /// as `Declaration.properties`, MINUS any that carry `sh:maxCount 0`
    /// -- D14's own cross-exclusion marker for "belongs to a sibling
    /// alternative, forbidden here" (`lint::xone`), not a real field of
    /// this variant.
    pub properties: Vec<PropertyShape>,
}

/// One `sh:PropertyGroup` referenced by `sh:group` on a connector's own
/// top-level property shapes (contreforts/contreforts-core#33, phase F
/// item W2). Resolved against the graph for its own `rdfs:label` and
/// `sh:order` the same way `category`/`ui_shape`/`config_field` already
/// are -- a direct predicate read off a known subject
/// (`shacl_rust::utils::get_string_value`/`get_integer_value`), not
/// SPARQL. `presentation_by_node`'s existing SPARQL query is what
/// surfaces *which* IRI a property's own `sh:group` names in the first
/// place (unchanged by this addition; see that function's own doc
/// comment) -- this type is what that IRI resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupDescriptor {
    /// The `sh:PropertyGroup`'s IRI, as a bare IRI (matching `category`/
    /// `target_class` elsewhere in this module), not oxigraph's bracketed
    /// term syntax.
    pub iri: String,
    /// The group's own `rdfs:label`. `None` when it declares none (the
    /// totality case: a group typed `a sh:PropertyGroup` with no
    /// presentation text at all -- see
    /// `synthetic-minimal-declaration.ttl`'s `synth:ConnectionGroup`).
    pub label: Option<String>,
    /// The group's own `sh:order`. `None` when it declares none.
    ///
    /// `shacl_rust::utils::get_integer_value` returns `Option<i32>`, but
    /// this field is `Option<i64>` to match `PropertyShape::order`
    /// (populated via SPARQL, which parses the literal straight to
    /// `i64`). Widened with `i64::from` rather than re-read as a raw
    /// literal a second time: `i32 -> i64` is total and lossless, unlike
    /// `PropertyShape::min_length`'s `u32::try_from`, which guards a real
    /// narrowing that can fail. Recorded here per the issue's own
    /// "Unsettled" item 1, rather than left to drift silently.
    pub order: Option<i64>,
    /// `false` when this IRI is referenced by an `sh:group` but never
    /// itself typed `a sh:PropertyGroup` anywhere in the graph -- a
    /// dangling reference. Before this field existed, such a reference
    /// left no trace anywhere in `Declaration` at all: exactly the
    /// "absence presenting as success" defect class this epic exists to
    /// close (contreforts/contreforts-workspace#19, comment 6689). `true`
    /// for every one of the 17 real `sh:PropertyGroup` subjects tree-wide
    /// today; only `declaration/tests/fixtures/dangling-group-reference.ttl`
    /// (synthetic) exercises `false`.
    pub declared: bool,
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
    /// The node shape's own `sh:name`, falling back to `rdfs:label` when
    /// `sh:name` is absent (contreforts/contreforts-core#33, phase F item
    /// W2) -- exactly `shacl_rust`'s own precedence for `Shape.name`
    /// (`apply_common_shape_properties`, shacl-rust-0.2.9
    /// `src/parser/mod.rs:110-114`), read straight off the already-parsed
    /// typed `Shape`, not via a second graph read or SPARQL. `None` for
    /// every connector node shape on disk today (all seven carry
    /// neither); W3 (gated on this issue) is what adds `rdfs:label` to
    /// them, and `sh:name` would win over it if a future declaration ever
    /// carried both (see `declaration/tests/fixtures/node-shape-name-vs-label.ttl`).
    pub label: Option<String>,
    /// The node shape's own `sh:description`, read the same way as
    /// `label` above (`Shape.description`, already parsed by
    /// `shacl_rust`). `None` for every connector node shape on disk
    /// today; W3 is what fills it in.
    pub description: Option<String>,
    /// Every `sh:PropertyGroup` referenced by `sh:group` on this
    /// connector's own top-level `properties` (contreforts/
    /// contreforts-core#33, phase F item W2), deduplicated by IRI --
    /// never by label: only 10 distinct label strings cover the 17 real
    /// groups tree-wide ("Connection" alone appears 6 times), so a
    /// label-keyed set would silently under-count. Sorted by `(order,
    /// iri)` with a `None` order sorted last, so the ordering is total
    /// and deterministic regardless of declaration or fixture order. Also
    /// walks `variants[].properties` (contreforts-core#35): an `sh:xone`
    /// alternative's own fields resolve `.group` through the same
    /// presentation map the flat path uses, so a variant-only `sh:group`
    /// still gets a `GroupDescriptor` here, unioned by IRI with the flat
    /// set -- see `build_groups`.
    pub groups: Vec<GroupDescriptor>,
    /// The connector's own top-level fields: the direct `sh:property`
    /// children of `shape`. Deliberately does NOT walk into `sh:xone`
    /// alternatives or `sh:node`-nested shapes (e.g.
    /// `forgejo:GroupMappingShape`) -- those are validation structure, not
    /// this connector's flat, renderable field list. A form generator that
    /// needs a nested shape's own fields follows `sh:node` itself.
    ///
    /// A field whose requiredness genuinely depends on which `sh:xone`
    /// alternative applies (e.g. caldav:password, required only under
    /// `authMode="basic"`) still appears here with its real `sh:minCount`
    /// folded in from the one alternative that declares it, when exactly
    /// one alternative does (see `merge_variant_requiredness`) -- not the
    /// flat restatement's own, deliberately absent, `sh:minCount`.
    pub properties: Vec<PropertyShape>,
    /// This connector's own `sh:xone` alternatives, if its node shape
    /// declares one (contreforts/contreforts-config-api#27 item 2) --
    /// empty for a connector with no tagged union. See
    /// [`DeclarationVariant`].
    pub variants: Vec<DeclarationVariant>,
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

/// Whether `node` is itself typed `a sh:PropertyGroup` anywhere in
/// `graph` -- `GroupDescriptor::declared`'s own check. A direct predicate
/// read, not SPARQL: `rdf:type` is an ordinary graph lookup, the same
/// shape as `category`/`ui_shape` elsewhere in this module.
fn is_declared_property_group(graph: &oxigraph::model::Graph, node: NamedNodeRef<'_>) -> bool {
    graph
        .objects_for_subject_predicate(node, rdf::TYPE)
        .any(|term| matches!(term, TermRef::NamedNode(n) if n == sh::PROPERTY_GROUP))
}

/// One `GroupDescriptor`, resolved against `graph` for `iri`'s own
/// `rdfs:label`, `sh:order`, and whether it is itself typed `a
/// sh:PropertyGroup` -- see `GroupDescriptor`'s own doc comment for why
/// none of this needs SPARQL.
fn build_group_descriptor(graph: &oxigraph::model::Graph, iri: &str) -> GroupDescriptor {
    let node = NamedNodeRef::new_unchecked(iri);
    let label = get_string_value(graph, node.into(), rdfs::LABEL);
    let order = get_integer_value(graph, node.into(), sh::ORDER).map(i64::from);
    let declared = is_declared_property_group(graph, node);
    GroupDescriptor {
        iri: iri.to_string(),
        label,
        order,
        declared,
    }
}

/// `Declaration.groups`: every distinct IRI named by `sh:group` on either
/// `properties` (the connector's own flat, top-level field list) or any
/// `sh:xone` variant's own `properties` (contreforts-core#35) -- unioned
/// by IRI across both so a group referenced only inside a variant
/// alternative's own copy of a property still gets a `GroupDescriptor`,
/// rather than being a dangling `PropertyShape.group` nothing in
/// `Declaration.groups` can resolve a label/order for. Deduplicated by
/// IRI, in first-seen order (flat properties first, then each variant in
/// order), then sorted by `(order, iri)` with a `None` order sorted last
/// -- comparing `(order.is_none(), order, iri)` rather than deriving
/// `Ord` on `Option<i64>` directly, because `Option`'s derived order puts
/// `None` *first* (`None < Some(_)`), which is the wrong end (see
/// `declaration/tests/fixtures/group-ordering-none-last.ttl`, built
/// specifically to catch that).
fn build_groups(
    graph: &oxigraph::model::Graph,
    properties: &[PropertyShape],
    variants: &[DeclarationVariant],
) -> Vec<GroupDescriptor> {
    let mut seen = std::collections::HashSet::new();
    let mut groups: Vec<GroupDescriptor> = Vec::new();
    for property in properties
        .iter()
        .chain(variants.iter().flat_map(|v| v.properties.iter()))
    {
        let Some(iri) = property.group.as_deref() else {
            continue;
        };
        if !seen.insert(iri.to_string()) {
            continue;
        }
        groups.push(build_group_descriptor(graph, iri));
    }
    sort_groups(&mut groups);
    groups
}

/// The one comparator both `build_groups` and `build_declaration`'s cross-shape fold sort by:
/// `(order, iri)` with a `None` order sorted last -- comparing `(order.is_none(), order, iri)`
/// rather than deriving `Ord` on `Option<i64>` directly, because `Option`'s derived order puts
/// `None` *first* (`None < Some(_)`), which is the wrong end (see
/// `declaration/tests/fixtures/group-ordering-none-last.ttl`, built specifically to catch
/// that). Factored out to one function, called from both sites, so a future change to the
/// ordering rule cannot drift the two apart, and so a test that exercises only one call site
/// (e.g. `validate`'s single-shape path, which also runs `build_declaration`'s own re-sort
/// unconditionally after a no-op fold) still exercises the same code a `declarations`-only
/// caller depends on -- rather than two independently-maintained copies of the same
/// three-tuple, where a bug in one could go uncaught by tests that only ever exercise the
/// other.
fn sort_groups(groups: &mut [GroupDescriptor]) {
    groups.sort_by(|a, b| {
        (a.order.is_none(), a.order, &a.iri).cmp(&(b.order.is_none(), b.order, &b.iri))
    });
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

/// A single RDF term's lexical value, for the two constraint kinds this
/// module reads a raw term out of (`sh:in`'s list members, `sh:minInclusive`
/// / `sh:maxInclusive`'s bound). Mirrors `shacl-rust`'s own
/// `get_string_value` (`utils.rs`), which does the same for a predicate
/// read directly off the graph -- this is the term-level equivalent for a
/// constraint already parsed into a `TermRef`.
fn term_literal_value(term: &TermRef<'_>) -> Option<String> {
    match term {
        TermRef::Literal(lit) => Some(lit.value().to_string()),
        TermRef::NamedNode(n) => Some(n.as_str().to_string()),
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
    let mut in_values = None;
    let mut pattern = None;
    let mut min_inclusive = None;
    let mut max_inclusive = None;
    let mut min_length = None;
    let mut node_kind = None;
    let mut unhandled = Vec::new();
    for constraint in &shape.constraints {
        match constraint {
            Constraint::Datatype(d) => datatype = Some(d.0.as_str().to_string()),
            Constraint::MinCount(c) => min_count = Some(c.0),
            Constraint::MaxCount(c) => max_count = Some(c.0),
            // contreforts/contreforts-config-api#27 item 1: these four
            // constraint kinds used to fall through to `_ => {}` here and
            // vanish -- `PropertyShape` had no field to hold any of them.
            Constraint::In(values) => {
                in_values = Some(
                    values
                        .0
                        .iter()
                        .filter_map(term_literal_value)
                        .collect::<Vec<_>>(),
                );
            }
            Constraint::Pattern(p) => pattern = Some(p.pattern.clone()),
            Constraint::MinInclusive(term) => min_inclusive = term_literal_value(&term.0),
            Constraint::MaxInclusive(term) => max_inclusive = term_literal_value(&term.0),
            // contreforts/contreforts-workspace#60, phase F item W1: same
            // previously-dropped-by-`_ => {}` defect as the four above.
            // See `min_length`'s doc comment on `PropertyShape` for why
            // this is `u32::try_from`, not `as u32`.
            Constraint::MinLength(c) => min_length = u32::try_from(c.0).ok(),
            Constraint::NodeKind(c) => node_kind = Some(node_kind_iri(c.0).to_string()),
            // W1's own point (see `PropertyShape::unhandled`'s doc
            // comment): every other constraint kind still has no field to
            // hold it, but it is no longer silently discarded -- it is
            // named here instead.
            other => unhandled.push(constraint_variant_name(other)),
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
        in_values,
        pattern,
        min_inclusive,
        max_inclusive,
        min_length,
        node_kind,
        unhandled,
    })
}

/// Every `sh:NodeShape` carrying `sh:targetClass` in `shapes` -- one per
/// connector a graph describes. `validate()` has already run the
/// structural lint requiring at least one to exist before this is called.
///
/// contreforts/contreforts-config-api#27 item 3: this used to be
/// `find_connector_shape` (singular), `.find()`-ing only the first such
/// shape and documenting that as safe "because no real declaration [had]
/// more than one" -- true only as long as every caller validated one
/// connector's own `declaration.ttl` in isolation. `PRODUCT_GRAPH_TTL`,
/// the union of every enabled connector's own declaration, is the first
/// graph with more than one: calling the old, singular function against
/// it silently picked one arbitrary connector and reported `Ok`,
/// discarding the rest -- exactly the "absence presenting as success"
/// defect class this epic exists to close, arriving in the one function
/// several other parts of it depend on. Selecting *by kind* -- one shape
/// per connector actually present, not one shape total -- is what
/// `build_declaration`/`build_declarations` below both build on.
fn find_connector_shapes<'a, 'g>(shapes: &'a [Shape<'g>]) -> Vec<&'a Shape<'g>> {
    shapes
        .iter()
        .filter(|s| s.is_node_shape() && s.targets.iter().any(|t| matches!(t, Target::Class(_))))
        .collect()
}

fn target_class_iri(shape: &Shape<'_>) -> Option<String> {
    shape.targets.iter().find_map(|t| match t {
        Target::Class(NamedOrBlankNodeRef::NamedNode(n)) => Some(n.as_str().to_string()),
        _ => None,
    })
}

/// The single `sh:in` value discriminating one `sh:xone` alternative shape
/// from its siblings, together with the `sh:path` of the property it was
/// read off -- recognised as the one property shape inside `alternative`
/// whose own `sh:in` names exactly one legal value (e.g.
/// `caldav:BasicAuthShape`'s own copy of `caldav:authMode`, restricted to
/// `sh:in ( "basic" )`, vs. the flat restatement's `sh:in ( "basic"
/// "bearer" )` with two). `None` if no property shape matches, or if the
/// one that does has no plain-IRI `sh:path` to name -- an alternative this
/// crate cannot honestly name a discriminant for is dropped by
/// `build_variants` rather than fabricating one.
fn discriminant_value_and_path(alternative: &Shape<'_>) -> Option<(String, String)> {
    alternative.property_shapes.iter().find_map(|p| {
        let value = p.constraints.iter().find_map(|c| match c {
            Constraint::In(values) if values.0.len() == 1 => term_literal_value(&values.0[0]),
            _ => None,
        })?;
        Some((value, simple_path_iri(p)?))
    })
}

/// D14's own marker (`lint::xone`) for "this predicate belongs to a
/// sibling alternative, forbidden here" -- not a real field of the
/// alternative that carries it.
fn has_max_count_zero(shape: &Shape<'_>) -> bool {
    shape
        .constraints
        .iter()
        .any(|c| matches!(c, Constraint::MaxCount(m) if m.0 == 0))
}

/// `shape`'s own `sh:xone` alternatives (contreforts/contreforts-config-api#27
/// item 2), if it declares one -- empty otherwise. Reads `Constraint::Xone`
/// directly off `shape.constraints`, exactly like `lint::xone::check`
/// already does for D14, rather than re-deriving it some other way.
fn build_variants(
    shape: &Shape<'_>,
    graph: &oxigraph::model::Graph,
    presentation: &HashMap<String, Presentation>,
) -> Vec<DeclarationVariant> {
    shape
        .constraints
        .iter()
        .find_map(|c| match c {
            Constraint::Xone(alternatives) => Some(&alternatives.0),
            _ => None,
        })
        .into_iter()
        .flatten()
        .filter_map(|alternative| {
            let (discriminant_value, discriminant_path) = discriminant_value_and_path(alternative)?;
            let properties = alternative
                .property_shapes
                .iter()
                .filter(|p| !has_max_count_zero(p))
                .filter_map(|p| build_property_shape(p, graph, presentation))
                .collect();
            Some(DeclarationVariant {
                discriminant_path,
                discriminant_value,
                properties,
            })
        })
        .collect()
}

/// The facts about a connector's own `sh:NodeShape` that [`Declaration`]
/// deliberately does not carry, and that a *total* digest of it cannot do
/// without (contreforts/contreforts-core#34, phase F item W4).
///
/// Every one of these is a construct that reaches no code at all today, and
/// therefore leaves no trace in `Declaration`:
///   - `sh:closed` / `sh:deactivated` are separate fields on `shacl_rust`'s
///     `Shape`, not members of `shape.constraints`, so `build_property_shape`'s
///     `unhandled` recording arm (W1) is structurally blind to both;
///   - `build_variants` `find_map`s `Constraint::Xone` and nothing else, so
///     any other node-level constraint reaches nothing;
///   - `build_property_shape` opens with `let path = simple_path_iri(shape)?;`
///     and `build_variants` with `let (..) = discriminant_value_and_path(..)?;`,
///     so a property with an inverse/sequence path, or an alternative with no
///     single-valued `sh:in`, simply vanishes -- and the only evidence left
///     is the *count* that went in, which is why both are recorded here.
///
/// Crate-internal, and paired with its own `Declaration` at construction
/// time by [`build_declarations_with_facts`] rather than looked up
/// afterwards by shape IRI: a lookup that misses is exactly the
/// "absence presenting as success" failure this whole item exists to close,
/// and pairing at construction makes a miss unrepresentable instead of
/// merely unlikely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NodeShapeFacts {
    /// `shape.closed.is_some()`.
    pub closed: bool,
    /// `shape.deactivated`.
    pub deactivated: bool,
    /// The discriminant name (e.g. `"Not"`, `"Sparql"`) of every node-level
    /// constraint that is not `sh:xone` -- the ones `build_variants` reaches
    /// with nothing at all. Same derivation as
    /// [`PropertyShape::unhandled`]'s, via `constraint_variant_name`.
    pub non_xone_constraints: Vec<String>,
    /// How many direct `sh:property` children the parser saw, *before*
    /// `build_property_shape` dropped any of them.
    pub declared_property_count: usize,
    /// How many `sh:xone` alternatives the parser saw, *before*
    /// `build_variants` dropped any of them. Sums every `sh:xone` list on
    /// the shape, while `build_variants` reads only the first -- so a shape
    /// carrying two `sh:xone` constraints is itself reported as a drop.
    pub declared_variant_count: usize,
}

fn node_shape_facts(shape: &Shape<'_>) -> NodeShapeFacts {
    let mut non_xone_constraints = Vec::new();
    let mut declared_variant_count = 0;
    for constraint in &shape.constraints {
        match constraint {
            Constraint::Xone(alternatives) => declared_variant_count += alternatives.0.len(),
            other => non_xone_constraints.push(constraint_variant_name(other)),
        }
    }
    NodeShapeFacts {
        closed: shape.closed.is_some(),
        deactivated: shape.deactivated,
        non_xone_constraints,
        declared_property_count: shape.property_shapes.len(),
        declared_variant_count,
    }
}

/// contreforts/contreforts-config-api#27 item 3: a field whose real
/// requiredness is declared only inside one `sh:xone` alternative (e.g.
/// caldav:password's `sh:minCount 1`, declared only inside
/// `caldav:BasicAuthShape`, never on its own flat restatement, which
/// carries no `sh:minCount` at all "by design" -- see
/// `Declaration.properties`'s own doc comment) is invisible to a consumer
/// of the flat property list alone. Where exactly one variant both
/// mentions the path (variants already exclude a sibling's `sh:maxCount 0`
/// predicates -- see `build_variants`) and requires it (`sh:minCount` >
/// 0), that is this field's real requirement whenever this connector is
/// relevant at all -- folded back onto the flat entry here. A field
/// required by more than one variant (ambiguous at the flat level) or not
/// required by any is left exactly as its own flat declaration says.
fn merge_variant_requiredness(properties: &mut [PropertyShape], variants: &[DeclarationVariant]) {
    for prop in properties.iter_mut() {
        if prop.min_count.is_some() {
            continue;
        }
        let mut required_by = variants
            .iter()
            .filter_map(|v| v.properties.iter().find(|vp| vp.path == prop.path))
            .filter_map(|vp| vp.min_count.filter(|&c| c > 0));
        if let Some(only) = required_by.next()
            && required_by.next().is_none()
        {
            prop.min_count = Some(only);
        }
    }
}

/// One connector's own `Declaration`, built entirely from its own node
/// shape -- `shape`/`target_class`/`category`/`ui_shape`/`properties`/
/// `variants` all describe exactly `shape` and nothing else. The one
/// building block both `build_declaration` (single, aggregated) and
/// `build_declarations` (one per connector) below share.
fn build_declaration_for_shape(
    shape: &Shape<'_>,
    graph: &oxigraph::model::Graph,
    presentation: &HashMap<String, Presentation>,
) -> Result<Declaration, Violation> {
    let target_class = target_class_iri(shape).ok_or_else(|| {
        Violation::structural(format!(
            "connector shape {} has no plain-IRI sh:targetClass",
            shape.node
        ))
    })?;

    let category = get_string_value(graph, shape.node, CATEGORY_PREDICATE);
    let ui_shape = get_string_value(graph, shape.node, UI_SHAPE_PREDICATE);
    // contreforts/contreforts-core#33, phase F item W2: both already parsed by shacl-rust's
    // `apply_common_shape_properties` (sh:name falling back to rdfs:label for `label`,
    // sh:description for `description`) -- no graph read or SPARQL needed here at all.
    let label = shape.name.clone();
    let description = shape.description.clone();

    let mut properties: Vec<PropertyShape> = shape
        .property_shapes
        .iter()
        .filter_map(|p| build_property_shape(p, graph, presentation))
        .collect();

    let variants = build_variants(shape, graph, presentation);
    merge_variant_requiredness(&mut properties, &variants);
    let groups = build_groups(graph, &properties, &variants);

    Ok(Declaration {
        shape: shape.node.to_string(),
        target_class,
        category,
        ui_shape,
        label,
        description,
        groups,
        properties,
        variants,
    })
}

const NO_CONNECTOR_SHAPE_MESSAGE: &str = "no sh:NodeShape with sh:targetClass found when building the Declaration -- the \
     structural lint should have already rejected this declaration before reaching model \
     construction";

/// `validate()`'s own entry point: always exactly one `Declaration`, for
/// backward compatibility with every existing caller (a connector's own
/// `build.rs`/tests, each validating one connector's own `declaration.ttl`
/// in isolation -- there, `find_connector_shapes` returns exactly one
/// shape and the fold below is a no-op).
///
/// contreforts/contreforts-config-api#27 item 3: for a graph that unions
/// more than one connector's own shape (`PRODUCT_GRAPH_TTL` is the only
/// one that does), this used to silently keep only the first shape found
/// -- `.properties` reflected one arbitrary connector while reporting
/// `Ok` for the whole graph. Every qualifying shape's own
/// `properties`/`variants` are folded into the one `Declaration` returned
/// here instead, so `.properties` genuinely reflects the whole graph
/// rather than one arbitrary survivor. `.shape`/`.target_class`/
/// `.category`/`.ui_shape` still describe only the first shape found --
/// those four are inherently singular per connector and have no honest
/// merge across differently-typed connectors; a caller that needs them
/// scoped per kind should use [`build_declarations`] instead, which is
/// exactly what it is for.
pub(crate) fn build_declaration(
    shapes: &[Shape<'_>],
    graph: &oxigraph::model::Graph,
) -> Result<Declaration, Violation> {
    let connector_shapes = find_connector_shapes(shapes);
    if connector_shapes.is_empty() {
        return Err(Violation::structural(NO_CONNECTOR_SHAPE_MESSAGE));
    }

    let presentation = presentation_by_node(graph)?;
    let mut declarations = connector_shapes
        .into_iter()
        .map(|shape| build_declaration_for_shape(shape, graph, &presentation))
        .collect::<Result<Vec<_>, _>>()?;

    let mut primary = declarations.remove(0);
    for rest in declarations {
        primary.properties.extend(rest.properties);
        primary.variants.extend(rest.variants);
        primary.groups.extend(rest.groups);
    }
    // Re-sort after folding in every other shape's own groups -- each `build_declaration_for_shape`
    // call already sorted its own slice, but the fold above interleaves them back out of
    // (order, iri) order. Same comparator as `build_groups`'s own sort -- see `sort_groups`.
    sort_groups(&mut primary.groups);
    Ok(primary)
}

/// The per-kind alternative to `build_declaration`: one properly-scoped
/// `Declaration` per qualifying `sh:NodeShape` in `shapes`, each carrying
/// only that shape's own properties/variants -- not one shape's fields
/// silently standing in for every connector present, and not the same
/// shape repeated. Exposed publicly as [`crate::declarations`], for a
/// caller (contreforts-config-api#27's product-graph route is the first)
/// that needs the whole product graph split back out by connector kind,
/// which `build_declaration`'s single, aggregated `Declaration` cannot
/// express.
pub(crate) fn build_declarations(
    shapes: &[Shape<'_>],
    graph: &oxigraph::model::Graph,
) -> Result<Vec<Declaration>, Violation> {
    let connector_shapes = find_connector_shapes(shapes);
    if connector_shapes.is_empty() {
        return Err(Violation::structural(NO_CONNECTOR_SHAPE_MESSAGE));
    }

    let presentation = presentation_by_node(graph)?;
    connector_shapes
        .into_iter()
        .map(|shape| build_declaration_for_shape(shape, graph, &presentation))
        .collect()
}

/// [`build_declarations`], plus each connector's own [`NodeShapeFacts`] --
/// the graph-level constructs `Declaration` deliberately does not model, but
/// which a *total* digest of it has to be able to name
/// (contreforts/contreforts-core#34, phase F item W4; see `NodeShapeFacts`
/// for the list and for why the two are paired here rather than joined by
/// shape IRI afterwards).
///
/// Deliberately a third entry point rather than five new public fields on
/// `Declaration`: `build_declaration` (singular) folds *every* connector
/// shape in a multi-connector graph into one `Declaration`, and there is no
/// honest way to fold a per-shape `sh:closed` flag or `sh:property` count
/// across differently-typed connectors -- a fold would have to pick one
/// arbitrary shape's answer and report it for all of them, which is the same
/// defect contreforts-config-api#27 item 3 already had to fix once in this
/// very function's singular sibling.
pub(crate) fn build_declarations_with_facts(
    shapes: &[Shape<'_>],
    graph: &oxigraph::model::Graph,
) -> Result<Vec<(Declaration, NodeShapeFacts)>, Violation> {
    let connector_shapes = find_connector_shapes(shapes);
    if connector_shapes.is_empty() {
        return Err(Violation::structural(NO_CONNECTOR_SHAPE_MESSAGE));
    }

    let presentation = presentation_by_node(graph)?;
    connector_shapes
        .into_iter()
        .map(|shape| {
            let declaration = build_declaration_for_shape(shape, graph, &presentation)?;
            Ok((declaration, node_shape_facts(shape)))
        })
        .collect()
}
