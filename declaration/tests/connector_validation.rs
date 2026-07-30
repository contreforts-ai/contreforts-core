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
    ConnectorDeclarations, ConnectorValidationOutcome, ConnectorValidator,
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
