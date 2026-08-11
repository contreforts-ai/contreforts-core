//! Integration tests for the connector-instance validation layer, relocated here from
//! `contreforts-kg::connector_validation` (contreforts/contreforts-workspace#58, comment 7791,
//! item D3a).
//!
//! These exercise `ConnectorValidator`, `ConnectorDeclarations`, `ConnectorIris` and
//! `ConnectorValidationOutcome` from their *new* home -- as `contreforts_declaration::...` --
//! proving the moved types keep every behaviour the pre-move `connector_validation.rs` had, not
//! merely that the names still exist. `GraphError::ConnectorValidation` stays behind in
//! `contreforts-kg` (D3a's scope) and is not exercised here.
//!
//! RED discipline (contreforts-workspace#58, `contreforts-kg/CONTRIBUTING.md` §3 -- a compile
//! error against a not-yet-existing symbol counts as the spec): this file does not compile yet,
//! because `contreforts_declaration::ConnectorValidator` (and friends) do not exist until the
//! move lands. That is the intended RED for this step.

use shacl_rust::rdf::read_graph_from_string;

use contreforts_declaration::{
    ConnectorDeclarations, ConnectorValidationOutcome, ConnectorValidator, WriteIntent,
};

/// Verbatim excerpt of `contreforts-connector-forgejo/declaration.ttl:185-224` -- the same real
/// declaration `contreforts-kg`'s own `tests/config_graph.rs` uses as `FORGEJO_DECLARATION_TTL`
/// (a real declaration, not a synthetic stand-in), so the move is proven against the shape
/// production actually uses.
const FORGEJO_DECLARATION_TTL: &str = r#"
    @prefix sh:      <http://www.w3.org/ns/shacl#> .
    @prefix xsd:     <http://www.w3.org/2001/XMLSchema#> .
    @prefix forgejo: <https://contreforts.ds-labs.org/ontologies/forgejo#> .

    forgejo:ForgejoConnectorShape a sh:NodeShape ;
        sh:targetClass forgejo:ForgejoConnector ;
        sh:property [
            sh:path forgejo:label ;
            sh:datatype xsd:string ;
            sh:minCount 1 ;
            sh:maxCount 1 ;
        ] ;
        sh:property [
            sh:path forgejo:instanceUrl ;
            sh:datatype xsd:string ;
            sh:minCount 1 ;
            sh:maxCount 1 ;
            sh:pattern "^https?://[^\\s]+$" ;
        ] ;
        sh:property [
            sh:path forgejo:token ;
            sh:datatype xsd:string ;
            sh:minCount 1 ;
            sh:maxCount 1 ;
        ] .
"#;

/// `(kind, type_name)` pairs, the shape `config_graph::all_connector_kinds()` hands
/// `ConnectorValidator::new` in production: one declared kind (forgejo) plus at least one
/// deliberately undeclared kind (erpnext), so case 1 and case 2 both have something to resolve
/// against in the same validator.
fn all_kinds() -> Vec<(&'static str, &'static str)> {
    vec![
        ("forgejo", "ForgejoConnector"),
        ("erpnext", "ErpNextConnector"),
    ]
}

fn forgejo_validator() -> ConnectorValidator {
    ConnectorValidator::new(FORGEJO_DECLARATION_TTL, &all_kinds())
        .expect("the embedded forgejo declaration is valid SHACL")
}

#[test]
fn connector_iris_case_1_declared_kind_resolves_class_and_fields() {
    // Catches: a declared kind's class/field IRIs failing to resolve after the move, or
    // resolving against the wrong namespace -- the module's own "case 1" policy.
    let validator = forgejo_validator();
    let iris = validator
        .connector_iris("forgejo")
        .expect("forgejo has a declaration in the handed-in shapes");

    assert_eq!(
        iris.class_iri,
        "https://contreforts.ds-labs.org/ontologies/forgejo#ForgejoConnector"
    );
    assert_eq!(
        iris.field_iris.get("token").map(String::as_str),
        Some("https://contreforts.ds-labs.org/ontologies/forgejo#token")
    );
    assert_eq!(iris.field_iris.len(), 3, "label, instanceUrl, token");
}

#[test]
fn connector_iris_case_2_undeclared_kind_resolves_to_none() {
    // Catches: an undeclared kind silently resolving to *some* IRIs (e.g. matching an unrelated
    // shape by accident) instead of the explicit "not declared" `None` case 2 requires.
    let validator = forgejo_validator();
    assert!(
        validator.connector_iris("erpnext").is_none(),
        "erpnext has no declaration in the handed-in shapes -- must be case 2, not case 1"
    );
}

#[test]
fn connector_declarations_none_means_no_kind_is_declared() {
    // Catches: `ConnectorDeclarations::none()` -- the explicit "no declarations in force" case
    // -- accidentally resolving some kind's IRIs anyway.
    let none = ConnectorDeclarations::none();
    assert!(none.connector_iris("forgejo").is_none());
    assert!(none.connector_iris("anything-at-all").is_none());
}

#[test]
fn validate_accepts_a_conforming_instance() {
    // Catches: a correctly-shaped write being rejected -- e.g. a regression in how the SHACL
    // validation dataset is built from the instance graph across the move.
    let validator = forgejo_validator();
    let instance_ttl = r#"
        @prefix forgejo: <https://contreforts.ds-labs.org/ontologies/forgejo#> .

        <https://contreforts.ds-labs.org/data/connector/forgejo/acme/main>
            a forgejo:ForgejoConnector ;
            forgejo:label "main" ;
            forgejo:instanceUrl "https://git.example.com" ;
            forgejo:token "tok-abc" .
    "#;
    let instance = read_graph_from_string(instance_ttl, "turtle").expect("valid turtle");

    let outcome = validator
        .validate(
            WriteIntent::Create,
            "https://contreforts.ds-labs.org/ontologies/forgejo#ForgejoConnector",
            &instance,
        )
        .expect("a conforming instance must validate");
    assert_eq!(outcome, ConnectorValidationOutcome::Conforms);
}

#[test]
fn validate_rejects_a_violating_instance_naming_the_property() {
    // Catches: a violating write being silently allowed through, or a rejection that loses
    // which field it is about -- either would defeat the point of wiring server-side
    // validation at all.
    let validator = forgejo_validator();
    // `forgejo:token` is entirely absent -- violates `sh:minCount 1` on that property shape.
    let instance_ttl = r#"
        @prefix forgejo: <https://contreforts.ds-labs.org/ontologies/forgejo#> .

        <https://contreforts.ds-labs.org/data/connector/forgejo/acme/main>
            a forgejo:ForgejoConnector ;
            forgejo:label "main" ;
            forgejo:instanceUrl "https://git.example.com" .
    "#;
    let instance = read_graph_from_string(instance_ttl, "turtle").expect("valid turtle");

    let violations = validator
        .validate(
            WriteIntent::Create,
            "https://contreforts.ds-labs.org/ontologies/forgejo#ForgejoConnector",
            &instance,
        )
        .expect_err("a write missing a required field must be rejected");

    assert!(
        violations
            .iter()
            .any(|v| v.field.as_deref() == Some("token")),
        "the rejection must name `token` as the violated property, got: {violations:?}"
    );
}

#[test]
fn an_unscoped_declaration_behaves_identically_under_both_intents() {
    // contreforts/contreforts-workspace#19, D8 amended: the ten connectors still carry a single
    // shape with no `contreforts:writeIntent`, and absence means "applies to both".
    //
    // Catches: reading the absent term as create-only or update-only. Either would be invisible
    // in the direction that still works and catastrophic in the other -- update-only would stop
    // enforcing forgejo's required token on creation, create-only would stop enforcing anything
    // at all on every update of every unmigrated connector. This is the regression test that
    // makes it safe to land the mechanism before migrating any declaration.
    let validator = forgejo_validator();
    let instance = read_graph_from_string(
        r#"
        @prefix forgejo: <https://contreforts.ds-labs.org/ontologies/forgejo#> .

        <https://contreforts.ds-labs.org/data/connector/forgejo/acme/main>
            a forgejo:ForgejoConnector ;
            forgejo:label "main" ;
            forgejo:instanceUrl "https://git.example.com" .
        "#,
        "turtle",
    )
    .expect("valid turtle");

    for intent in [WriteIntent::Create, WriteIntent::Update] {
        match validator.validate(
            intent,
            "https://contreforts.ds-labs.org/ontologies/forgejo#ForgejoConnector",
            &instance,
        ) {
            Ok(outcome) => panic!(
                "forgejo declares one unscoped shape requiring `token`, so a write without it \
                 must be rejected under {intent:?} too -- got {outcome:?}"
            ),
            Err(violations) => assert!(
                violations
                    .iter()
                    .any(|v| v.field.as_deref() == Some("token")),
                "under {intent:?} the rejection must name `token`, got: {violations:?}"
            ),
        }
    }
}

#[test]
fn connector_iris_captures_datatype_only_for_fields_that_declare_one() {
    // Ported verbatim from `contreforts-kg::connector_validation`'s own unit test
    // (contreforts-kg#25): proves the datatype/no-datatype distinction on `ConnectorIris`
    // survives the move byte-for-byte.
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
        "label has sh:path but no sh:datatype -- must have no entry, not a None value hidden \
         behind one"
    );
}

#[test]
fn unvalidated_kinds_and_write_counts_track_the_case_2_policy() {
    // Ported in spirit, not verbatim -- pre-move, this behaviour was only exercised implicitly
    // through `contreforts-kg`'s `config_graph` characterization tests, never directly against
    // `connector_validation.rs` itself. Follows the module's own doc comment on
    // `unvalidated_kinds`/`unvalidated_write_count`: erpnext has no declaration, so it must be
    // named at construction time (the static half) and counted at each write (the dynamic
    // half), never silently allowed with no trace.
    let validator = forgejo_validator();
    assert_eq!(validator.unvalidated_kinds().to_vec(), vec!["erpnext"]);
    assert_eq!(validator.unvalidated_write_count(), 0);

    let instance = read_graph_from_string(
        r#"
        @prefix core: <https://contreforts.ds-labs.org/ontologies/core#> .
        <https://contreforts.ds-labs.org/data/connector/erpnext/acme>
            a core:ErpNextConnector ;
            core:companyName "Acme Co" .
        "#,
        "turtle",
    )
    .expect("valid turtle");

    let outcome = validator
        .validate(
            WriteIntent::Create,
            "https://contreforts.ds-labs.org/ontologies/core#ErpNextConnector",
            &instance,
        )
        .expect("an undeclared kind's write is allowed under the case-2 policy");
    assert_eq!(outcome, ConnectorValidationOutcome::NotDeclared);
    assert_eq!(
        validator.unvalidated_write_count(),
        1,
        "the case-2 write must be counted, not silently passed through unlogged"
    );
}

// ---------------------------------------------------------------------
// D8's create-versus-update asymmetry
// (contreforts/contreforts-workspace#19, "D8 amended -- 2026-08-11")
// ---------------------------------------------------------------------

/// SYNTHETIC. The D8 arrangement, minimal: two node shapes over one `sh:targetClass`, differing
/// in exactly one triple -- `sh:minCount 1` on the secret, present on the create shape and
/// absent from the update shape. Everything else is identical on purpose, so a test that passes
/// for the wrong reason (because the two shapes differ somewhere else as well) is not possible.
///
/// `sh:pattern` on the secret is on **both** shapes deliberately: it is what makes
/// `an_update_still_validates_the_secret_it_was_given` able to fail. "Absent means unchanged" is
/// one rule, not a licence to stop checking updates, and the cheapest wrong implementation of
/// this whole feature -- skip validation entirely when the intent is `Update` -- passes every
/// other test in this section.
const WIDGET_D8_TTL: &str = r#"
    @prefix sh:          <http://www.w3.org/ns/shacl#> .
    @prefix xsd:         <http://www.w3.org/2001/XMLSchema#> .
    @prefix contreforts: <https://contreforts.ds-labs.org/ontologies/declaration#> .
    @prefix widget:      <https://contreforts.ds-labs.org/ontologies/widget#> .

    widget:WidgetCreateShape a sh:NodeShape ;
        sh:targetClass widget:Widget ;
        contreforts:writeIntent "create" ;
        sh:property [
            sh:path widget:label ;
            sh:datatype xsd:string ;
            sh:minCount 1 ;
            sh:maxCount 1 ;
        ] ;
        sh:property [
            sh:path widget:apiToken ;
            sh:datatype xsd:string ;
            sh:minCount 1 ;
            sh:maxCount 1 ;
            sh:pattern "^tok-" ;
            contreforts:secret true ;
        ] .

    widget:WidgetUpdateShape a sh:NodeShape ;
        sh:targetClass widget:Widget ;
        contreforts:writeIntent "update" ;
        sh:property [
            sh:path widget:label ;
            sh:datatype xsd:string ;
            sh:minCount 1 ;
            sh:maxCount 1 ;
        ] ;
        sh:property [
            sh:path widget:apiToken ;
            sh:datatype xsd:string ;
            sh:maxCount 1 ;
            sh:pattern "^tok-" ;
            contreforts:secret true ;
        ] .
"#;

const WIDGET_CLASS: &str = "https://contreforts.ds-labs.org/ontologies/widget#Widget";

fn widget_validator() -> ConnectorValidator {
    ConnectorValidator::new(WIDGET_D8_TTL, &[("widget", "Widget")])
        .expect("the D8 pair is valid SHACL and covers both intents")
}

/// A widget instance carrying `widget:apiToken` only when `token` is `Some` -- the two states
/// the asymmetry is about, built by one function so the "absent" and "present" cases cannot
/// drift apart in any other field.
fn widget_instance(token: Option<&str>) -> oxigraph::model::Graph {
    let token_triple = match token {
        Some(value) => format!("widget:apiToken \"{value}\" ;"),
        None => String::new(),
    };
    let ttl = format!(
        r#"
        @prefix widget: <https://contreforts.ds-labs.org/ontologies/widget#> .

        <https://contreforts.ds-labs.org/data/connector/widget/acme/main>
            a widget:Widget ;
            {token_triple}
            widget:label "main" .
        "#
    );
    read_graph_from_string(&ttl, "turtle").expect("valid turtle")
}

#[test]
fn an_absent_secret_is_refused_on_create() {
    // The first half of the asymmetry. Catches: `validate` running the update shapes (or all
    // shapes minus the create-scoped ones) for `WriteIntent::Create` -- i.e. a selection with
    // its two arms transposed, or a `shapes_for` that ignores its argument and returns
    // `update_shapes`. Without this, a connector could be created with no credential at all.
    let violations = widget_validator()
        .validate(WriteIntent::Create, WIDGET_CLASS, &widget_instance(None))
        .expect_err("creating a widget without its required secret must be refused");

    assert!(
        violations
            .iter()
            .any(|v| v.field.as_deref() == Some("apiToken")),
        "the refusal must name `apiToken`, got: {violations:?}"
    );
}

#[test]
fn an_absent_secret_is_accepted_on_update() {
    // The second half, and the one the whole two-shape design exists for: D8 makes the secret
    // write-only, so a form that submits nothing for it means "unchanged", not "clear".
    //
    // Catches: `validate` running every parsed shape regardless of intent (the pre-D8
    // behaviour, and the state this file would be in if `shapes_for` were deleted) -- the
    // create shape's `sh:minCount 1` would then fire on an update and make it impossible to
    // change a widget's label without re-typing its token.
    let outcome = widget_validator()
        .validate(WriteIntent::Update, WIDGET_CLASS, &widget_instance(None))
        .expect("an absent secret means unchanged on update, not a violation");

    assert_eq!(outcome, ConnectorValidationOutcome::Conforms);
}

#[test]
fn a_present_secret_is_accepted_under_both_intents() {
    // The direction people forget. Catches an implementation that achieves the asymmetry by
    // making the two shapes mutually exclusive rather than differently strict -- e.g. an update
    // shape written with `sh:maxCount 0` on the secret, which would satisfy both tests above
    // and then reject every legitimate credential rotation.
    let validator = widget_validator();
    for intent in [WriteIntent::Create, WriteIntent::Update] {
        let outcome = validator
            .validate(intent, WIDGET_CLASS, &widget_instance(Some("tok-abc")))
            .unwrap_or_else(|violations| {
                panic!("a valid secret must be accepted under {intent:?}, got: {violations:?}")
            });
        assert_eq!(outcome, ConnectorValidationOutcome::Conforms, "{intent:?}");
    }
}

#[test]
fn an_update_still_validates_the_secret_it_was_given() {
    // "Absent means unchanged" is not "updates are unvalidated". Catches the cheapest wrong
    // implementation of this feature -- short-circuit `validate` to `Ok(Conforms)` whenever the
    // intent is `Update` -- which every other test in this section passes.
    let violations = widget_validator()
        .validate(
            WriteIntent::Update,
            WIDGET_CLASS,
            &widget_instance(Some("not-a-token")),
        )
        .expect_err("a malformed secret is still a violation on update");

    assert!(
        violations
            .iter()
            .any(|v| v.field.as_deref() == Some("apiToken")),
        "the refusal must name `apiToken`, got: {violations:?}"
    );
}

#[test]
fn a_declaration_covering_only_one_intent_is_refused_at_construction() {
    // Catches: `ConnectorValidator::new` accepting a half-written pair. A class with a create
    // shape and no update shape does not reject updates -- SHACL resolves zero focus nodes for
    // the uncovered intent and reports conformance -- so every update would be accepted with
    // nothing checked, reported to the caller as `Ok(Conforms)`. Refusing to build the
    // validator is the only point at which that is detectable.
    let create_only = WIDGET_D8_TTL
        .split("widget:WidgetUpdateShape")
        .next()
        .expect("split always yields a first part");

    let error = ConnectorValidator::new(create_only, &[("widget", "Widget")])
        .err()
        .expect("a create shape with no update shape must not build a validator");

    let message = error.to_string();
    assert!(
        message.contains("update"),
        "the error must name the uncovered intent, got: {message}"
    );
    assert!(
        message.contains("Widget"),
        "the error must name the class it is about, got: {message}"
    );
}
