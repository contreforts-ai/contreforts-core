//! Server-side SHACL validation of connector config writes against the product graph
//! (contreforts-kg#19, phase C item C6 of the bootstrap epic,
//! contreforts-workspace#20).
//!
//! Relocated here from `contreforts-kg::connector_validation` by
//! contreforts/contreforts-workspace#58 (comment 7791), item D3a: this module belongs below
//! both `contreforts-kg` and the future `contreforts-config`, since D3c will need it from the
//! config-graph engine too, and neither store crate should have to depend on the other just to
//! reach it. `contreforts-kg` re-exports every name below (`src/lib.rs` and
//! `src/connector_validation.rs`) rather than re-declaring them, so its 15+ downstream users
//! keep compiling unchanged; `GraphError::ConnectorValidation` and its conversions stay behind
//! in `contreforts-kg` (out of this module's scope even before the move).
//!
//! Neither `contreforts-kg` nor `contreforts-config` needs to be depended on by this crate:
//! [`ConnectorValidator`] is built from plain data -- a `&str` of Turtle, e.g.
//! `contreforts-product::PRODUCT_GRAPH_TTL` -- handed in by whoever composes the application.
//! This module has no dependency on either store crate or on any connector crate.
//!
//! ## The undeclared-kind policy ("the absence problem, for the fourth time")
//!
//! `config_graph.rs` writes eleven connector kinds; as of contreforts-kg#21, only one of them
//! (`forgejo`) has a published declaration anywhere in this workspace. So SHACL cannot see a
//! missing shape, and the policy has to be decided and enforced explicitly, not discovered by
//! a validator that silently "passes" because it found nothing to check:
//!
//!   1. A kind whose declaration is present (its class is the `sh:targetClass` of some
//!      `sh:NodeShape` in the handed-in shapes, matched by short name against
//!      `ConnectorDescriptor::type_name` -- see [`ConnectorValidator::connector_iris`]):
//!      `write_connector` mints its `rdf:type` and every field predicate from that
//!      declaration's `sh:targetClass`/`sh:path` (contreforts-kg#21), and this validates the
//!      write, rejecting it on violation.
//!   2. A kind with no declaration: `write_connector` keeps minting `CORE_NS + type_name` and
//!      `CORE_NS + field`, exactly as before contreforts-kg#21 -- the honest statement that
//!      this kind is not migrated yet, not a fallback to hide. The write is allowed, but
//!      **counted** (see [`ConnectorValidator::unvalidated_write_count`]) and **named** in the
//!      construction-time report (see [`ConnectorValidator::unvalidated_kinds`]) -- never
//!      silently. [`ConnectorValidator::new`] also logs this report at `tracing::info!`, not
//!      `debug!`, so it is visible by default.
//!   3. **Seam for C7**: once every kind has a declaration, flip the `if` in
//!      [`ConnectorValidator::validate`] to reject an undeclared kind instead of allowing it,
//!      and delete `unvalidated_kinds`/`unvalidated_write_count` -- they exist only to make
//!      case 2 observable until then. **This move (D3a) does not touch that seam**: the
//!      behaviour stays byte-for-byte identical across the relocation: it is still C7's job,
//!      not D3a's.
//!
//! Case 1 and case 2 are also, as of contreforts-kg#21, the *only* place `config_graph.rs`
//! decides which namespace a connector's stored IRIs live in -- but as of contreforts-kg#23,
//! that decision no longer reads `ConnectorValidator` itself. It reads
//! [`ConnectorDeclarations`], a standalone view of the same `declared_connector_iris` this
//! validator computes at construction (see [`ConnectorValidator::declarations`]), which every
//! `ConfigGraph` constructor takes as its own required parameter -- independent of whether a
//! `&ConnectorValidator` is wired for write checking at all. That split is what lets a caller
//! turn SHACL validation off (or never build a `ConnectorValidator`) without a connector's
//! namespace resolution silently reverting to `CORE_NS`: passing
//! `ConfigGraph::new(store, validator.declarations())` keeps the declared namespace with no
//! validation, whereas the pre-contreforts-kg#23 API had no way to express that state at all
//! -- `Declared` only ever came bundled with a wired validator. `ConnectorDeclarations::none()`
//! is what every call site outside this crate's own tests still passes today
//! (`ConfigGraph::new` has no constructor that defaults to it) -- no declarations are
//! injected, so every kind is case 2, `CORE_NS` for everything, matching pre-contreforts-kg#21
//! behaviour exactly.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use oxigraph::model::{Graph, NamedOrBlankNodeRef};
use shacl_rust::validation::dataset::ValidationDataset;
use shacl_rust::{
    Constraint, PathElement, Shape, Target, ValidationReport, parse_shapes,
    rdf::read_graph_from_string, validate as shacl_validate,
};

/// One connector kind's class IRI and `short field name -> predicate IRI` mapping, read
/// directly off its declaration in the injected product graph (contreforts-kg#21) --
/// `sh:targetClass` for the class, `sh:path` on each of the target shape's direct
/// `sh:property` entries for the fields. Nothing here is derived from a namespace-shape
/// convention; a declaration that used an unexpected namespace, or omitted a field's
/// `sh:path`, would show up here exactly as declared (or as a missing entry in `field_iris`),
/// not silently papered over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorIris {
    /// The full `rdf:type` class IRI, e.g. `https://contreforts.ds-labs.org/ontologies/forgejo#ForgejoConnector`.
    pub class_iri: String,
    /// Short field name (e.g. `"instanceUrl"`) -> full predicate IRI, one entry per direct
    /// `sh:property` on the target shape whose `sh:path` is a single, simple predicate IRI
    /// (anything more exotic -- inverse/sequence paths -- has no short name to key on and is
    /// skipped; none of today's declarations use one).
    pub field_iris: HashMap<String, String>,
    /// Short field name -> its declared `sh:datatype` IRI (e.g.
    /// `http://www.w3.org/2001/XMLSchema#integer`), one entry only for a field whose property
    /// shape actually has an `sh:datatype` constraint (contreforts-kg#25). A field present in
    /// `field_iris` but absent here has a `sh:path` and no `sh:datatype` -- `write_connector`
    /// keeps writing a plain (untyped) literal for it, exactly as before this issue. Read
    /// directly off the same `prop.constraints` `resolve_connector_iris` already walks for
    /// `field_iris`, so the two maps can never disagree about which fields a declaration
    /// covers.
    pub field_datatypes: HashMap<String, String>,
}

/// Which connector kinds' class/field IRIs are declared, and where to find them --
/// `config_graph.rs`'s only input for deciding a connector's namespace (contreforts-kg#23).
///
/// This is deliberately a fact separate from [`ConnectorValidator`]: whether a `ConfigGraph`
/// checks writes against SHACL is a policy a caller can switch off, but which namespace a
/// connector's triples live in is a property of the deployment's declarations and must not
/// change with that switch (see this module's and `config_graph`'s top-level docs for why
/// coupling the two was the bug). Every `ConfigGraph` constructor takes one of these by value
/// -- there is no constructor that omits it, so "no declarations in force" can only ever be
/// the explicit [`ConnectorDeclarations::none`], never a default a caller falls into.
#[derive(Debug, Clone, Copy)]
pub struct ConnectorDeclarations<'a> {
    iris: Option<&'a HashMap<&'static str, ConnectorIris>>,
}

impl<'a> ConnectorDeclarations<'a> {
    /// States, explicitly, that no declarations are in force: every connector kind resolves
    /// to `CORE_NS`, exactly the behaviour `ConfigGraph::new` had before this issue existed.
    /// Callers reach for this deliberately (e.g. a script with no product graph to hand),
    /// never by omission -- there is no `ConfigGraph` constructor that defaults to it.
    pub fn none() -> Self {
        Self { iris: None }
    }

    pub(crate) fn from_map(iris: &'a HashMap<&'static str, ConnectorIris>) -> Self {
        Self { iris: Some(iris) }
    }

    /// The class/field IRIs declared for `kind`, or `None` when `kind` has no declaration --
    /// either because these are `ConnectorDeclarations::none()`, or because a real set of
    /// declarations simply does not cover `kind` yet. `config_graph::ConnectorNamespace`'s
    /// `Core` case treats both identically, on purpose (contreforts-kg#21).
    ///
    /// Widened from `pub(crate)` to `pub` by contreforts/contreforts-workspace#58 (comment
    /// 7791), item D3a: this type used to live in `contreforts-kg` alongside its only caller,
    /// `config_graph.rs`, so crate-private access was enough. The move puts
    /// `ConnectorDeclarations` in `contreforts-declaration` while `config_graph.rs` (and its
    /// call at `connector_namespace`, matching the case-1/case-2 policy documented on this
    /// type and at the top of this module) stays behind in `contreforts-kg` -- so the same call
    /// now crosses a crate boundary, and `pub` is mechanically necessary for it to keep
    /// compiling at all. The policy itself is unchanged by the widening: `Some` means case 1
    /// (a declared kind -- use its class/field IRIs), `None` means case 2 (either
    /// `ConnectorDeclarations::none()`, or a real declaration set that simply does not cover
    /// this `kind` yet) -- both resolve to `CORE_NS` in `config_graph.rs`, indistinguishably,
    /// by design.
    pub fn connector_iris(&self, kind: &str) -> Option<&'a ConnectorIris> {
        self.iris.and_then(|m| m.get(kind))
    }
}

/// Find the one top-level shape in `shapes` whose `sh:targetClass` short name (the part after
/// the last `#`/`/`) equals `type_name`, and read off its class IRI and field mapping -- the
/// case-1 half of the policy documented at the top of this module. Returns `None` when no
/// shape targets a class by that short name, i.e. case 2.
fn resolve_connector_iris(shapes: &[Shape<'static>], type_name: &str) -> Option<ConnectorIris> {
    for shape in shapes {
        for target in &shape.targets {
            let Target::Class(NamedOrBlankNodeRef::NamedNode(class_node)) = target else {
                continue;
            };
            if short_field_name(class_node.as_str()) != type_name {
                continue;
            }

            let mut field_iris: HashMap<String, String> = HashMap::new();
            let mut field_datatypes: HashMap<String, String> = HashMap::new();
            for prop in &shape.property_shapes {
                let Some(path) = prop.path.as_ref() else {
                    continue;
                };
                let [PathElement::Iri(pred)] = path.get_elements() else {
                    continue;
                };
                let short = short_field_name(pred.as_str());
                field_iris.insert(short.clone(), pred.as_str().to_string());

                if let Some(datatype_iri) = prop.constraints.iter().find_map(|c| match c {
                    Constraint::Datatype(dt) => Some(dt.0.as_str().to_string()),
                    _ => None,
                }) {
                    field_datatypes.insert(short, datatype_iri);
                }
            }

            return Some(ConnectorIris {
                class_iri: class_node.as_str().to_string(),
                field_iris,
                field_datatypes,
            });
        }
    }
    None
}

/// One SHACL constraint a written connector instance violated, formatted for a human
/// operator: which field (when SHACL reported a `sh:resultPath`) and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorViolation {
    /// The short field name (e.g. `"instanceUrl"`), when the violated constraint named a
    /// `sh:resultPath`. `None` for a node-level constraint (`sh:xone`, `sh:sparql`, ...).
    pub field: Option<String>,
    pub message: String,
}

impl std::fmt::Display for ConnectorViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.field {
            Some(field) => write!(f, "{field}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

/// Outcome of checking one connector write against the declared shapes, on the non-violating
/// path (a violation is reported through the `Err` side of [`ConnectorValidator::validate`]
/// instead, since it carries data callers must act on).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorValidationOutcome {
    /// No declaration covers this connector's class -- allowed under the case-2 policy above.
    /// Already reflected in `unvalidated_write_count` by the time this is returned.
    NotDeclared,
    /// A declared shape covers this class and the instance satisfied it.
    Conforms,
}

/// Failure constructing a [`ConnectorValidator`]: the handed-in Turtle was not parseable, or
/// not well-formed SHACL. Never returned by `validate` itself -- a per-write SHACL failure is
/// reported as `Vec<ConnectorViolation>` instead, since "the shapes graph is broken" and "this
/// one write is invalid" are different failures a caller needs to handle differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorValidatorError(String);

impl std::fmt::Display for ConnectorValidatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ConnectorValidatorError {}

/// Caches parsed SHACL shapes across writes and checks one connector instance against them.
///
/// Constructed once (e.g. at process startup) from a `&str` of Turtle -- see the module docs
/// for why it cannot obtain that Turtle itself. Reparsing per write would throw away exactly
/// the cost contreforts-workspace#21 measured shapes caching as saving, so parsing happens
/// once here, in [`ConnectorValidator::new`], not in [`ConnectorValidator::validate`].
pub struct ConnectorValidator {
    // Parsed once at construction and leaked for `'static`: a `ConnectorValidator` is meant to
    // be built once and held for the process's lifetime (there is no reclaim path that would
    // ever run anyway), so leaking the owning graph is the simplest sound way to hold
    // `Shape<'static>`s that borrow from it -- no unsafe code, no self-referential-struct
    // trickery, just `Box::leak`. `ValidationDataset::from_graphs` separately requires an
    // *owned* `Graph` per call (see `validate` below) regardless of this cache, so
    // `shapes_graph` is cloned again on every validated write -- that clone cost is measured,
    // not avoided, by design; see contreforts-kg#19's report for actual numbers.
    shapes_graph: &'static Graph,
    shapes: Vec<Shape<'static>>,
    declared_target_classes: HashSet<String>,
    /// Case-1 kinds (contreforts-kg#21): `kind` -> the class/field IRIs read off its
    /// declaration, computed once here from the same `shapes` the validator itself uses. As of
    /// contreforts-kg#23, `config_graph.rs` no longer reads this (or [`Self::connector_iris`])
    /// directly -- it takes a [`ConnectorDeclarations`] view of it instead, via
    /// [`Self::declarations`], so namespace resolution does not require a `ConnectorValidator`
    /// at all.
    declared_connector_iris: HashMap<&'static str, ConnectorIris>,
    unvalidated_kinds: Vec<&'static str>,
    /// `all_kinds.len()` at construction, kept only so `Display` can report "N/total" without
    /// re-deriving `total` from `declared_target_classes.len() + unvalidated_kinds.len()`,
    /// which would silently under-count if two kinds ever shared one `type_name`.
    total_kind_count: usize,
    unvalidated_write_count: AtomicUsize,
    validated_write_count: AtomicUsize,
}

impl ConnectorValidator {
    /// Parses `shapes_ttl` once and caches the result. `all_kinds` is every `(kind,
    /// type_name)` pair the write path can produce -- `config_graph::all_connector_kinds()` in
    /// practice -- used to compute both [`Self::connector_iris`] (contreforts-kg#21) and
    /// [`Self::unvalidated_kinds`] at construction time. `unvalidated_kinds` is exactly "kinds
    /// with no entry in `declared_connector_iris`" -- one computation, not two that could
    /// disagree about which kinds are declared.
    pub fn new(
        shapes_ttl: &str,
        all_kinds: &[(&'static str, &'static str)],
    ) -> Result<Self, ConnectorValidatorError> {
        let graph = read_graph_from_string(shapes_ttl, "turtle").map_err(|e| {
            ConnectorValidatorError(format!("shapes graph is not valid Turtle: {e}"))
        })?;
        let shapes_graph: &'static Graph = Box::leak(Box::new(graph));
        let shapes: Vec<Shape<'static>> = parse_shapes(shapes_graph).map_err(|e| {
            ConnectorValidatorError(format!("shapes graph is not well-formed SHACL: {e}"))
        })?;

        let declared_target_classes: HashSet<String> = shapes
            .iter()
            .flat_map(|s| s.targets.iter())
            .filter_map(|t| match *t {
                Target::Class(NamedOrBlankNodeRef::NamedNode(n)) => Some(n.as_str().to_string()),
                _ => None,
            })
            .collect();

        let declared_connector_iris: HashMap<&'static str, ConnectorIris> = all_kinds
            .iter()
            .filter_map(|(kind, type_name)| {
                resolve_connector_iris(&shapes, type_name).map(|iris| (*kind, iris))
            })
            .collect();

        let unvalidated_kinds: Vec<&'static str> = all_kinds
            .iter()
            .filter(|(kind, _)| !declared_connector_iris.contains_key(kind))
            .map(|(kind, _)| *kind)
            .collect();

        let validator = Self {
            shapes_graph,
            shapes,
            declared_target_classes,
            declared_connector_iris,
            unvalidated_kinds,
            total_kind_count: all_kinds.len(),
            unvalidated_write_count: AtomicUsize::new(0),
            validated_write_count: AtomicUsize::new(0),
        };

        // Not a `tracing::debug!` nobody reads (contreforts-kg#19 explicitly calls that out as
        // a false-assurance failure mode): this is the one place a caller doing nothing else
        // still sees, by default, which connector kinds have no server-side write check.
        tracing::info!("{validator}");

        Ok(validator)
    }

    /// Validates one connector instance's about-to-be-written triples against the cached
    /// shapes. `class_iri` is the full `rdf:type` IRI the write uses -- as of contreforts-kg#21,
    /// [`Self::connector_iris`]'s `class_iri` for a declared kind, or `CORE_NS +
    /// descriptor.type_name` for an undeclared one; `instance` must contain exactly the
    /// triples `write_connector` is about to write for this connector and nothing else -- see
    /// `config_graph::ConfigGraph::connector_instance_graph`, the one function that builds
    /// both the validated graph and the actual store write from the same
    /// `(descriptor, label, fields)`, so they cannot diverge.
    ///
    /// `Ok(NotDeclared)` and `Ok(Conforms)` both mean "the write may proceed"; `Err` carries
    /// every violation SHACL found (never just the first), for the caller to fail the write
    /// with.
    pub fn validate(
        &self,
        class_iri: &str,
        instance: &Graph,
    ) -> Result<ConnectorValidationOutcome, Vec<ConnectorViolation>> {
        if !self.declared_target_classes.contains(class_iri) {
            self.unvalidated_write_count.fetch_add(1, Ordering::Relaxed);
            return Ok(ConnectorValidationOutcome::NotDeclared);
        }

        // `ValidationDataset::from_graphs` takes owned graphs -- this clone of the cached
        // shapes graph, and the `sh:sparql` full-store materialization `build_store` performs
        // when a SPARQL constraint is present (shacl-rust's `ValidationDataset::store`,
        // triggered lazily only if a shape actually needs it), are exactly the per-write costs
        // contreforts-kg#19's report measures separately for the Core-only and sh:sparql
        // cases.
        let dataset = ValidationDataset::from_graphs(instance.clone(), self.shapes_graph.clone())
            .map_err(|e| {
            vec![ConnectorViolation {
                field: None,
                message: format!("failed to build the validation dataset: {e}"),
            }]
        })?;

        let report: ValidationReport<'_> = shacl_validate(&dataset, &self.shapes);
        self.validated_write_count.fetch_add(1, Ordering::Relaxed);

        if *report.get_conforms() {
            return Ok(ConnectorValidationOutcome::Conforms);
        }

        Err(report
            .get_results()
            .iter()
            .map(|r| ConnectorViolation {
                field: r.result_path().map(|p| short_field_name(&p.to_string())),
                message: if !r.messages().is_empty() {
                    r.messages().join("; ")
                } else if let Some(detail) = r.constraint_detail() {
                    detail.to_string()
                } else {
                    format!("violates shape {}", r.source_shape())
                },
            })
            .collect())
    }

    /// Connector kinds with no `sh:targetClass` in the shapes handed to [`Self::new`], computed
    /// once at construction -- the static half of the case-2 policy's "surfaced, not silent"
    /// requirement. See [`Self::unvalidated_write_count`] for the dynamic half (how many writes
    /// actually went through unvalidated).
    pub fn unvalidated_kinds(&self) -> &[&'static str] {
        &self.unvalidated_kinds
    }

    /// The class IRI and field-predicate mapping `write_connector` (and every read helper)
    /// must use for `kind`, per the case-1/case-2 policy documented at the top of this module
    /// (contreforts-kg#21) -- `None` for a kind with no declaration in the shapes handed to
    /// [`Self::new`] (case 2: `config_graph.rs` falls back to `CORE_NS` uniformly, never
    /// mixing it with a declared field on the same connector).
    pub fn connector_iris(&self, kind: &str) -> Option<&ConnectorIris> {
        self.declared_connector_iris.get(kind)
    }

    /// This validator's declarations, as the standalone [`ConnectorDeclarations`] every
    /// `ConfigGraph` constructor requires (contreforts-kg#23). Typically passed straight to
    /// `contreforts_kg::config_graph::ConfigGraph::with_validator` so namespace resolution and
    /// SHACL validation agree on what is declared -- but the two parameters stay independent: a
    /// caller may also hand these declarations to `ConfigGraph::new` (no `&self` validator at
    /// all) to keep namespace resolution while turning write validation off.
    pub fn declarations(&self) -> ConnectorDeclarations<'_> {
        ConnectorDeclarations::from_map(&self.declared_connector_iris)
    }

    /// How many `validate()` calls returned `NotDeclared` (i.e. writes that were allowed
    /// without a server-side check) since construction.
    pub fn unvalidated_write_count(&self) -> usize {
        self.unvalidated_write_count.load(Ordering::Relaxed)
    }

    /// How many `validate()` calls actually ran a declared shape against an instance (whether
    /// or not it conformed) since construction.
    pub fn validated_write_count(&self) -> usize {
        self.validated_write_count.load(Ordering::Relaxed)
    }
}

impl std::fmt::Display for ConnectorValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.unvalidated_kinds.is_empty() {
            write!(
                f,
                "connector write validation: every connector kind has a declared shape"
            )
        } else {
            write!(
                f,
                "connector write validation: {}/{} connector kind(s) have no declaration and \
                 are allowed unvalidated (contreforts-kg#19 case-2 policy, seam for C7): {}",
                self.unvalidated_kinds.len(),
                self.total_kind_count,
                self.unvalidated_kinds.join(", "),
            )
        }
    }
}

/// Last path segment of a SHACL `sh:resultPath` display string (which is a bare IRI, e.g.
/// `https://contreforts.ds-labs.org/ontologies/core#instanceUrl`), for a short, readable field
/// name in an error message.
fn short_field_name(path_display: &str) -> String {
    let bare = path_display.trim_start_matches('<').trim_end_matches('>');
    bare.rsplit(['#', '/']).next().unwrap_or(bare).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic (no real declaration has a `sh:datatype` other than `xsd:string` yet, per
    /// this module's and `config_graph`'s docs) -- `count` declares `sh:datatype xsd:integer`,
    /// `label` has an `sh:path` and deliberately no `sh:datatype`. contreforts-kg#25: proves
    /// `resolve_connector_iris` (via `ConnectorValidator::connector_iris`) captures the two
    /// differently -- the mechanism every field-typing decision in `config_graph.rs` reads.
    const WIDGET_DECLARATION_TTL: &str = r#"
        @prefix sh:  <http://www.w3.org/ns/shacl#> .
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
        @prefix widget: <https://contreforts.ds-labs.org/ontologies/widget#> .

        widget:WidgetShape a sh:NodeShape ;
            sh:targetClass widget:Widget ;
            sh:property [
                sh:path widget:label ;
            ] ;
            sh:property [
                sh:path widget:count ;
                sh:datatype xsd:integer ;
            ] .
    "#;

    #[test]
    fn connector_iris_captures_datatype_only_for_fields_that_declare_one() {
        let validator =
            ConnectorValidator::new(WIDGET_DECLARATION_TTL, &[("widget", "Widget")]).unwrap();

        let iris = validator
            .connector_iris("widget")
            .expect("widget has a declaration");

        assert_eq!(iris.field_iris.len(), 2, "both fields have an sh:path");

        assert_eq!(
            iris.field_datatypes.get("count").map(String::as_str),
            Some("http://www.w3.org/2001/XMLSchema#integer"),
            "count declares sh:datatype xsd:integer"
        );
        assert!(
            !iris.field_datatypes.contains_key("label"),
            "label has sh:path but no sh:datatype -- must have no entry, not a None value \
             hidden behind one"
        );
    }
}
