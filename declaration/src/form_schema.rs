//! The **total** digest that turns a validated declaration into a
//! renderable form schema (contreforts/contreforts-core#34, phase F item
//! W4, tracked at contreforts/contreforts-workspace#60).
//!
//! ## What "total" means here, and why it is the whole point
//!
//! Everything above this function is a renderer. The config webui has no
//! RDF awareness at all under the architecture recorded on
//! contreforts-workspace#60: the server hands it a digested form schema and
//! it draws that, nothing more. So a SHACL construct this digest does not
//! understand has exactly one place left where it can still become visible
//! -- here, as a named [`DigestError`]. Never a fallback control, never an
//! omitted field, never a schema that is quietly one field short.
//!
//! That is not a stylistic preference. `build_property_shape`
//! (`model.rs`) opens with `let path = simple_path_iri(shape)?;`,
//! `build_variants` with `let (..) = discriminant_value_and_path(..)?;`,
//! and until phase F item W1 the constraint match ended in a bare
//! `_ => {}`. Each of those drops a declared construct *before* anything
//! downstream can notice it is gone, and the epic has recorded twelve prior
//! instances of absence presenting as success
//! (contreforts/contreforts-workspace#19, comment 6689). This module
//! converts that entire class into one named failure at one boundary.
//!
//! Concretely, every one of these is an error rather than a silent
//! omission, and **every one has zero instances in the tree today** -- which
//! is exactly why they are closed now, before the first declaration that
//! uses one meets a digest that has never walked it:
//!
//! | construct | error |
//! |---|---|
//! | a non-empty [`PropertyShape::unhandled`] entry (W1's mechanism) | [`DigestError::UnhandledConstraint`] |
//! | `sh:node` on a property | [`DigestError::NestedShape`] |
//! | a `contreforts:uiShape` outside [`KNOWN_UI_SHAPES`] | [`DigestError::UnknownUiShape`] |
//! | an `sh:datatype` with no control mapping | [`DigestError::UnmappedDatatype`] |
//! | neither `sh:datatype` nor `sh:in` | [`DigestError::NoDatatypeAndNoIn`] |
//! | an `sh:group` resolving to no label/order | [`DigestError::UnresolvedGroup`] |
//! | `sh:closed` / `sh:deactivated` | [`DigestError::ClosedOrDeactivatedShape`] |
//! | a property dropped by `simple_path_iri` | [`DigestError::DroppedProperty`] |
//! | an alternative dropped by `discriminant_value_and_path` | [`DigestError::DroppedVariant`] |
//! | a node-level constraint other than `sh:xone` | [`DigestError::UnhandledConstraint`] |
//! | an `sh:targetClass` with no `#` | [`DigestError::UndeckedKind`] |
//! | a meta-shape or lint failure from [`crate::declarations`] | [`DigestError::Declaration`] |
//!
//! Errors are **collected, never fail-fast**: one property shape can carry
//! two unsupported constructs at once (the vendored forgejo fixture's
//! `groupMapping` carries both `sh:node` and
//! `contreforts:uiShape "mapping-table"`), and W5 renders the whole list.
//!
//! ## Requiredness comes from `variants[]`, never from the flat list
//!
//! `merge_variant_requiredness` (`model.rs`) folds an `sh:xone`
//! alternative's own `sh:minCount` back onto the flat property whenever
//! exactly one alternative requires it. On the real declarations that means
//! `Declaration.properties` reports caldav's `username`, `password` **and**
//! `token` as required simultaneously -- three mutually exclusive fields --
//! and o365's `tenantId`, `clientId`, `clientSecret` **and** `token`. A form
//! built from the flat list would be unsatisfiable.
//!
//! So [`FieldDescriptor::required`] is `min_count > 0` **only** for a field
//! no alternative mentions. Any field named by any alternative is forced
//! `required: false` with `variant_governed: true`, and its real
//! requiredness lives in [`VariantRule::required`] alone -- making flat
//! requiredness for a union field *unexpressible* rather than merely
//! discouraged.
//!
//! The discriminator itself is the exception, and the one a naive rule gets
//! wrong: `authMode` carries `sh:minCount 1` inside all four real
//! alternatives, so it appears in every alternative's property list, but its
//! requiredness is genuinely flat -- the form always asks for it. It is
//! excluded from `shown`/`required`/`hidden` on every rule and keeps its
//! flat `required: true`.
//!
//! ## Determinism
//!
//! `Declaration.properties`, `variants[].properties` and
//! `find_connector_shapes`'s own result all come back in a graph traversal
//! order that is **not stable even between two calls in one process**
//! (measured on this tree: two `form_schemas` calls in the same test binary
//! disagreed on 6 of 6 runs with the sorts below removed). Every ordering
//! this module emits is therefore its own, and total:
//!
//! - schemas by `(kind, target class)`;
//! - groups by `(sh:order, group local name)`;
//! - fields, within a group, by `(sh:order, field id)` with an absent
//!   `sh:order` sorted last;
//! - [`VariantRule`]'s three id vectors by the schema's own flattened field
//!   order -- declaration order is unrecoverable (see above) and
//!   alphabetical would render o365's auth fields as
//!   `clientId, clientSecret, tenantId`, contradicting the declared
//!   `tenantId, clientId, clientSecret`;
//! - errors by the same declaration order the schemas would have had, then
//!   node-level faults before field-level ones.
//!
//! ## No prefill affordance
//!
//! `sh:defaultValue` has zero triples tree-wide, and the maintainer's
//! decision on contreforts-workspace#60 is that no prefill affordance may be
//! designed. [`PropertyShape::default_value`] is therefore the one field of
//! `PropertyShape` this module deliberately never reads: nothing resembling
//! a default reaches the wire, and there is no code path by which one could.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::error::Violations;
use crate::model::{Declaration, GroupDescriptor, NodeShapeFacts, PropertyShape};

/// Every `contreforts:uiShape` value this system has a renderer for.
///
/// Exactly one is declared tree-wide (`"discriminated-union"`, on caldav's
/// and o365's node shapes). Anything else names a component the thin
/// renderer does not have and has no way to discover is missing, so it is
/// [`DigestError::UnknownUiShape`] rather than an attribute passed through
/// for someone else to fail on. Applies at both levels: a node shape's own
/// `contreforts:uiShape` and a property shape's.
pub const KNOWN_UI_SHAPES: &[&str] = &["discriminated-union"];

// ---------------------------------------------------------------------
// The wire types
// ---------------------------------------------------------------------

/// One connector's complete, renderable form.
///
/// `Serialize` is not incidental: W7 serves these verbatim rather than
/// re-modelling them in a second wire struct, so this type *is* the wire
/// contract and a field added here appears in the API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FormSchema {
    /// The connector kind (`"caldav"`, `"o365"`, ...), derived from the
    /// `sh:targetClass` IRI's own namespace segment -- the key every layer
    /// above joins on.
    pub kind: String,
    /// The node shape's own `sh:name`/`rdfs:label`, falling back to
    /// [`Self::kind`] when it declares neither. Non-optional deliberately: a
    /// connector picker entry with an empty heading is not a thing a
    /// renderer should have to decide how to handle.
    pub label: String,
    /// The node shape's own `sh:description`. `None` is legitimate.
    pub description: Option<String>,
    /// The node shape's own `contreforts:uiShape`, already checked against
    /// [`KNOWN_UI_SHAPES`].
    pub ui_shape: Option<String>,
    /// The form's sections, in render order. Nested rather than flat
    /// because `sh:order` is *not* unique within a connector -- caldav and
    /// o365 each declare three fields at order 1 -- so a flat sort by order
    /// alone is ambiguous and would interleave sections.
    pub groups: Vec<GroupSection>,
    /// The show/hide/require rules of this connector's tagged union, if it
    /// declares one. Empty otherwise.
    pub variants: Vec<VariantRule>,
}

/// One `sh:PropertyGroup`'s worth of fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupSection {
    /// The group IRI's local name (e.g. `"AuthenticationGroup"`) -- the same
    /// identity rule as [`FieldDescriptor::id`], so a client holds one kind
    /// of key, not two.
    pub id: String,
    /// The group's own `rdfs:label`. Non-optional, which is only truthful
    /// because a group resolving to no label is
    /// [`DigestError::UnresolvedGroup`]: a section with no heading is a
    /// field the user cannot find.
    pub label: String,
    /// The group's own `sh:order`. Non-optional for the same reason.
    pub order: i64,
    pub fields: Vec<FieldDescriptor>,
}

/// One rendered field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldDescriptor {
    /// The `sh:path` IRI's local name (the fragment after the last `#` or
    /// `/`). Property shapes are blank nodes throughout, so this is the only
    /// stable identity a client can hold, and it is already the settled one
    /// everywhere else (`product_graph.rs`'s route, `write_connector`'s
    /// `fields` table).
    pub id: String,
    /// The property's own `sh:name`, falling back to [`Self::id`].
    pub label: String,
    pub description: Option<String>,
    pub control: Control,
    /// `sh:minCount > 0`, **and only for a field no `sh:xone` alternative
    /// mentions** -- see this module's own doc comment.
    pub required: bool,
    /// Whether this field's requiredness (and visibility) is decided by
    /// [`FormSchema::variants`] rather than by its own `sh:minCount`.
    pub variant_governed: bool,
    /// `contreforts:secret`, surfaced explicitly *as well as* folded into
    /// [`Control::Secret`] -- W7 and W8 both key elision on it and must not
    /// have to pattern-match a control to find it.
    pub secret: bool,
    /// `contreforts:configField`: the *storage* path (`caldav:password` ->
    /// `"auth.password"`). Carried through, and deliberately **not** the
    /// field id -- the 27 declared values are not unique per local name
    /// across connectors, so a schema keyed by them would collide. `None` is
    /// meaningful (D15): identity/presentation rather than runtime config.
    pub config_field: Option<String>,
    /// The property's own `contreforts:uiShape`, already checked against
    /// [`KNOWN_UI_SHAPES`].
    pub ui_shape: Option<String>,
}

/// How a field is rendered. A **closed** enum, deliberately not
/// `#[non_exhaustive]`: a consumer must be able to `match` it with no
/// wildcard arm, so that adding a control is a compile error everywhere it
/// has to be handled rather than a silently unrendered field somewhere.
///
/// Closedness is defensible only because the declared surface is genuinely
/// this small: of the 45 top-level properties tree-wide, 39 declare
/// `xsd:string`, 4 declare `xsd:integer`, and the remaining 2 -- caldav's
/// and o365's `authMode` -- declare no `sh:datatype` at all and are covered
/// by `sh:in`. [`Control::Decimal`], [`Control::Boolean`] and
/// [`Control::Date`] have **zero** declared instances anywhere; they are
/// kept so that the first declaration to use one does not break W5's build
/// until a core PR lands, and each is proven by its own synthetic fixture
/// rather than being an unreachable arm wearing the costume of a census.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Control {
    Text {
        /// `sh:pattern`, carried byte-for-byte. All seven declared patterns
        /// tree-wide are the same string; re-escaping one is how a renderer
        /// silently starts rejecting valid input.
        pattern: Option<String>,
        min_length: Option<u32>,
    },
    Integer {
        /// `sh:minInclusive`, parsed as an integer rather than coerced
        /// through `f64` -- `1` round-tripping as `1.0` is not equal under
        /// `serde_json::Number`, which is exactly why the product-graph
        /// route's `numeric_or_null` refuses that coercion too.
        min: Option<i64>,
        max: Option<i64>,
    },
    Decimal,
    Boolean,
    Date,
    Secret,
    Select {
        options: Vec<SelectOption>,
    },
}

/// One `sh:in` member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectOption {
    /// The raw declared value, untouched -- this is what gets stored.
    pub value: String,
    /// A deterministic humanisation of [`Self::value`] (snake_case to
    /// sentence case), computed in Rust so that every renderer agrees.
    /// Whether the resulting copy is the *right* copy is a product
    /// judgement no test can settle (contreforts-core#34, Unsettled item 4);
    /// what is settled here is that it is a pure function of the declared
    /// value.
    pub label: String,
}

/// One alternative of a connector's tagged union, as show/hide/require
/// instructions over **field ids only**.
///
/// Never [`FieldDescriptor`]s: the 20 property shapes inside the four real
/// `sh:xone` alternatives carry no `sh:name`, no `sh:description`, no
/// `sh:group`, no `sh:order`, no `contreforts:secret` and no
/// `contreforts:configField` -- verified on all four -- while all 45 flat
/// properties carry all of them. A descriptor built from an alternative's
/// copy of `caldav:password` would render an unlabelled, ungrouped,
/// **non-secret** plain text box. Every variant field is restated flat, so
/// every id here resolves against the flat set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VariantRule {
    pub discriminant_field: String,
    pub discriminant_value: String,
    /// This alternative's own field ids, minus the discriminator.
    pub shown: Vec<String>,
    /// Those of [`Self::shown`] the alternative declares `sh:minCount > 0`
    /// for. The discriminator is excluded explicitly: it carries
    /// `sh:minCount 1` inside all four real alternatives, so a naive filter
    /// yields `["authMode", "username", "password"]` for caldav-basic.
    pub required: Vec<String>,
    /// (union of every alternative's field ids) - (this alternative's) -
    /// discriminator.
    ///
    /// It cannot be read off the graph: `build_variants` filters the
    /// `sh:maxCount 0` cross-exclusion markers out before they ever reach
    /// `variants[].properties`, so the set difference is the only
    /// derivation -- and it is still purely a function of `variants[]`.
    pub hidden: Vec<String>,
}

// ---------------------------------------------------------------------
// The error type
// ---------------------------------------------------------------------

/// Everything this digest can refuse to guess at.
///
/// Deliberately **not** `Serialize`: these are build-time failures W5 prints
/// via `Display`, not part of the wire contract, and
/// [`DigestError::Declaration`] wraps a [`Violations`] whose own `Display`
/// is the thing a connector author needs to read.
///
/// Every variant names the connector kind it came from, the offending
/// `sh:path` local name (or nothing, for a node-level fault), and the
/// construct -- so the message alone is enough to act on without going back
/// to the graph.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DigestError {
    /// [`crate::declarations`] itself failed: malformed Turtle, a meta-shape
    /// violation, or a D14/D2/D15/entityKind lint. Carried through rather
    /// than `unwrap`ped, because W5 calls this from a `build.rs` where a
    /// panic is a stack trace instead of a legible list of what the author
    /// got wrong.
    #[error("{0}")]
    Declaration(Violations),

    /// The `sh:targetClass` IRI has no `#`, so no connector kind can be
    /// derived from it -- and a guessed kind is a form wired to the wrong
    /// connector.
    #[error(
        "sh:targetClass <{target_class}> has no '#' fragment, so no connector kind can be \
         derived from it; D2 requires a declaration to mint its terms in its own connector's \
         namespace"
    )]
    UndeckedKind { target_class: String },

    /// An `sh:datatype` with no [`Control`] mapping. Never a fallback to
    /// [`Control::Text`] (which renders a hex blob as a text box and reports
    /// success) and never an omitted field.
    #[error(
        "{kind}: sh:path {path} declares sh:datatype <{datatype}>, which this digest has no \
         form control for -- add a Control mapping rather than letting the field fall back to \
         a text box"
    )]
    UnmappedDatatype {
        kind: String,
        path: String,
        datatype: String,
    },

    /// A property with neither `sh:datatype` nor `sh:in` has no derivable
    /// control at all.
    #[error(
        "{kind}: sh:path {path} declares neither sh:datatype nor sh:in, so no form control can \
         be derived for it"
    )]
    NoDatatypeAndNoIn { kind: String, path: String },

    /// A SHACL construct this digest does not model: either an entry W1
    /// recorded in [`PropertyShape::unhandled`], or a node-level constraint
    /// other than `sh:xone` (which `build_variants` reaches with nothing at
    /// all, and which W1's per-property recording arm never sees).
    #[error(
        "{kind}: {} carries the SHACL constraint {construct}, which this digest does not \
         understand",
        location(.path)
    )]
    UnhandledConstraint {
        kind: String,
        path: Option<String>,
        construct: String,
    },

    /// `sh:node` on a property: a nested shape this digest does not walk.
    /// Kept as its own variant rather than folded into
    /// [`Self::UnhandledConstraint`] so the message names the construct a
    /// maintainer has to go and look at.
    #[error(
        "{kind}: sh:path {path} carries sh:node, a nested shape this digest does not walk -- \
         its fields would be silently absent from the rendered form"
    )]
    NestedShape { kind: String, path: String },

    /// A `contreforts:uiShape` outside [`KNOWN_UI_SHAPES`].
    #[error(
        "{kind}: {} declares contreforts:uiShape \"{value}\", which is not one of the UI shapes \
         this renderer knows ({known})",
        location(.path)
    )]
    UnknownUiShape {
        kind: String,
        path: Option<String>,
        value: String,
        known: String,
    },

    /// An `sh:group` that resolves to no label and order -- dangling
    /// (referenced but never typed `a sh:PropertyGroup`), declared but
    /// untitled, or simply absent from the property altogether.
    #[error("{kind}: sh:path {path} {reason}, so it belongs to no titled section of the form")]
    UnresolvedGroup {
        kind: String,
        path: String,
        reason: String,
    },

    /// `sh:closed` or `sh:deactivated` on a connector node shape. Neither is
    /// a member of `shape.constraints` in `shacl_rust` -- both are separate
    /// fields on `Shape` -- so W1's recording arm is structurally blind to
    /// them and `declarations()` returns a perfectly ordinary
    /// `Declaration` for either.
    #[error(
        "{kind}: the connector node shape declares {construct}, which this digest does not \
         model -- the form it would render is not the form the shape describes"
    )]
    ClosedOrDeactivatedShape { kind: String, construct: String },

    /// The node shape declares more direct `sh:property` children than
    /// `Declaration` could model -- `simple_path_iri` dropped the
    /// difference (an inverse, sequence or alternative `sh:path`). Reported
    /// as a *count* because a dropped property leaves no other trace.
    #[error(
        "{kind}: the node shape declares {declared} sh:property children but only {modelled} \
         could be modelled -- {} were dropped before any constraint was examined, most likely \
         a non-IRI sh:path (inverse, sequence or alternative)",
        .declared.saturating_sub(*.modelled)
    )]
    DroppedProperty {
        kind: String,
        declared: usize,
        modelled: usize,
    },

    /// The node shape declares more `sh:xone` alternatives than
    /// `Declaration` could model -- an alternative with no single-valued
    /// `sh:in` has no nameable discriminant and is dropped, leaving the
    /// union one branch short and reporting success.
    #[error(
        "{kind}: the node shape declares {declared} sh:xone alternative(s) but only {modelled} \
         yielded a variant -- {} were dropped, most likely for carrying no single-valued sh:in \
         to name a discriminant with",
        .declared.saturating_sub(*.modelled)
    )]
    DroppedVariant {
        kind: String,
        declared: usize,
        modelled: usize,
    },
}

/// Renders a [`DigestError`]'s optional `sh:path` for its message: either
/// the property it names, or the node shape itself.
fn location(path: &Option<String>) -> String {
    match path {
        Some(p) => format!("sh:path {p}"),
        None => "the connector node shape".to_string(),
    }
}

// ---------------------------------------------------------------------
// The digest
// ---------------------------------------------------------------------

/// Turns every connector shape in `ttl` into a [`FormSchema`], or reports
/// **every** reason it could not.
///
/// There is deliberately no "here are six schemas and one error" middle
/// ground: W5 wires this into a `build.rs`, and a build that goes green
/// while one connector is unrenderable is the failure this item exists to
/// prevent.
///
/// See the module doc comment for the totality rules, the requiredness rule,
/// and the ordering guarantees.
pub fn form_schemas(ttl: &str) -> Result<Vec<FormSchema>, Vec<DigestError>> {
    let mut pairs = match crate::validate::declarations_with_facts(ttl) {
        Ok(pairs) => pairs,
        Err(violations) => return Err(vec![DigestError::Declaration(violations)]),
    };

    // `find_connector_shapes` hands its result back in graph traversal
    // order, which is not stable between calls -- so fix the order once,
    // here, *before* walking, and let both the schema list and the error
    // list inherit it. Deliberately the ONLY ordering of connectors in this
    // module: sorting the finished schemas again afterwards would establish
    // the same guarantee twice, and a guarantee established twice is one
    // that no single change can be shown to break.
    //
    // Keyed on the derived kind first (the documented order, and the key
    // every layer above joins on), then on the target class and shape IRI,
    // which are total even for a declaration whose kind cannot be derived at
    // all -- itself one of the cases that has to be *reported*
    // deterministically.
    pairs.sort_by_cached_key(|(d, _)| {
        (
            kind_of_target_class(&d.target_class),
            d.target_class.clone(),
            d.shape.clone(),
        )
    });

    let mut schemas = Vec::new();
    let mut errors = Vec::new();
    for (declaration, facts) in &pairs {
        match build_form_schema(declaration, facts) {
            Ok(schema) => schemas.push(schema),
            Err(mut e) => errors.append(&mut e),
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(schemas)
}

/// A field, and the group it belongs to, once every check on it has passed.
struct PlacedField {
    group_iri: String,
    field: FieldDescriptor,
}

fn build_form_schema(
    declaration: &Declaration,
    facts: &NodeShapeFacts,
) -> Result<FormSchema, Vec<DigestError>> {
    let mut errors = Vec::new();

    // `kind` is the key every message below quotes, so it is resolved
    // first. When it cannot be derived, the fault is reported and the
    // target class's own local name stands in -- so a declaration with two
    // faults still names both, rather than the second one arriving
    // anonymous.
    let kind = match kind_of_target_class(&declaration.target_class) {
        Some(kind) => kind,
        None => {
            errors.push(DigestError::UndeckedKind {
                target_class: declaration.target_class.clone(),
            });
            local_name(&declaration.target_class)
        }
    };

    check_node_shape(declaration, facts, &kind, &mut errors);

    let placed = build_fields(declaration, &kind, &mut errors);
    let groups = build_groups(declaration, placed, &kind, &mut errors);
    let field_order: Vec<String> = groups
        .iter()
        .flat_map(|g| g.fields.iter().map(|f| f.id.clone()))
        .collect();
    let variants = build_variant_rules(declaration, &field_order);

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(FormSchema {
        label: declaration.label.clone().unwrap_or_else(|| kind.clone()),
        kind,
        description: declaration.description.clone(),
        ui_shape: declaration.ui_shape.clone(),
        groups,
        variants,
    })
}

/// The node-level half of totality: the four constructs that never reach
/// `Declaration.properties` at all.
fn check_node_shape(
    declaration: &Declaration,
    facts: &NodeShapeFacts,
    kind: &str,
    errors: &mut Vec<DigestError>,
) {
    if facts.closed {
        errors.push(DigestError::ClosedOrDeactivatedShape {
            kind: kind.to_string(),
            construct: "sh:closed".to_string(),
        });
    }
    if facts.deactivated {
        errors.push(DigestError::ClosedOrDeactivatedShape {
            kind: kind.to_string(),
            construct: "sh:deactivated".to_string(),
        });
    }
    for construct in &facts.non_xone_constraints {
        errors.push(DigestError::UnhandledConstraint {
            kind: kind.to_string(),
            path: None,
            construct: construct.clone(),
        });
    }
    if let Some(value) = declaration.ui_shape.as_deref()
        && !KNOWN_UI_SHAPES.contains(&value)
    {
        errors.push(unknown_ui_shape(kind, None, value));
    }
    if declaration.properties.len() != facts.declared_property_count {
        errors.push(DigestError::DroppedProperty {
            kind: kind.to_string(),
            declared: facts.declared_property_count,
            modelled: declaration.properties.len(),
        });
    }
    if declaration.variants.len() != facts.declared_variant_count {
        errors.push(DigestError::DroppedVariant {
            kind: kind.to_string(),
            declared: facts.declared_variant_count,
            modelled: declaration.variants.len(),
        });
    }
}

fn unknown_ui_shape(kind: &str, path: Option<String>, value: &str) -> DigestError {
    DigestError::UnknownUiShape {
        kind: kind.to_string(),
        path,
        value: value.to_string(),
        known: KNOWN_UI_SHAPES.join(", "),
    }
}

/// One [`FieldDescriptor`] per flat property, in a total order, with every
/// property-level totality check applied on the way past.
fn build_fields(
    declaration: &Declaration,
    kind: &str,
    errors: &mut Vec<DigestError>,
) -> Vec<PlacedField> {
    // Sorted here, once, and never re-sorted: bucketing into groups below
    // preserves relative order, so this single sort is what makes both the
    // rendered field order and the error order deterministic. `sh:order` is
    // optional; an absent one sorts last rather than first, matching
    // `sort_groups`'s own rule in `model.rs` (and unlike `Option`'s derived
    // ordering, which puts `None` at the wrong end).
    let mut ordered: Vec<(bool, Option<i64>, String, &PropertyShape)> = declaration
        .properties
        .iter()
        .map(|p| (p.order.is_none(), p.order, local_name(&p.path), p))
        .collect();
    ordered.sort_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));

    let governed = variant_governed_ids(declaration);

    let mut placed = Vec::new();
    for (_, _, id, property) in ordered {
        // All three checks run on every property, unconditionally, and the
        // field is dropped only afterwards. One property shape really can
        // carry several independent faults at once -- the vendored forgejo
        // fixture's `groupMapping` carries `sh:node`,
        // `contreforts:uiShape "mapping-table"` AND no `sh:datatype` -- and
        // a digest that returned at the first of them would report one
        // third of what a maintainer has to fix.
        check_property_constructs(property, kind, &id, errors);
        let control = resolve_control(property, kind, &id, errors);
        let group_iri = resolve_group(declaration, property, kind, &id, errors);
        let (Some(control), Some(group_iri)) = (control, group_iri) else {
            continue;
        };

        let variant_governed = governed.contains(&id);
        placed.push(PlacedField {
            group_iri,
            field: FieldDescriptor {
                label: property.name.clone().unwrap_or_else(|| id.clone()),
                id,
                description: property.description.clone(),
                control,
                // The whole point: a variant-governed field's flat
                // `min_count` is `merge_variant_requiredness`'s fold, not a
                // declared fact, and trusting it produces a form demanding
                // mutually exclusive fields at once.
                required: !variant_governed && property.min_count.is_some_and(|c| c > 0),
                variant_governed,
                secret: property.secret,
                config_field: property.config_field.clone(),
                ui_shape: property.ui_shape.clone(),
            },
        });
    }
    placed
}

/// Every field id named by any `sh:xone` alternative, **minus** the
/// discriminators. The discriminator appears in every alternative's own
/// property list (it is what selects them) but is not governed by them --
/// it governs them, and its `sh:minCount 1` at the flat level is a real,
/// declared fact.
fn variant_governed_ids(declaration: &Declaration) -> HashSet<String> {
    let discriminants: HashSet<String> = declaration
        .variants
        .iter()
        .map(|v| local_name(&v.discriminant_path))
        .collect();
    declaration
        .variants
        .iter()
        .flat_map(|v| v.properties.iter().map(|p| local_name(&p.path)))
        .filter(|id| !discriminants.contains(id))
        .collect()
}

/// Control resolution, in the declared order of precedence. Returns `None`
/// only after pushing the error that says why -- never a fallback.
fn resolve_control(
    property: &PropertyShape,
    kind: &str,
    id: &str,
    errors: &mut Vec<DigestError>,
) -> Option<Control> {
    // 1. `contreforts:secret` beats everything: a secret rendered as a
    //    number box is a secret in the DOM.
    if property.secret {
        return Some(Control::Secret);
    }
    // 2. `sh:in` beats `sh:datatype`: an enumeration rendered as free text
    //    is an enumeration nobody enforces.
    if let Some(values) = &property.in_values {
        return Some(Control::Select {
            options: values
                .iter()
                .map(|value| SelectOption {
                    value: value.clone(),
                    label: humanise(value),
                })
                .collect(),
        });
    }
    // 3. `sh:datatype`, from a closed table.
    let Some(datatype) = property.datatype.as_deref() else {
        errors.push(DigestError::NoDatatypeAndNoIn {
            kind: kind.to_string(),
            path: id.to_string(),
        });
        return None;
    };
    const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
    let Some(local) = datatype.strip_prefix(XSD) else {
        errors.push(DigestError::UnmappedDatatype {
            kind: kind.to_string(),
            path: id.to_string(),
            datatype: datatype.to_string(),
        });
        return None;
    };
    match local {
        "integer" | "int" | "long" | "short" | "byte" | "nonNegativeInteger"
        | "positiveInteger" => Some(Control::Integer {
            min: integer_bound(
                property.min_inclusive.as_deref(),
                kind,
                id,
                "MinInclusive",
                errors,
            ),
            max: integer_bound(
                property.max_inclusive.as_deref(),
                kind,
                id,
                "MaxInclusive",
                errors,
            ),
        }),
        "decimal" | "float" | "double" => Some(Control::Decimal),
        "boolean" => Some(Control::Boolean),
        "date" | "dateTime" => Some(Control::Date),
        "string" | "anyURI" => Some(Control::Text {
            pattern: property.pattern.clone(),
            min_length: property.min_length,
        }),
        // 4. Anything else is a NAMED failure. Not a fallback to Text, not
        //    an omission.
        _ => {
            errors.push(DigestError::UnmappedDatatype {
                kind: kind.to_string(),
                path: id.to_string(),
                datatype: datatype.to_string(),
            });
            None
        }
    }
}

/// `sh:minInclusive`/`sh:maxInclusive` on an integer-typed field.
///
/// `PropertyShape` keeps a bound as its literal's raw lexical form,
/// deliberately -- that layer does not know the property's range and will
/// not narrow it. Here the range *is* known, so this is the one parse. A
/// bound that will not parse as an integer on an integer-typed field is a
/// declared constraint the rendered form would silently not enforce, so it
/// is reported rather than dropped to `None`.
fn integer_bound(
    raw: Option<&str>,
    kind: &str,
    id: &str,
    construct: &str,
    errors: &mut Vec<DigestError>,
) -> Option<i64> {
    let raw = raw?;
    match raw.parse::<i64>() {
        Ok(bound) => Some(bound),
        Err(_) => {
            errors.push(DigestError::UnhandledConstraint {
                kind: kind.to_string(),
                path: Some(id.to_string()),
                construct: format!("{construct} (non-integer bound {raw:?} on an integer field)"),
            });
            None
        }
    }
}

/// The property-level constructs that have nothing to do with picking a
/// control, but which would otherwise vanish: W1's `unhandled` record, and
/// an unregistered `contreforts:uiShape`.
///
/// Deliberately *not* short-circuited against `resolve_control`'s own
/// failure, and deliberately reporting every entry rather than the first:
/// the vendored forgejo fixture's `groupMapping` carries `sh:node` **and**
/// `contreforts:uiShape "mapping-table"` on one property shape, so a
/// fail-fast digest could only ever report one of the two.
fn check_property_constructs(
    property: &PropertyShape,
    kind: &str,
    id: &str,
    errors: &mut Vec<DigestError>,
) {
    for construct in &property.unhandled {
        // `Constraint::Node` is what W1's recording arm calls `sh:node`.
        // Given its own variant so the message names the construct rather
        // than a shacl-rust enum discriminant nobody outside this crate has
        // to know.
        if construct == "Node" {
            errors.push(DigestError::NestedShape {
                kind: kind.to_string(),
                path: id.to_string(),
            });
        } else {
            errors.push(DigestError::UnhandledConstraint {
                kind: kind.to_string(),
                path: Some(id.to_string()),
                construct: construct.clone(),
            });
        }
    }
    if let Some(value) = property.ui_shape.as_deref()
        && !KNOWN_UI_SHAPES.contains(&value)
    {
        errors.push(unknown_ui_shape(kind, Some(id.to_string()), value));
    }
}

/// The `sh:PropertyGroup` a field belongs to, once it is known to resolve to
/// both a label and an order. Returns `None` after pushing the error that
/// says why -- a field whose section has no heading is a field the user
/// cannot find, and dropping it silently is worse.
///
/// Zero real connectors pay for this: all 45 top-level properties tree-wide
/// carry an `sh:group`, and all 17 groups they name resolve to both an
/// `rdfs:label` and an `sh:order`.
fn resolve_group(
    declaration: &Declaration,
    property: &PropertyShape,
    kind: &str,
    id: &str,
    errors: &mut Vec<DigestError>,
) -> Option<String> {
    let unresolved = |reason: String| DigestError::UnresolvedGroup {
        kind: kind.to_string(),
        path: id.to_string(),
        reason,
    };

    let Some(iri) = property.group.as_deref() else {
        errors.push(unresolved("declares no sh:group at all".to_string()));
        return None;
    };
    let Some(group) = declaration.groups.iter().find(|g| g.iri == iri) else {
        errors.push(unresolved(format!(
            "names sh:group <{iri}>, which resolved to no group descriptor"
        )));
        return None;
    };
    if !group.declared {
        errors.push(unresolved(format!(
            "names sh:group <{iri}>, which is never itself typed a sh:PropertyGroup (a dangling \
             reference)"
        )));
        return None;
    }
    match (group.label.as_deref(), group.order) {
        (Some(_), Some(_)) => Some(iri.to_string()),
        (label, order) => {
            let mut missing = Vec::new();
            if label.is_none() {
                missing.push("rdfs:label");
            }
            if order.is_none() {
                missing.push("sh:order");
            }
            errors.push(unresolved(format!(
                "names sh:group <{iri}>, which declares no {}",
                missing.join(" and no ")
            )));
            None
        }
    }
}

/// Buckets the placed fields into their sections, in `(sh:order, local
/// name)` order. Every group referenced here has already been proven to
/// carry both, by [`resolve_group`].
fn build_groups(
    declaration: &Declaration,
    placed: Vec<PlacedField>,
    kind: &str,
    errors: &mut Vec<DigestError>,
) -> Vec<GroupSection> {
    let by_iri: HashMap<&str, &GroupDescriptor> = declaration
        .groups
        .iter()
        .map(|g| (g.iri.as_str(), g))
        .collect();

    let mut sections: Vec<GroupSection> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for PlacedField { group_iri, field } in placed {
        let position = match index.get(&group_iri) {
            Some(&position) => position,
            None => {
                // `resolve_group` already proved this IRI names a declared
                // group carrying both an `rdfs:label` and an `sh:order`, so
                // the `else` arm below is unreachable for any input --
                // it exists because the alternative is a section rendered
                // with an invented heading, which is the exact failure mode
                // this whole module is here to make impossible. If it ever
                // fires it is a bug in this file, and it says so.
                let resolved = by_iri
                    .get(group_iri.as_str())
                    .and_then(|d| Some((d.label.clone()?, d.order?)));
                let Some((label, order)) = resolved else {
                    errors.push(DigestError::UnresolvedGroup {
                        kind: kind.to_string(),
                        path: field.id.clone(),
                        reason: format!(
                            "names sh:group <{group_iri}>, which lost its label or order between \
                             resolution and grouping -- this is a bug in form_schema.rs, not in \
                             the declaration"
                        ),
                    });
                    continue;
                };
                sections.push(GroupSection {
                    id: local_name(&group_iri),
                    label,
                    order,
                    fields: Vec::new(),
                });
                index.insert(group_iri.clone(), sections.len() - 1);
                sections.len() - 1
            }
        };
        sections[position].fields.push(field);
    }

    sections.sort_by(|a, b| (a.order, &a.id).cmp(&(b.order, &b.id)));
    sections
}

/// One [`VariantRule`] per `sh:xone` alternative, from `Declaration.variants`
/// **alone**. See the module doc comment for why the flat list is not a
/// source of truth here, and [`VariantRule`] for the discriminator
/// exclusion and the `hidden` set difference.
fn build_variant_rules(declaration: &Declaration, field_order: &[String]) -> Vec<VariantRule> {
    let position = |id: &String| {
        field_order
            .iter()
            .position(|f| f == id)
            // A variant field the flat list does not restate has no
            // rendered position; sorted last, by id, rather than at an
            // arbitrary one. Every variant field in every real declaration
            // is restated flat.
            .unwrap_or(usize::MAX)
    };
    let sorted = |mut ids: Vec<String>| {
        ids.sort_by(|a, b| (position(a), a).cmp(&(position(b), b)));
        ids.dedup();
        ids
    };

    let union: HashSet<String> = declaration
        .variants
        .iter()
        .flat_map(|v| v.properties.iter().map(|p| local_name(&p.path)))
        .collect();

    declaration
        .variants
        .iter()
        .map(|variant| {
            let discriminant_field = local_name(&variant.discriminant_path);
            let own: HashSet<String> = variant
                .properties
                .iter()
                .map(|p| local_name(&p.path))
                .filter(|id| *id != discriminant_field)
                .collect();
            let shown = sorted(own.iter().cloned().collect());
            let required = sorted(
                variant
                    .properties
                    .iter()
                    .filter(|p| p.min_count.is_some_and(|c| c > 0))
                    .map(|p| local_name(&p.path))
                    .filter(|id| *id != discriminant_field)
                    .collect(),
            );
            let hidden = sorted(
                union
                    .iter()
                    .filter(|id| !own.contains(*id) && **id != discriminant_field)
                    .cloned()
                    .collect(),
            );
            VariantRule {
                discriminant_field,
                discriminant_value: variant.discriminant_value.clone(),
                shown,
                required,
                hidden,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------
// Small, shared derivations
// ---------------------------------------------------------------------

/// An IRI's local name: the fragment after the last `#` or `/`. The settled
/// identity for both `sh:path` and `sh:group` everywhere in this system --
/// see [`FieldDescriptor::id`].
fn local_name(iri: &str) -> String {
    iri.rsplit(['#', '/'])
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or(iri)
        .to_string()
}

/// The connector kind implied by a `sh:targetClass` IRI's own namespace
/// segment (e.g. `".../ontologies/forgejo#ForgejoConnector"` -> `"forgejo"`).
///
/// D2 requires every declaration to mint its terms in its own connector's
/// namespace, so this is a real derivation from the declared IRI rather than
/// a hand-maintained kind table -- and it is a *port* of
/// `kind_of_target_class` in contreforts-config-api's `product_graph` route,
/// not a second invention, so the `kind` this digest emits is the same
/// string that route, `contreforts_product::ContributorInfo` and
/// `contreforts_config::connector_type()` already agree on.
///
/// `split_once`, not `split('#').next()`: the latter returns `Some` even
/// when the IRI has no `#` at all, silently treating a fragment-less IRI as
/// its own namespace instead of reporting that no kind is derivable.
fn kind_of_target_class(target_class: &str) -> Option<String> {
    let (namespace, _fragment) = target_class.split_once('#')?;
    namespace
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .map(str::to_string)
}

/// snake_case to sentence case, for an `sh:in` member's display label.
///
/// Only `_` is treated as a word separator: `-` is left alone, because a
/// declared value is stored verbatim and this crate has no basis for
/// deciding that a hyphen in someone's enumeration is a word break rather
/// than part of the term.
fn humanise(value: &str) -> String {
    let spaced = value.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanise_is_a_pure_function_of_the_declared_value() {
        assert_eq!(humanise("client_credentials"), "Client credentials");
        assert_eq!(humanise("slow_and_careful"), "Slow and careful");
        assert_eq!(humanise("basic"), "Basic");
        assert_eq!(humanise(""), "");
    }

    #[test]
    fn kind_and_local_name_agree_with_the_product_graph_route() {
        assert_eq!(
            kind_of_target_class(
                "https://contreforts.ds-labs.org/ontologies/forgejo#ForgejoConnector"
            )
            .as_deref(),
            Some("forgejo")
        );
        assert_eq!(
            kind_of_target_class("https://contreforts.ds-labs.org/ontologies/forgejo/Connector"),
            None,
            "a fragment-less IRI yields no kind rather than a guess"
        );
        assert_eq!(
            local_name("https://contreforts.ds-labs.org/ontologies/caldav#password"),
            "password"
        );
    }
}
