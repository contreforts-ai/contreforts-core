//! Integration tests for contreforts-declaration
//! (contreforts/contreforts-core#14, Part 5).
//!
//! Aimed at tests a wrong implementation fails, not tests that restate
//! constants. The single most important test here is
//! `naive_xone_declaration_is_rejected_naming_the_missing_predicates` --
//! without it, D14 could be written to agree with itself.

use contreforts_declaration::{Rule, validate};

const O365_DECLARATION: &str = include_str!("fixtures/o365-declaration.ttl");
const O365_NAIVE_XONE: &str = include_str!("fixtures/o365-shape-naive-xone.ttl");
const FORGEJO_DECLARATION: &str = include_str!("fixtures/forgejo-declaration.ttl");
const SYNTHETIC_MINIMAL: &str = include_str!("fixtures/synthetic-minimal-declaration.ttl");
const MALFORMED: &str = include_str!("fixtures/malformed.ttl");
const CORE_NS_VIOLATION: &str = include_str!("fixtures/core-ns-violation.ttl");
const CONFIG_FIELD_CONFLICT: &str = include_str!("fixtures/config-field-conflict.ttl");
const CONFIG_FIELD_SECRET_WITHOUT_FIELD: &str =
    include_str!("fixtures/config-field-secret-without-field.ttl");
const CONFIG_FIELD_VALID: &str = include_str!("fixtures/config-field-valid.ttl");

#[test]
fn hardened_o365_declaration_validates_clean() {
    let result = validate(O365_DECLARATION);
    assert!(
        result.is_ok(),
        "the hardened O365 declaration must pass meta-shapes and D14, got: {}",
        result.err().map(|v| v.to_string()).unwrap_or_default()
    );
    let declaration = result.unwrap();
    assert_eq!(
        declaration.target_class,
        "https://contreforts.ds-labs.org/ontologies/o365#O365Connector"
    );
    assert_eq!(declaration.category.as_deref(), Some("calendar"));
    assert_eq!(
        declaration.ui_shape.as_deref(),
        Some("discriminated-union"),
        "O365's non-trivial structure is at the node level (the sh:xone \
         union), so contreforts:uiShape belongs on the node shape itself"
    );
    // The connector's flat, top-level fields -- NOT the sh:xone
    // alternatives' internal restatements.
    let paths: Vec<&str> = declaration
        .properties
        .iter()
        .map(|p| p.path.as_str())
        .collect();
    assert_eq!(
        paths.len(),
        9,
        "label, userPrincipal, customer, authMode, tenantId, clientId, \
         clientSecret, token, refreshToken -- got {paths:?}"
    );
    assert!(paths.contains(&"https://contreforts.ds-labs.org/ontologies/o365#clientSecret"));

    let client_secret = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("clientSecret"))
        .expect("clientSecret property present");
    assert!(
        client_secret.secret,
        "clientSecret must carry contreforts:secret true"
    );
    assert_eq!(client_secret.name.as_deref(), Some("Client secret"));

    let label = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("/o365#label"))
        .expect("label property present");
    assert!(!label.secret);
    assert_eq!(label.min_count, Some(1));
    assert_eq!(label.max_count, Some(1));
}

#[test]
fn naive_xone_declaration_is_rejected_naming_the_missing_predicates() {
    // The single most important test in this crate: without the
    // sh:maxCount 0 cross-exclusions, sh:xone alone silently accepts a
    // mixed-variant instance (contreforts-workspace#27's headline
    // finding). D14 must reject the DECLARATION itself -- at build time,
    // before any instance is ever validated against it.
    let result = validate(O365_NAIVE_XONE);
    assert!(
        result.is_err(),
        "the naive sh:xone declaration (no cross-exclusions) must be rejected by D14"
    );
    let violations = result.unwrap_err();

    let d14: Vec<_> = violations
        .iter()
        .filter(|v| v.rule() == Rule::D14Xone)
        .collect();
    assert_eq!(
        d14.len(),
        5,
        "expected exactly 5 missing cross-exclusions (3 for NaiveDelegatedShape: \
         tenantId/clientId/clientSecret, 2 for NaiveClientCredentialsShape: \
         token/refreshToken), got: {violations}"
    );

    let rendered = violations.to_string();
    for predicate in [
        "o365#tenantId",
        "o365#clientId",
        "o365#clientSecret",
        "o365#token",
        "o365#refreshToken",
    ] {
        assert!(
            rendered.contains(predicate),
            "rejection message must name the missing predicate {predicate}; got:\n{rendered}"
        );
    }
    // Names the specific alternative missing the exclusion, not just the
    // top-level node shape -- a connector author must be able to fix it
    // without reading the issue.
    assert!(
        rendered.contains("NaiveClientCredentialsShape")
            || rendered.contains("NaiveDelegatedShape")
    );

    println!("D14 rejection message for the naive sh:xone fixture:\n{rendered}");
}

#[test]
fn forgejo_declaration_validates_clean() {
    let result = validate(FORGEJO_DECLARATION);
    assert!(
        result.is_ok(),
        "the Forgejo declaration must pass meta-shapes and D14, got: {}",
        result.err().map(|v| v.to_string()).unwrap_or_default()
    );
    let declaration = result.unwrap();
    assert_eq!(
        declaration.target_class,
        "https://contreforts.ds-labs.org/ontologies/forgejo#ForgejoConnector"
    );
    assert_eq!(declaration.category.as_deref(), Some("git-forge"));
    assert_eq!(
        declaration.ui_shape, None,
        "Forgejo's node shape carries no contreforts:uiShape -- only its \
         groupMapping property shape does"
    );

    let paths: Vec<&str> = declaration
        .properties
        .iter()
        .map(|p| p.path.as_str())
        .collect();
    assert_eq!(
        paths.len(),
        4,
        "label, instanceUrl, token, groupMapping -- got {paths:?}"
    );

    let token = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("/forgejo#token"))
        .expect("token property present");
    assert!(token.secret);

    let group_mapping = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("groupMapping"))
        .expect("groupMapping property present");
    assert_eq!(group_mapping.ui_shape.as_deref(), Some("mapping-table"));
    assert!(!group_mapping.secret);
}

#[test]
fn presentation_metadata_is_read_via_sparql_not_the_typed_shape_api() {
    // contreforts-workspace#21: sh:group, sh:order and sh:defaultValue are
    // declared by SHACL but not surfaced by shacl-rust's typed Shape.
    // Neither real declaration exercises sh:defaultValue at all, so this
    // is checked against a small synthetic declaration, same as the O365
    // spike did.
    let declaration =
        validate(SYNTHETIC_MINIMAL).expect("synthetic-minimal-declaration.ttl must validate clean");

    let label = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("/synthetic-test#label"))
        .expect("label property present");
    assert_eq!(
        label.group.as_deref(),
        Some("https://contreforts.ds-labs.org/ontologies/synthetic-test#ConnectionGroup")
    );
    assert_eq!(label.order, Some(1));
    assert_eq!(label.default_value, None);

    let poll_interval = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("pollIntervalSecs"))
        .expect("pollIntervalSecs property present");
    assert_eq!(poll_interval.order, Some(2));
    assert_eq!(poll_interval.group, None);
    assert_eq!(
        poll_interval.default_value.as_deref(),
        Some("\"300\"^^<http://www.w3.org/2001/XMLSchema#integer>"),
        "sh:defaultValue must be read back from the shapes graph by SPARQL, \
         per contreforts-workspace#21"
    );

    let token = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("/synthetic-test#token"))
        .expect("token property present");
    assert!(token.secret);
}

#[test]
fn core_namespace_subject_that_is_not_an_alignment_stub_is_rejected() {
    let result = validate(CORE_NS_VIOLATION);
    assert!(
        result.is_err(),
        "a core:-namespaced subject that is not a skos:Concept/ConceptScheme must be rejected (D2)"
    );
    let violations = result.unwrap_err();
    let d2: Vec<_> = violations
        .iter()
        .filter(|v| v.rule() == Rule::D2CoreNamespace)
        .collect();
    assert_eq!(
        d2.len(),
        1,
        "expected exactly one D2 violation, got: {violations}"
    );
    assert!(d2[0].subject().unwrap().contains("NotAnAlignmentStub"));
}

#[test]
fn real_declarations_alignment_stub_carve_out_is_not_rejected() {
    // The flip side of the test above: both real declarations reproduce a
    // minimal core: SKOS stub (core:Customer, core:Calendar, ...) as
    // alignment targets, and D2 must NOT reject those. Already implied by
    // `hardened_o365_declaration_validates_clean` and
    // `forgejo_declaration_validates_clean` passing, but asserted directly
    // here so a future change to D2 that breaks the carve-out fails a test
    // whose name says exactly what broke.
    for (label, ttl) in [("o365", O365_DECLARATION), ("forgejo", FORGEJO_DECLARATION)] {
        let result = validate(ttl);
        assert!(
            result.is_ok(),
            "{label}'s core: SKOS alignment stub must not be rejected by D2, got: {}",
            result.err().map(|v| v.to_string()).unwrap_or_default()
        );
    }
}

#[test]
fn real_declarations_carry_no_config_field_yet_and_still_validate() {
    // D15 (contreforts-core#16), Part 5's first requirement: neither real
    // declaration uses contreforts:configField yet -- absence must be
    // legal. Already implied by the two `..._validates_clean` tests above
    // passing at all once the D15 lints are wired into `validate()`, but
    // asserted directly here (mirroring
    // `real_declarations_alignment_stub_carve_out_is_not_rejected`'s own
    // reasoning for D2) so a future change to lint::config_field that
    // breaks this fails a test whose name says exactly what broke.
    for (label, ttl) in [("o365", O365_DECLARATION), ("forgejo", FORGEJO_DECLARATION)] {
        let declaration = validate(ttl).unwrap_or_else(|v| {
            panic!("{label}'s fixture must still validate clean under the D15 lints: {v}")
        });
        assert!(
            declaration
                .properties
                .iter()
                .all(|p| p.config_field.is_none()),
            "{label}'s fixture does not use contreforts:configField yet"
        );
    }
}

#[test]
fn duplicate_config_field_on_same_node_shape_is_rejected_naming_both() {
    let result = validate(CONFIG_FIELD_CONFLICT);
    assert!(
        result.is_err(),
        "two property shapes naming the same contreforts:configField on one node shape must be rejected"
    );
    let violations = result.unwrap_err();
    let d15: Vec<_> = violations
        .iter()
        .filter(|v| v.rule() == Rule::D15ConfigField)
        .collect();
    assert_eq!(
        d15.len(),
        1,
        "expected exactly one duplicate-configField violation, got: {violations}"
    );

    let rendered = violations.to_string();
    for needle in [
        "API URL",
        "Mirror URL",
        "\"url\"",
        "ConfigFieldConflictShape",
    ] {
        assert!(
            rendered.contains(needle),
            "rejection message must name the node shape and both offending properties; \
             missing {needle:?} in:\n{rendered}"
        );
    }

    println!("duplicate-configField rejection message:\n{rendered}");
}

#[test]
fn secret_without_config_field_is_rejected_once_the_node_shape_opts_in() {
    let result = validate(CONFIG_FIELD_SECRET_WITHOUT_FIELD);
    assert!(
        result.is_err(),
        "contreforts:secret true without contreforts:configField must be rejected once the \
         node shape already uses contreforts:configField elsewhere"
    );
    let violations = result.unwrap_err();
    let d15: Vec<_> = violations
        .iter()
        .filter(|v| v.rule() == Rule::D15ConfigField)
        .collect();
    assert_eq!(
        d15.len(),
        1,
        "expected exactly one secret-without-configField violation, got: {violations}"
    );

    let rendered = violations.to_string();
    for needle in ["API token", "ConfigFieldSecretShape"] {
        assert!(
            rendered.contains(needle),
            "rejection message must name the node shape and the offending secret property; \
             missing {needle:?} in:\n{rendered}"
        );
    }

    println!("secret-without-configField rejection message:\n{rendered}");
}

#[test]
fn valid_config_field_annotations_validate_and_are_readable_from_the_declaration_model() {
    let declaration =
        validate(CONFIG_FIELD_VALID).expect("config-field-valid.ttl must validate clean");

    let label = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("/config-field-test#label"))
        .expect("label property present");
    assert_eq!(
        label.config_field, None,
        "label carries no contreforts:configField, matching vocabulary.ttl's own example \
         (identity, not configuration)"
    );

    let api_url = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("apiUrl"))
        .expect("apiUrl property present");
    assert_eq!(api_url.config_field.as_deref(), Some("url"));

    let api_token = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("apiToken"))
        .expect("apiToken property present");
    assert_eq!(api_token.config_field.as_deref(), Some("token"));
    assert!(
        api_token.secret,
        "apiToken must carry contreforts:secret true"
    );
}

#[test]
fn malformed_turtle_is_a_legible_error_not_a_panic() {
    let result = validate(MALFORMED);
    assert!(
        result.is_err(),
        "malformed Turtle must be rejected, not accepted"
    );
    let violations = result.unwrap_err();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations.iter().next().unwrap().rule(), Rule::Turtle);
    let message = violations.to_string();
    assert!(
        message.to_lowercase().contains("turtle") || message.to_lowercase().contains("parse"),
        "error message should say what went wrong, got: {message}"
    );
}

#[test]
fn violations_display_is_panic_ready() {
    let result = validate(O365_NAIVE_XONE);
    let violations = result.unwrap_err();
    let rendered = violations.to_string();
    assert!(rendered.contains("violation"));
    assert!(
        rendered.lines().count() > 1,
        "should render one line per violation plus a header"
    );
}
