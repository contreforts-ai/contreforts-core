//! Phase F item W4 (contreforts/contreforts-core#34): the TOTAL digest that
//! turns a validated declaration into a renderable form schema.
//!
//! The failure this whole module exists to prevent is a form field that
//! silently does not render. Everything above `form_schemas` is a renderer --
//! the webui has no RDF awareness at all -- so a SHACL construct the digest
//! does not understand has exactly one place left where it can become
//! visible: here, as a named `DigestError`. Never a fallback control, never
//! an omitted field, never a schema that is quietly one field short.
//!
//! Two properties of this crate's inputs shape almost every assertion below,
//! and both were measured, not assumed (workspace develop @ `4cf06df`):
//!
//! 1. **`Declaration.properties` is in HASH ORDER, and the hash order changes
//!    between runs of the same binary.** Two consecutive `cargo test` runs
//!    reported o365's nine properties as
//!    `["clientId", "userPrincipal", "tenantId", "label", ...]` and
//!    `["tenantId", "userPrincipal", "clientId", "authMode", ...]`. So does
//!    `variants[].properties`. Every ordered assertion here is therefore a
//!    real test of the digest's own sort, not an accidental restatement of
//!    the parser's traversal -- and any implementation that emits ids in the
//!    order it happened to walk them will fail these tests intermittently,
//!    which is the point.
//! 2. **Requiredness at the flat level is actively wrong for a union
//!    connector.** `merge_variant_requiredness` (src/model.rs) folds an
//!    alternative's `sh:minCount` onto the flat entry whenever exactly one
//!    alternative requires it, so caldav's flat list marks `username`,
//!    `password` AND `token` required simultaneously -- three mutually
//!    exclusive fields -- and o365's marks four. A digest that read
//!    requiredness from the flat list would generate an unsatisfiable form.

use contreforts_declaration::{
    Control, DigestError, FieldDescriptor, FormSchema, GroupSection, SelectOption, VariantRule,
    declarations, form_schemas,
};
use oxigraph::model::NamedNodeRef;
use shacl_rust::rdf::read_graph_from_string;

// -- Fixtures ------------------------------------------------------------
//
// Fourteen files are added by this item: two real declarations vendored
// verbatim (caldav, o365 as of workspace develop @ 4cf06df) and twelve
// synthetics. The vendored pair is what keeps the happy path honest -- a
// synthetic connector can always be written to agree with whatever the digest
// happens to do. The synthetics exist for the opposite reason: every totality
// hole W4 closes has ZERO instances in the real tree, and an error arm no
// input reaches is not a guarantee, it is unreviewed code.

const O365: &str = include_str!("fixtures/o365-declaration-current.ttl");
/// The 2026-07 spike copy, kept in this directory since
/// contreforts-workspace#27 and now drifted from the connector: it carries a
/// node-level `sh:sparql` the real declaration dropped. Used here only to
/// prove the node-level rule against a REAL file rather than a synthetic.
const O365_SPIKE: &str = include_str!("fixtures/o365-declaration.ttl");
const CALDAV: &str = include_str!("fixtures/caldav-declaration.ttl");
const FORGEJO: &str = include_str!("fixtures/forgejo-declaration.ttl");
const DATATYPE_CONTROLS: &str = include_str!("fixtures/synthetic-datatype-controls.ttl");
const CONTROL_PRECEDENCE: &str = include_str!("fixtures/synthetic-control-precedence.ttl");
const UNMAPPED_DATATYPE: &str = include_str!("fixtures/synthetic-unmapped-datatype.ttl");
const UNTYPED_PROPERTY: &str = include_str!("fixtures/synthetic-untyped-property.ttl");
const CLOSED_SHAPE: &str = include_str!("fixtures/synthetic-closed-shape.ttl");
const DEACTIVATED_SHAPE: &str = include_str!("fixtures/synthetic-deactivated-shape.ttl");
const INVERSE_PATH: &str = include_str!("fixtures/synthetic-inverse-path.ttl");
const XONE_WITHOUT_DISCRIMINANT: &str =
    include_str!("fixtures/synthetic-xone-without-discriminant.ttl");
const NODE_LEVEL_CONSTRAINT: &str = include_str!("fixtures/synthetic-node-level-constraint.ttl");
const UNKNOWN_NODE_UI_SHAPE: &str = include_str!("fixtures/synthetic-unknown-node-ui-shape.ttl");
const UNDECKED_KIND: &str = include_str!("fixtures/synthetic-undecked-kind.ttl");
const ENTITY_NEAR_MISS: &str = include_str!("fixtures/synthetic-entity-vocabulary-near-miss.ttl");
const UNHANDLED_CONSTRAINT: &str = include_str!("fixtures/synthetic-unhandled-constraint.ttl");
const SYNTHETIC_MINIMAL: &str = include_str!("fixtures/synthetic-minimal-declaration.ttl");
const DANGLING_GROUP: &str = include_str!("fixtures/dangling-group-reference.ttl");
const NAIVE_XONE: &str = include_str!("fixtures/o365-shape-naive-xone.ttl");
const MALFORMED: &str = include_str!("fixtures/malformed.ttl");

// -- Helpers -------------------------------------------------------------

fn ok(ttl: &str) -> Vec<FormSchema> {
    match form_schemas(ttl) {
        Ok(schemas) => schemas,
        Err(errors) => panic!(
            "expected a form schema, got {} digest error(s): {}",
            errors.len(),
            display_all(&errors)
        ),
    }
}

fn one(ttl: &str) -> FormSchema {
    let mut schemas = ok(ttl);
    assert_eq!(
        schemas.len(),
        1,
        "expected exactly one schema from a single-connector fixture, got {}",
        schemas.len()
    );
    schemas.remove(0)
}

fn err(ttl: &str) -> Vec<DigestError> {
    match form_schemas(ttl) {
        Ok(schemas) => panic!(
            "expected digest errors, got {} schema(s) with fields {:?} -- a construct this \
             digest does not understand became an absent field instead of a named failure",
            schemas.len(),
            schemas.iter().map(field_ids).collect::<Vec<_>>()
        ),
        Err(errors) => errors,
    }
}

fn display_all(errors: &[DigestError]) -> String {
    errors
        .iter()
        .map(|e| format!("[{}] {e}", variant_name(e)))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// The `DigestError` variant's own discriminant name, taken from its derived
/// `Debug` output rather than by matching. Mirrors this crate's own
/// `constraint_variant_name` (src/model.rs), and for the same reason: the
/// tests can then pin WHICH error was reported without also pinning whether
/// the variant is a tuple or a struct variant, or what its fields are called
/// -- decisions that belong to the implementation, not to its contract.
fn variant_name(error: &DigestError) -> String {
    let debug = format!("{error:?}");
    debug
        .split(['(', '{', ' '])
        .next()
        .unwrap_or(&debug)
        .to_string()
}

fn variant_names(errors: &[DigestError]) -> Vec<String> {
    let mut names: Vec<String> = errors.iter().map(variant_name).collect();
    names.sort();
    names.dedup();
    names
}

/// Asserts at least one error is of `name` and returns its `Display` text.
fn message_of(errors: &[DigestError], name: &str) -> String {
    let found: Vec<String> = errors
        .iter()
        .filter(|e| variant_name(e) == name)
        .map(|e| e.to_string())
        .collect();
    assert!(
        !found.is_empty(),
        "expected a DigestError::{name}, got {:?} -- full report: {}",
        variant_names(errors),
        display_all(errors)
    );
    found.join(" ")
}

/// Every field id in a schema, flattened across its groups IN ORDER. This is
/// the sequence a renderer walks top to bottom, so it is the sequence the
/// tests pin.
fn field_ids(schema: &FormSchema) -> Vec<String> {
    schema
        .groups
        .iter()
        .flat_map(|g| g.fields.iter().map(|f| f.id.clone()))
        .collect()
}

fn field<'a>(schema: &'a FormSchema, id: &str) -> &'a FieldDescriptor {
    schema
        .groups
        .iter()
        .flat_map(|g| g.fields.iter())
        .find(|f| f.id == id)
        .unwrap_or_else(|| {
            panic!(
                "no field {id:?} in the {} schema; it has {:?}",
                schema.kind,
                field_ids(schema)
            )
        })
}

fn group<'a>(schema: &'a FormSchema, id: &str) -> &'a GroupSection {
    schema
        .groups
        .iter()
        .find(|g| g.id == id)
        .unwrap_or_else(|| {
            panic!(
                "no group {id:?} in the {} schema; it has {:?}",
                schema.kind,
                schema.groups.iter().map(|g| &g.id).collect::<Vec<_>>()
            )
        })
}

fn rule<'a>(schema: &'a FormSchema, discriminant_value: &str) -> &'a VariantRule {
    schema
        .variants
        .iter()
        .find(|v| v.discriminant_value == discriminant_value)
        .unwrap_or_else(|| {
            panic!(
                "no variant rule for {discriminant_value:?} in the {} schema; it has {:?}",
                schema.kind,
                schema
                    .variants
                    .iter()
                    .map(|v| &v.discriminant_value)
                    .collect::<Vec<_>>()
            )
        })
}

fn options(control: &Control) -> Vec<(String, String)> {
    match control {
        Control::Select { options } => options
            .iter()
            .map(|SelectOption { value, label }| (value.clone(), label.clone()))
            .collect(),
        other => panic!("expected Control::Select, got {other:?}"),
    }
}

/// The local name of every subject carrying `contreforts:entityKind` in
/// `ttl` -- phases E1-E4's entity vocabulary, which shares a graph with the
/// connector shapes and must never be rendered as form fields.
fn entity_class_local_names(ttl: &str) -> Vec<String> {
    const ENTITY_KIND: NamedNodeRef<'static> = NamedNodeRef::new_unchecked(
        "https://contreforts.ds-labs.org/ontologies/declaration#entityKind",
    );
    let graph = read_graph_from_string(ttl, "turtle").expect("fixture is valid Turtle");
    let mut names: Vec<String> = graph
        .iter()
        .filter(|triple| triple.predicate == ENTITY_KIND)
        .map(|triple| local_name(&triple.subject.to_string()))
        .collect();
    names.sort();
    names.dedup();
    names
}

fn local_name(term: &str) -> String {
    term.trim_matches(['<', '>'])
        .rsplit(['#', '/'])
        .next()
        .unwrap_or(term)
        .to_string()
}

// ========================================================================
// The happy path, against the two vendored REAL declarations
// ========================================================================

/// The o365 fixture: one connector in, one schema out, with all nine
/// declared fields present and in a fixed order.
///
/// Both `label` and `description` come from the node shape's own W3-era
/// `rdfs:label`/`sh:description`. The FALLBACK arm -- `label` standing in as
/// the kind when a declaration carries neither -- is pinned separately by
/// `a_declaration_with_no_node_label_falls_back_to_its_kind`; without that
/// pair, a digest that ignored `Declaration.label` entirely would pass one of
/// them.
#[test]
fn o365_fixture_yields_one_schema_with_all_nine_fields_in_group_order() {
    let schema = one(O365);

    assert_eq!(schema.kind, "o365");
    assert_eq!(
        schema.label, "Microsoft 365",
        "the node shape's own rdfs:label, not the kind"
    );
    assert_eq!(
        schema.description.as_deref(),
        Some(
            "Syncs calendars and events from Microsoft 365 via the Microsoft Graph API using \
             OAuth2."
        )
    );
    assert_eq!(schema.ui_shape.as_deref(), Some("discriminated-union"));

    // Ordered: groups by (order, id), fields by (order, id). The parser hands
    // these over in a hash order that differs between runs -- see the module
    // doc comment -- so this sequence can only come from the digest's sort.
    assert_eq!(
        field_ids(&schema),
        vec![
            // ConnectionGroup, sh:order 1
            "label",
            // AuthenticationGroup, sh:order 2
            "authMode",
            "tenantId",
            "clientId",
            "clientSecret",
            "token",
            "refreshToken",
            // ScopeGroup, sh:order 3
            "userPrincipal",
            "customer",
        ]
    );

    // The same claim as a set, matching contreforts-core#34's own done-when
    // wording -- so a reordering and a dropped field cannot be confused.
    let mut sorted = field_ids(&schema);
    sorted.sort();
    assert_eq!(
        sorted,
        vec![
            "authMode",
            "clientId",
            "clientSecret",
            "customer",
            "label",
            "refreshToken",
            "tenantId",
            "token",
            "userPrincipal",
        ]
    );
}

/// Group sections carry a resolved label and order, and are nested, not flat.
/// The nesting is load-bearing rather than cosmetic: `sh:order` is NOT unique
/// within a connector -- o365 declares three fields at order 1 (`label`,
/// `authMode`, `userPrincipal`) and caldav does too -- so a flat sort by
/// `order` alone is ambiguous and would interleave groups.
#[test]
fn o365_groups_are_resolved_and_ordered_and_the_nesting_disambiguates_order() {
    let schema = one(O365);

    assert_eq!(
        schema
            .groups
            .iter()
            .map(|g| (g.id.as_str(), g.label.as_str(), g.order))
            .collect::<Vec<_>>(),
        vec![
            ("ConnectionGroup", "Connection", 1),
            ("AuthenticationGroup", "Authentication", 2),
            ("ScopeGroup", "Scope", 3),
        ]
    );

    assert_eq!(
        group(&schema, "AuthenticationGroup")
            .fields
            .iter()
            .map(|f| f.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "authMode",
            "tenantId",
            "clientId",
            "clientSecret",
            "token",
            "refreshToken"
        ]
    );

    // Three distinct fields all declare sh:order 1. They must land in three
    // DIFFERENT sections, each as that section's first field -- which is what
    // makes the nesting load-bearing: flattened by `order` alone they would
    // be three fields competing for one position.
    let sections_of_order_one: Vec<&str> = ["label", "authMode", "userPrincipal"]
        .into_iter()
        .map(|id| {
            let section = schema
                .groups
                .iter()
                .find(|g| g.fields.iter().any(|f| f.id == id))
                .unwrap_or_else(|| panic!("{id} landed in no section at all"));
            assert_eq!(
                section.fields[0].id, id,
                "{id} declares sh:order 1 and must be first in its own section"
            );
            section.id.as_str()
        })
        .collect();
    assert_eq!(
        sections_of_order_one,
        vec!["ConnectionGroup", "AuthenticationGroup", "ScopeGroup"]
    );
}

/// The hard requirement, o365 half: variant rules are computed from
/// `Declaration.variants` alone, and the discriminant is excluded from all
/// three id lists.
///
/// `hidden` cannot be read off the graph at all -- `build_variants`
/// (src/model.rs) filters out the `sh:maxCount 0` cross-exclusion markers
/// before they ever reach `variants[].properties` -- so it is the set
/// difference (union of every alternative's ids) - (this alternative's ids) -
/// discriminant, and nothing else.
#[test]
fn o365_variant_rules_are_derived_from_variants_and_exclude_the_discriminant() {
    let schema = one(O365);
    assert_eq!(schema.variants.len(), 2);

    let cc = rule(&schema, "client_credentials");
    assert_eq!(cc.discriminant_field, "authMode");
    assert_eq!(cc.shown, vec!["tenantId", "clientId", "clientSecret"]);
    assert_eq!(cc.required, vec!["tenantId", "clientId", "clientSecret"]);
    assert_eq!(cc.hidden, vec!["token", "refreshToken"]);

    let delegated = rule(&schema, "delegated");
    assert_eq!(delegated.discriminant_field, "authMode");
    assert_eq!(delegated.shown, vec!["token", "refreshToken"]);
    // refreshToken declares sh:minCount 0 inside the delegated alternative:
    // shown but optional. A rule that equated `shown` with `required` would
    // pass every other assertion here and fail this one.
    assert_eq!(delegated.required, vec!["token"]);
    assert_eq!(
        delegated.hidden,
        vec!["tenantId", "clientId", "clientSecret"]
    );

    // The discriminant carries sh:minCount 1 INSIDE all four alternatives
    // tree-wide, so a naive filter yields ["authMode", ...] in every list.
    for rule in &schema.variants {
        for (list_name, list) in [
            ("shown", &rule.shown),
            ("required", &rule.required),
            ("hidden", &rule.hidden),
        ] {
            assert!(
                !list.contains(&"authMode".to_string()),
                "the discriminant must not appear in {list_name} of the {:?} rule, got {list:?}",
                rule.discriminant_value
            );
        }
    }
}

/// The divergence between the flat list and the truth, asserted in ONE place
/// so it reads as documented rather than as a bug.
///
/// `declarations()` reports `min_count == Some(1)` for four mutually
/// exclusive o365 fields at once, because `merge_variant_requiredness` folds
/// each alternative's own `sh:minCount` back onto the flat entry. The digest
/// must therefore force `required = false` and `variant_governed = true` for
/// every field named by any alternative -- making flat requiredness for a
/// union field *unexpressible* rather than merely discouraged.
#[test]
fn flat_requiredness_is_suppressed_for_variant_governed_fields_although_the_model_asserts_it() {
    let declaration = declarations(O365)
        .expect("the o365 fixture validates")
        .remove(0);
    let flat_min_count = |id: &str| {
        declaration
            .properties
            .iter()
            .find(|p| p.path.ends_with(&format!("#{id}")))
            .unwrap_or_else(|| panic!("no flat property {id}"))
            .min_count
    };

    // Half one: what the flat model says. Four fields, two of which can never
    // be set at the same time as the other two.
    for id in ["tenantId", "clientId", "clientSecret", "token"] {
        assert_eq!(
            flat_min_count(id),
            Some(1),
            "Declaration.properties should still report {id} as required -- if this changed, \
             merge_variant_requiredness changed and the digest's suppression rule needs revisiting"
        );
    }
    assert_eq!(
        flat_min_count("refreshToken"),
        None,
        "refreshToken is required by no alternative, so nothing is folded onto it"
    );

    // Half two: what the schema says.
    let schema = one(O365);
    for id in [
        "tenantId",
        "clientId",
        "clientSecret",
        "token",
        "refreshToken",
    ] {
        let f = field(&schema, id);
        assert!(
            !f.required,
            "{id} is governed by a variant, so flat requiredness must not survive"
        );
        assert!(f.variant_governed, "{id} should be marked variant_governed");
    }

    // And the fields no alternative mentions keep their real flat requiredness.
    assert!(field(&schema, "label").required);
    assert!(!field(&schema, "label").variant_governed);
    assert!(!field(&schema, "userPrincipal").required);
    assert!(!field(&schema, "userPrincipal").variant_governed);

    // The discriminant is the exception that the naive rule gets wrong: it
    // appears in every alternative's own property list, but its requiredness
    // is genuinely flat -- the form always asks for it.
    assert!(
        field(&schema, "authMode").required,
        "the discriminant is required at the flat level and must stay required"
    );
    assert!(
        !field(&schema, "authMode").variant_governed,
        "the discriminant governs the variants; it is not governed BY them"
    );
}

/// The 20 property shapes inside the four real `sh:xone` alternatives carry no
/// `sh:name`, no `sh:description`, no `sh:group`, no `sh:order`, no
/// `contreforts:secret` and no `contreforts:configField` -- verified on all
/// four. So a `FieldDescriptor` built from a variant's copy of `clientSecret`
/// would be an unlabelled, ungrouped, NON-SECRET plain text box. This is the
/// test that catches that substitution.
#[test]
fn secret_fields_survive_the_union_with_their_label_group_and_secret_flag() {
    let schema = one(O365);
    for (id, label) in [
        ("clientSecret", "Client secret"),
        ("token", "Access token"),
        ("refreshToken", "Refresh token"),
    ] {
        let f = field(&schema, id);
        assert_eq!(
            f.control,
            Control::Secret,
            "{id} must resolve to a Secret control"
        );
        assert!(
            f.secret,
            "{id}: `secret` is surfaced explicitly as well as folded into the control, because \
             W7 and W8 both key elision on it"
        );
        assert_eq!(
            f.label, label,
            "{id} must keep the flat entry's own sh:name"
        );
        assert!(
            group(&schema, "AuthenticationGroup")
                .fields
                .iter()
                .any(|g| g.id == id),
            "{id} must land in its declared group"
        );
    }
}

/// Every id a `VariantRule` names must resolve to a real field of the same
/// schema. caldav's variant union is a subset of its 8 flat fields and o365's
/// of its 9, so this holds today -- and a rule naming a field the renderer
/// cannot find is a silently dead show/hide instruction.
#[test]
fn every_variant_rule_id_resolves_to_a_field_in_the_same_schema() {
    for ttl in [O365, CALDAV] {
        let schema = one(ttl);
        let ids = field_ids(&schema);
        assert!(
            !schema.variants.is_empty(),
            "{} declares a union",
            schema.kind
        );
        for rule in &schema.variants {
            let mut referenced = vec![rule.discriminant_field.clone()];
            referenced.extend(rule.shown.iter().cloned());
            referenced.extend(rule.required.iter().cloned());
            referenced.extend(rule.hidden.iter().cloned());
            for id in referenced {
                assert!(
                    ids.contains(&id),
                    "{}: variant rule {:?} names {id:?}, which is not a field of this schema {ids:?}",
                    schema.kind,
                    rule.discriminant_value
                );
            }
        }
    }
}

// ========================================================================
// caldav: the hard requirement, and the other arm of every fallback
// ========================================================================

/// The hard requirement, caldav half (contreforts/contreforts-workspace#60):
/// `basic` requires exactly username+password and hides token; `bearer`
/// requires exactly token. Resolved from `variants[]`, never from the flat
/// list -- which claims all three are required at once.
#[test]
fn caldav_variant_rules_require_only_their_own_alternatives_fields() {
    let schema = one(CALDAV);
    assert_eq!(schema.variants.len(), 2);

    let basic = rule(&schema, "basic");
    assert_eq!(basic.discriminant_field, "authMode");
    assert_eq!(basic.required, vec!["username", "password"]);
    assert_eq!(basic.shown, vec!["username", "password"]);
    assert_eq!(basic.hidden, vec!["token"]);

    let bearer = rule(&schema, "bearer");
    assert_eq!(bearer.discriminant_field, "authMode");
    assert_eq!(bearer.required, vec!["token"]);
    assert_eq!(bearer.shown, vec!["token"]);
    assert_eq!(bearer.hidden, vec!["username", "password"]);
}

/// caldav's flat list marks three mutually exclusive fields required at once.
/// The same pairing as the o365 test above, kept separately because caldav is
/// the connector whose own declaration.ttl says in prose (its authentication
/// section) that the flat restatement deliberately omits `sh:minCount`.
#[test]
fn caldav_flat_list_marks_three_mutually_exclusive_fields_required_and_the_schema_does_not() {
    let declaration = declarations(CALDAV)
        .expect("the caldav fixture validates")
        .remove(0);
    for id in ["username", "password", "token"] {
        assert_eq!(
            declaration
                .properties
                .iter()
                .find(|p| p.path.ends_with(&format!("#{id}")))
                .unwrap_or_else(|| panic!("no flat property {id}"))
                .min_count,
            Some(1),
            "the flat model claims {id} is required"
        );
    }

    let schema = one(CALDAV);
    for id in ["username", "password", "token"] {
        assert!(
            !field(&schema, id).required,
            "{id} must not be flatly required"
        );
        assert!(field(&schema, id).variant_governed);
    }
    // The non-union fields are unaffected.
    assert!(field(&schema, "instanceUrl").required);
    assert!(!field(&schema, "calendarHome").required);
}

/// The other arm of `FormSchema.label`'s fallback: caldav's node shape carries
/// a real `rdfs:label` and `sh:description` (W3), so neither is invented from
/// the kind. Paired with `o365_fixture_yields_one_schema_...`, which pins the
/// fallback itself.
#[test]
fn caldav_node_label_and_description_come_from_the_shape_not_the_kind() {
    let schema = one(CALDAV);
    assert_eq!(schema.kind, "caldav");
    assert_eq!(schema.label, "CalDAV");
    assert_eq!(
        schema.description.as_deref(),
        Some(
            "Syncs calendars and events from any CalDAV server (RFC 4791) using basic auth or a \
             bearer token."
        )
    );
    assert_eq!(schema.ui_shape.as_deref(), Some("discriminated-union"));
    assert_eq!(
        field_ids(&schema),
        vec![
            // ConnectionGroup, order 1
            "label",
            "instanceUrl",
            "calendarHome",
            // AuthenticationGroup, order 2
            "authMode",
            "username",
            "password",
            "token",
            // ScopeGroup, order 3
            "customer",
        ]
    );
}

/// `config_field` is carried through but is NOT the field id: `caldav:password`
/// is the field `password` and the storage path `auth.password`. 27 config
/// fields are declared tree-wide and they are not unique per local name across
/// connectors, so a digest that keyed fields by `configField` would collide.
#[test]
fn config_field_is_carried_through_and_is_not_the_field_id() {
    let schema = one(CALDAV);
    assert_eq!(
        field(&schema, "password").config_field.as_deref(),
        Some("auth.password")
    );
    assert_eq!(field(&schema, "password").id, "password");
    assert_eq!(
        field(&schema, "authMode").config_field.as_deref(),
        Some("auth.type")
    );
    assert_eq!(
        field(&schema, "instanceUrl").config_field.as_deref(),
        Some("url")
    );
    // Not every field declares one; absence is meaningful (D15) and must stay
    // absent rather than being backfilled from the id.
    assert_eq!(field(&schema, "label").config_field, None);
}

// ========================================================================
// Control resolution
// ========================================================================

/// `Control` is a CLOSED enum of exactly seven variants. This match has no
/// wildcard arm on purpose: an eighth variant, or a `#[non_exhaustive]`
/// attribute, makes this file stop compiling -- which is the only mechanical
/// way a test can hold a Rust enum closed.
fn control_name(control: &Control) -> &'static str {
    match control {
        Control::Text { .. } => "Text",
        Control::Integer { .. } => "Integer",
        Control::Decimal => "Decimal",
        Control::Boolean => "Boolean",
        Control::Date => "Date",
        Control::Secret => "Secret",
        Control::Select { .. } => "Select",
    }
}

#[test]
fn control_is_closed_with_exactly_seven_variants() {
    let sample = [
        Control::Text {
            pattern: None,
            min_length: None,
        },
        Control::Integer {
            min: None,
            max: None,
        },
        Control::Decimal,
        Control::Boolean,
        Control::Date,
        Control::Secret,
        Control::Select {
            options: Vec::new(),
        },
    ];
    let names: Vec<&str> = sample.iter().map(control_name).collect();
    assert_eq!(
        names,
        vec![
            "Text", "Integer", "Decimal", "Boolean", "Date", "Secret", "Select"
        ]
    );
}

/// `sh:in` becomes a `Select` whose options keep DECLARATION order (not
/// alphabetical: "delegated" would sort before "client_credentials" only by
/// accident here, but "basic"/"bearer" would not move at all -- so both
/// fixtures are asserted) and whose labels are humanised deterministically in
/// Rust: snake_case to sentence case.
///
/// Whether "Client credentials" is the right copy versus today's hand-written
/// "Delegated (access/refresh token)" is a product judgement no test can
/// settle (contreforts-core#34, Unsettled item 4). What this test settles is
/// that the humanisation is a pure function of the declared value.
#[test]
fn select_options_keep_declaration_order_and_are_humanised_deterministically() {
    assert_eq!(
        options(&field(&one(O365), "authMode").control),
        vec![
            (
                "client_credentials".to_string(),
                "Client credentials".to_string()
            ),
            ("delegated".to_string(), "Delegated".to_string()),
        ]
    );
    assert_eq!(
        options(&field(&one(CALDAV), "authMode").control),
        vec![
            ("basic".to_string(), "Basic".to_string()),
            ("bearer".to_string(), "Bearer".to_string()),
        ]
    );
}

/// `Control::Text` carries `sh:minLength` and `sh:pattern`. All seven declared
/// patterns tree-wide are the same string; it is asserted byte-for-byte,
/// backslash included, because a pattern that survives the digest re-escaped
/// is a pattern the renderer will silently reject valid input with.
#[test]
fn text_control_carries_min_length_and_the_pattern_byte_for_byte() {
    assert_eq!(
        field(&one(O365), "label").control,
        Control::Text {
            pattern: None,
            min_length: Some(1),
        },
        "o365:label declares sh:minLength 1 and no sh:pattern"
    );
    assert_eq!(
        field(&one(CALDAV), "instanceUrl").control,
        Control::Text {
            pattern: Some("^https?://[^\\s]+$".to_string()),
            min_length: None,
        },
        "caldav:instanceUrl declares sh:pattern and no sh:minLength"
    );
}

/// `Control::Integer` carries INTEGER bounds, not floats: `PropertyShape`
/// keeps `sh:minInclusive`/`sh:maxInclusive` as their raw lexical form
/// ("1", "65535"), and coercing those through `f64` is exactly what
/// `numeric_or_null` (config-api's product_graph route) refuses to do, since
/// `1` round-tripping as `1.0` is not equal under `serde_json::Number`.
#[test]
fn integer_control_carries_parsed_integer_bounds() {
    let schema = one(DATATYPE_CONTROLS);
    assert_eq!(
        field(&schema, "listenPort").control,
        Control::Integer {
            min: Some(1),
            max: Some(65535),
        },
        "listenPort declares sh:datatype xsd:integer with sh:minInclusive 1 and \
         sh:maxInclusive 65535"
    );
    // A bound-less integer is still an Integer, with no bounds invented.
    assert_eq!(
        field(&schema, "cacheBytes").control,
        Control::Integer {
            min: None,
            max: None,
        },
        "cacheBytes declares sh:datatype xsd:long and no bounds"
    );
}

/// The datatype mapping table in full, including every arm with ZERO triples
/// anywhere in the tree. Without this, `Decimal`, `Date` and `Boolean` -- and
/// every integer alias but `xsd:integer` itself -- are unreachable code
/// wearing the costume of a census.
#[test]
fn every_mapped_datatype_including_the_unexercised_ones_resolves_to_its_control() {
    let schema = one(DATATYPE_CONTROLS);
    let expected = [
        ("label", "Text"),
        ("listenPort", "Integer"),
        ("retries", "Integer"),       // xsd:nonNegativeInteger
        ("cacheBytes", "Integer"),    // xsd:long
        ("enabled", "Boolean"),       // xsd:boolean -- zero connector triples
        ("startDate", "Date"),        // xsd:date -- zero triples anywhere
        ("startedAt", "Date"),        // xsd:dateTime -- zero triples anywhere
        ("samplingRatio", "Decimal"), // xsd:decimal -- zero triples anywhere
        ("latencyBudget", "Decimal"), // xsd:double -- zero triples anywhere
        ("homepage", "Text"),         // xsd:anyURI -- zero triples anywhere
    ];
    for (id, control) in expected {
        assert_eq!(
            control_name(&field(&schema, id).control),
            control,
            "{id} resolved to {:?}",
            field(&schema, id).control
        );
    }
}

/// Steps 1 and 2 of the resolution order, neither of which any declared field
/// exercises: no property today is both secret and enumerated, and none is
/// both enumerated and typed. Without this fixture whichever arm happened to
/// be written first would win silently -- a secret rendered as a number box,
/// or an enumeration rendered as free text.
#[test]
fn secret_beats_datatype_and_sh_in_beats_datatype() {
    let schema = one(CONTROL_PRECEDENCE);

    let secret_port = field(&schema, "secretPort");
    assert_eq!(
        secret_port.control,
        Control::Secret,
        "contreforts:secret wins over sh:datatype xsd:integer"
    );
    assert!(secret_port.secret);

    assert_eq!(
        options(&field(&schema, "mode").control),
        vec![
            ("fast".to_string(), "Fast".to_string()),
            (
                "slow_and_careful".to_string(),
                "Slow and careful".to_string()
            ),
        ],
        "sh:in wins over sh:datatype xsd:string, and humanisation handles multi-word values"
    );
}

// ========================================================================
// Determinism and the wire shape
// ========================================================================

/// Two calls on the same input serialise byte-identically. Measured, not
/// assumed: this is a STRONG assertion here, because the underlying traversal
/// order is not stable even within one process -- with the digest's own sort
/// removed, this single test failed on 6 of 6 consecutive runs, i.e. two
/// `form_schemas` calls in the same binary already disagreed with each other.
///
/// It also pins the maintainer's decision that there is NO prefill
/// affordance: `sh:defaultValue` has zero triples tree-wide, so nothing
/// resembling a default may appear in the serialised schema at all
/// (contreforts/contreforts-workspace#60).
#[test]
fn schemas_serialise_deterministically_and_carry_no_prefill_affordance() {
    let first = serde_json::to_string(&ok(CALDAV)).expect("FormSchema serialises");
    let second = serde_json::to_string(&ok(CALDAV)).expect("FormSchema serialises");
    assert_eq!(first, second);

    // Non-vacuity: a serialiser that emitted "[]" would also be deterministic.
    for needle in ["authMode", "instanceUrl", "Authentication", "bearer"] {
        assert!(
            first.contains(needle),
            "the serialised schema should mention {needle}: {first}"
        );
    }

    assert!(
        !first.to_lowercase().contains("default"),
        "no default/prefill affordance may reach the wire: {first}"
    );
}

/// A graph carrying two connectors yields one properly scoped schema each, in
/// a deterministic order, with no field of one leaking into the other. This is
/// the shape W5 will call `form_schemas` in -- against `PRODUCT_GRAPH_TTL`,
/// the union of all seven -- and `find_connector_shapes` returns those shapes
/// in hash order, so an unsorted result would reorder between runs.
#[test]
fn a_two_connector_graph_yields_one_scoped_schema_each_in_a_deterministic_order() {
    let union = format!("{CONTROL_PRECEDENCE}\n{DATATYPE_CONTROLS}");
    let schemas = ok(&union);

    assert_eq!(
        schemas.iter().map(|s| s.kind.as_str()).collect::<Vec<_>>(),
        vec!["control-precedence-test", "datatype-controls-test"],
        "schemas are ordered by kind, not by the parser's hash order"
    );
    assert_eq!(field_ids(&schemas[0]), vec!["label", "secretPort", "mode"]);
    assert_eq!(
        field_ids(&schemas[1]).len(),
        10,
        "each schema carries only its own connector's fields"
    );
    assert!(
        !field_ids(&schemas[1]).contains(&"secretPort".to_string()),
        "one connector's fields must not leak into another's schema"
    );

    // The kind ordering above can pass by luck (the parser's hash order
    // agrees with it roughly one run in three, measured). This does not:
    // two calls in the same process already disagree unless the digest sorts.
    assert_eq!(
        serde_json::to_string(&ok(&union)).expect("serialises"),
        serde_json::to_string(&ok(&union)).expect("serialises"),
        "a two-connector digest must be stable call to call"
    );
}

// ========================================================================
// The entity-vocabulary boundary
// ========================================================================

/// No field id may EQUAL the local name of a class carrying
/// `contreforts:entityKind`. Exact match, deliberately: case-insensitively
/// the claim is already false -- `customer` is a real `sh:path` on caldav,
/// o365 and stalwart, while `Customer` is an `rdfs:Class` carrying
/// `contreforts:entityKind "customer"` in erpnext, forgejo, gitlab and
/// pennylane. Full IRIs never collide.
///
/// The discriminator is REACHABILITY FROM `sh:targetClass`, not a type test:
/// `find_connector_shapes` (src/model.rs) admits only shapes that are
/// `is_node_shape()` AND carry a `Target::Class`, and no `rdfs:Class` is ever
/// the object of `sh:property`. This test exists anyway, because
/// "structurally impossible" is what the previous twelve instances of absence
/// presenting as success also looked like.
///
/// The near-miss fixture is the only input where this is not vacuous: it puts
/// a `customer` field and a `Customer` entity class in one graph, as the
/// assembled product graph does and as no single declaration.ttl does. The
/// test asserts the entity set is NON-EMPTY before asserting the intersection
/// is empty -- five separate defects in this project took the shape of a
/// check that passed because it examined the wrong thing.
#[test]
fn no_field_id_equals_an_entity_class_local_name_although_one_matches_case_insensitively() {
    let entities = entity_class_local_names(ENTITY_NEAR_MISS);
    assert_eq!(
        entities,
        vec!["Customer", "Project"],
        "the fixture must actually carry entity classes, or this test proves nothing"
    );

    let ids = field_ids(&one(ENTITY_NEAR_MISS));
    assert_eq!(ids, vec!["label", "customer"]);

    for id in &ids {
        assert!(
            !entities.contains(id),
            "field id {id:?} collides exactly with an entity class -- the entity vocabulary is \
             being rendered as a form field"
        );
    }

    // The near miss is real, and is why the rule is EXACT match: a
    // case-insensitive rule would reject a legitimate field.
    assert!(
        ids.iter()
            .any(|id| entities.iter().any(|e| e.eq_ignore_ascii_case(id))),
        "the fixture is supposed to contain a case-insensitive near miss (customer/Customer)"
    );

    // Breadth: the same exact check over both vendored real declarations.
    // caldav declares Calendar and CalendarEvent, so its entity set is
    // non-empty too and this is not vacuous either.
    let caldav_entities = entity_class_local_names(CALDAV);
    assert_eq!(caldav_entities, vec!["Calendar", "Event"]);
    for id in field_ids(&one(CALDAV)) {
        assert!(
            !caldav_entities.contains(&id),
            "caldav field {id:?} collides"
        );
    }
}

// ========================================================================
// Totality: every hole, each a named error
// ========================================================================

/// An `sh:datatype` with no control mapping is a hard error naming the
/// connector, the `sh:path` local name and the FULL datatype IRI -- not a
/// fallback to `Text` (which renders a hex blob as a text box and reports
/// success) and not an omitted field.
#[test]
fn unmapped_datatype_is_an_error_naming_kind_path_and_the_full_datatype_iri() {
    let errors = err(UNMAPPED_DATATYPE);
    let message = message_of(&errors, "UnmappedDatatype");
    assert!(
        message.contains("unmapped-datatype-test"),
        "names the kind: {message}"
    );
    assert!(
        message.contains("fingerprint"),
        "names the sh:path local name: {message}"
    );
    assert!(
        message.contains("http://www.w3.org/2001/XMLSchema#hexBinary"),
        "names the full datatype IRI a maintainer has to go and look at: {message}"
    );
}

/// A property with neither `sh:datatype` nor `sh:in` has no derivable control
/// at all.
#[test]
fn a_property_with_no_datatype_and_no_sh_in_is_an_error_not_a_guess() {
    let errors = err(UNTYPED_PROPERTY);
    let message = message_of(&errors, "NoDatatypeAndNoIn");
    assert!(
        message.contains("untyped-property-test"),
        "names the kind: {message}"
    );
    assert!(
        message.contains("anything"),
        "names the sh:path local name: {message}"
    );
}

/// W1's `PropertyShape.unhandled` is the mechanism; this is the consumer.
/// `sh:maxLength` has no dedicated field, so it lands in `unhandled` as
/// `"MaxLength"` and must surface by that name.
#[test]
fn an_unhandled_constraint_is_an_error_naming_the_recorded_discriminant() {
    let errors = err(UNHANDLED_CONSTRAINT);
    let message = message_of(&errors, "UnhandledConstraint");
    assert!(
        message.contains("synthetic-unhandled-test"),
        "names the kind: {message}"
    );
    assert!(
        message.contains("label"),
        "names the sh:path local name: {message}"
    );
    assert!(
        message.contains("MaxLength"),
        "names the constraint W1 recorded, so a maintainer knows what to add: {message}"
    );
}

/// One input, TWO errors, from ONE property shape: the vendored forgejo
/// fixture's `groupMapping` carries both an `sh:node` and
/// `contreforts:uiShape "mapping-table"`. A fail-fast digest can only ever
/// report one of them, so this is the collection test.
///
/// (contreforts-core#34's own body calls this property `groupMappings`; the
/// fixture declares `forgejo:groupMapping`, singular. The fixture wins.)
#[test]
fn the_forgejo_fixture_reports_both_the_nested_shape_and_the_unknown_ui_shape() {
    let errors = err(FORGEJO);
    assert!(
        errors.len() >= 2,
        "expected at least two errors from one property carrying two unsupported constructs, \
         got {}: {}",
        errors.len(),
        display_all(&errors)
    );

    let nested = message_of(&errors, "NestedShape");
    assert!(nested.contains("forgejo"), "names the kind: {nested}");
    assert!(
        nested.contains("groupMapping"),
        "names the sh:path local name: {nested}"
    );
    assert!(nested.contains("sh:node"), "names the construct: {nested}");

    let ui = message_of(&errors, "UnknownUiShape");
    assert!(ui.contains("forgejo"), "names the kind: {ui}");
    assert!(
        ui.contains("mapping-table"),
        "names the unregistered value: {ui}"
    );
}

/// The node-level half of the uiShape hole. Exactly one value is registered
/// tree-wide, `"discriminated-union"`; anything else names a component the
/// thin renderer does not have and cannot discover is missing.
#[test]
fn an_unknown_node_level_ui_shape_is_an_error_naming_the_value() {
    let errors = err(UNKNOWN_NODE_UI_SHAPE);
    let message = message_of(&errors, "UnknownUiShape");
    assert!(
        message.contains("unknown-ui-shape-test"),
        "names the kind: {message}"
    );
    assert!(
        message.contains("multi-step-wizard"),
        "names the value: {message}"
    );
}

/// `sh:closed` and `sh:deactivated` are SEPARATE FIELDS on shacl-rust's
/// `Shape`, not members of `shape.constraints` -- so W1's recording arm is
/// structurally blind to both, and `declarations()` returns a perfectly
/// ordinary `Declaration` for either. Verified against shacl-rust 0.2.9.
#[test]
fn a_closed_or_deactivated_shape_is_an_error_although_neither_reaches_constraints() {
    for (ttl, kind, construct) in [
        (CLOSED_SHAPE, "closed-shape-test", "sh:closed"),
        (
            DEACTIVATED_SHAPE,
            "deactivated-shape-test",
            "sh:deactivated",
        ),
    ] {
        // Precondition: the model layer really does report these as fine.
        assert!(
            declarations(ttl).is_ok(),
            "{kind}: declarations() is expected to still accept this -- that is the whole hole"
        );

        let errors = err(ttl);
        let message = message_of(&errors, "ClosedOrDeactivatedShape");
        assert!(message.contains(kind), "names the kind: {message}");
        assert!(
            message.contains(construct),
            "names the construct: {message}"
        );
    }
}

/// `build_property_shape` opens with `let path = simple_path_iri(shape)?;`, so
/// a non-IRI `sh:path` makes the whole property vanish before any constraint
/// is examined. The digest must notice the COUNT, not merely fail to find
/// anything wrong with the fields that survived.
#[test]
fn an_inverse_path_is_a_dropped_property_error_not_a_schema_one_field_short() {
    // Precondition, so the test cannot pass for the wrong reason: the model
    // layer really does return a Declaration with the property missing.
    let declaration = declarations(INVERSE_PATH)
        .expect("still validates")
        .remove(0);
    assert_eq!(
        declaration.properties.len(),
        1,
        "the fixture declares two sh:property children and the model reports one -- if this \
         changed, simple_path_iri changed and this test needs rewriting"
    );

    let errors = err(INVERSE_PATH);
    let message = message_of(&errors, "DroppedProperty");
    assert!(
        message.contains("inverse-path-test"),
        "names the kind: {message}"
    );
    assert!(
        message.contains('2') && message.contains('1'),
        "names both counts, so a maintainer can see how many went missing: {message}"
    );
}

/// `build_variants` opens each alternative with
/// `let discriminant_value = discriminant_value(alternative)?;`, so an
/// alternative with no single-valued `sh:in` is dropped and the union renders
/// one branch short, reporting success.
#[test]
fn an_xone_alternative_without_a_discriminant_is_a_dropped_variant_error() {
    // Precondition: two alternatives in, one variant out.
    let declaration = declarations(XONE_WITHOUT_DISCRIMINANT)
        .expect("still validates")
        .remove(0);
    assert_eq!(
        declaration.variants.len(),
        1,
        "the fixture declares an sh:xone list of two and the model reports one variant"
    );

    let errors = err(XONE_WITHOUT_DISCRIMINANT);
    let message = message_of(&errors, "DroppedVariant");
    assert!(
        message.contains("xone-no-discriminant-test"),
        "names the kind: {message}"
    );
    assert!(
        message.contains('2') && message.contains('1'),
        "names both counts: {message}"
    );
}

/// `build_variants` `find_map`s `Constraint::Xone` and nothing else, so any
/// other node-level constraint reaches no code at all -- and W1's `unhandled`
/// does not see it either, because that recording arm runs per PROPERTY shape.
/// The seven real connector node shapes carry `Xone` (caldav, o365) or nothing
/// (the other five), so rejecting the rest costs nothing today.
#[test]
fn a_node_level_constraint_other_than_xone_is_an_error() {
    let errors = err(NODE_LEVEL_CONSTRAINT);
    let message = message_of(&errors, "UnhandledConstraint");
    assert!(
        message.contains("node-level-constraint-test"),
        "names the kind: {message}"
    );
    assert!(
        message.contains("Not"),
        "names the constraint that reached nothing: {message}"
    );
}

/// `kind` is the key every layer above joins on -- the route, the connector
/// picker, `contreforts-config`'s `connector_type()`. A `sh:targetClass` with
/// no `#` yields no kind, and a guessed one is a form wired to the wrong
/// connector.
#[test]
fn a_target_class_with_no_fragment_is_an_undecked_kind_error() {
    let errors = err(UNDECKED_KIND);
    let message = message_of(&errors, "UndeckedKind");
    assert!(
        message.contains(
            "https://contreforts.ds-labs.org/ontologies/undecked-kind-test/UndeckedKindConnector"
        ),
        "names the target class IRI, which is all the identity there is here: {message}"
    );
    assert!(
        message.contains("sh:targetClass"),
        "names the construct: {message}"
    );
}

/// A `sh:group` the group table cannot resolve to a label and an order, and a
/// property with no `sh:group` at all. All 45 real top-level properties carry
/// an `sh:group`, and all 17 real groups resolve to both an `rdfs:label` and
/// an `sh:order` (measured over the seven declarations), so neither case
/// costs a real connector anything -- and a section with no heading is a
/// field the user cannot find.
#[test]
fn an_unresolvable_group_is_an_error_whether_dangling_or_merely_unlabelled() {
    // Dangling: referenced by sh:group, never typed a sh:PropertyGroup.
    let dangling = err(DANGLING_GROUP);
    let message = message_of(&dangling, "UnresolvedGroup");
    assert!(
        message.contains("PhantomGroup"),
        "names the group: {message}"
    );
    assert!(
        message.contains("dangling-group-test"),
        "names the kind: {message}"
    );

    // Declared, but with no rdfs:label and no sh:order -- and two of the three
    // properties carry no sh:group at all.
    let minimal = err(SYNTHETIC_MINIMAL);
    let message = message_of(&minimal, "UnresolvedGroup");
    assert!(
        message.contains("synthetic-test"),
        "names the kind: {message}"
    );
    for id in ["pollIntervalSecs", "token"] {
        assert!(
            message.contains(id),
            "an ungrouped property must be named, not dropped from every section: {message}"
        );
    }
}

/// `declarations()` returns `Result<Vec<Declaration>, Violations>` -- the
/// meta-shapes and the D14/D2/D15/entityKind lints. `form_schemas` must carry
/// those through as a `DigestError`, not `unwrap` them into a panic: W5 calls
/// this from a `build.rs`, where a panic is a stack trace instead of a
/// legible list of what the connector author got wrong.
#[test]
fn a_lint_failure_is_carried_as_a_declaration_error_not_a_panic() {
    let errors = err(NAIVE_XONE);
    let message = message_of(&errors, "Declaration");
    let violations = declarations(NAIVE_XONE)
        .expect_err("the naive xone fixture is supposed to fail D14")
        .to_string();

    // Compared line by line rather than as one string: `Violations`'s own
    // Display orders the meta-shape violations non-deterministically (two
    // consecutive calls on this very fixture swapped violations 1 and 2), so
    // asserting `message.contains(&violations)` is a test that passes most of
    // the time -- which is worse than one that fails.
    assert!(
        message.contains("7 violation(s)"),
        "the violation count must survive: {message}"
    );
    for needle in [
        "D14 (sh:xone cross-exclusion)",
        "NaiveDelegatedShape",
        "must carry exactly one contreforts:category",
    ] {
        assert!(
            message.contains(needle),
            "the Violations Display must survive intact; missing {needle:?}.\n  digest: \
             {message}\n  violations: {violations}"
        );
    }
}

/// Malformed Turtle is the same story one layer down: a legible error, not a
/// panic surfacing out of oxigraph.
#[test]
fn malformed_turtle_is_a_declaration_error_not_a_panic() {
    let errors = err(MALFORMED);
    let message = message_of(&errors, "Declaration");
    assert!(
        message.to_lowercase().contains("turtle"),
        "should say what is wrong: {message}"
    );
}

/// The digest collects EVERY error and never fails fast. Asserted over a
/// union of three independently-broken connectors, so it cannot pass by
/// accident on one graph that happens to yield several errors from one place.
#[test]
fn errors_from_several_connectors_are_all_collected_never_fail_fast() {
    let union = format!("{UNMAPPED_DATATYPE}\n{UNTYPED_PROPERTY}\n{UNKNOWN_NODE_UI_SHAPE}");
    let errors = err(&union);
    let names = variant_names(&errors);
    for expected in ["NoDatatypeAndNoIn", "UnknownUiShape", "UnmappedDatatype"] {
        assert!(
            names.contains(&expected.to_string()),
            "expected a {expected} among the collected errors, got {names:?}: {}",
            display_all(&errors)
        );
    }
}

/// A well-formed connector next to a broken one must not be reported as fine.
/// `form_schemas` returns `Result<Vec<FormSchema>, Vec<DigestError>>`: there
/// is deliberately no "here are six schemas and one error" middle ground, so
/// W5's build cannot go green while a connector is unrenderable.
#[test]
fn one_broken_connector_fails_the_whole_digest_not_just_its_own_schema() {
    let union = format!("{CALDAV}\n{UNMAPPED_DATATYPE}");
    let errors = err(&union);
    assert!(
        errors.iter().any(|e| variant_name(e) == "UnmappedDatatype"),
        "{}",
        display_all(&errors)
    );
}

/// The other arm of `FormSchema.label`: a declaration whose node shape carries
/// no `rdfs:label` and no `sh:name` falls back to its kind, rather than
/// rendering a connector picker entry with an empty heading. All seven real
/// declarations carry one today (W3), so only a synthetic reaches this.
#[test]
fn a_declaration_with_no_node_label_falls_back_to_its_kind() {
    let schema = one(DATATYPE_CONTROLS);
    assert_eq!(schema.label, "datatype-controls-test");
    assert_eq!(schema.kind, "datatype-controls-test");
    assert_eq!(schema.description, None);
}

/// The node-level rule, proven against a REAL file rather than a synthetic:
/// the 2026-07 o365 spike fixture carries a node-level `sh:sparql`
/// (sibling-label uniqueness, `o365-declaration.ttl:327`) that
/// `build_variants` reaches with nothing at all -- it `find_map`s
/// `Constraint::Xone` and stops. The connector itself dropped that constraint
/// (`crates/contreforts-connector-o365/declaration.ttl:381`), which is why
/// `o365-declaration-current.ttl` exists and is what every happy-path o365
/// assertion above runs against.
///
/// This is a drift the phase-F plan did not know about: contreforts-core#34
/// asks for `form_schemas(include_str!("fixtures/o365-declaration.ttl"))` to
/// return `Ok`, AND for any non-`Xone` node-level constraint to be rejected,
/// naming `sh:sparql` among the examples. Both cannot hold for this file.
#[test]
fn the_o365_spike_fixture_is_rejected_for_its_node_level_sh_sparql() {
    let errors = err(O365_SPIKE);
    let message = message_of(&errors, "UnhandledConstraint");
    assert!(message.contains("o365"), "names the kind: {message}");
    assert!(
        message.contains("Sparql"),
        "names the constraint that reached no code: {message}"
    );
}
