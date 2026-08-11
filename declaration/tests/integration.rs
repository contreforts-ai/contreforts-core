//! Integration tests for contreforts-declaration
//! (contreforts/contreforts-core#14, Part 5).
//!
//! Aimed at tests a wrong implementation fails, not tests that restate
//! constants. The single most important test here is
//! `naive_xone_declaration_is_rejected_naming_the_missing_predicates` --
//! without it, D14 could be written to agree with itself.

use contreforts_declaration::{
    CONCEPTS_TTL, GroupDescriptor, IntentScope, Rule, declarations, validate,
};

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
const ENTITY_KIND_CONFLICT: &str = include_str!("fixtures/entity-kind-conflict.ttl");
const ENTITY_KIND_TWO_CONNECTORS_SAME_VALUE: &str =
    include_str!("fixtures/entity-kind-two-connectors-same-value.ttl");
const CORE_NS_DEFINES_CONCEPT: &str = include_str!("fixtures/core-ns-defines-concept.ttl");
const SYNTHETIC_UNHANDLED_CONSTRAINT: &str =
    include_str!("fixtures/synthetic-unhandled-constraint.ttl");
// W2 (contreforts/contreforts-core#33): the sh:PropertyGroup table and the node shape's own
// label/description.
const DANGLING_GROUP_REFERENCE: &str = include_str!("fixtures/dangling-group-reference.ttl");
const NODE_SHAPE_NAME_VS_LABEL: &str = include_str!("fixtures/node-shape-name-vs-label.ttl");
const GROUP_ORDERING_NONE_LAST: &str = include_str!("fixtures/group-ordering-none-last.ttl");
// contreforts-core#35: a group referenced only inside an sh:xone alternative's own property,
// never in the flat properties list.
const SYNTHETIC_VARIANT_ONLY_GROUP: &str =
    include_str!("fixtures/synthetic-variant-only-group.ttl");
// contreforts-core#31: a malformed (negative) sh:minLength, distinct from an absent one.
const SYNTHETIC_NEGATIVE_MIN_LENGTH: &str =
    include_str!("fixtures/synthetic-negative-min-length.ttl");
// contreforts-workspace#19, D8 amended: the create/update shape pair, the half-written pair,
// and the term put somewhere the validator never reads it.
const WRITE_INTENT_PAIR: &str = include_str!("fixtures/write-intent-pair.ttl");
const WRITE_INTENT_CREATE_ONLY: &str = include_str!("fixtures/write-intent-create-only.ttl");
const WRITE_INTENT_ON_PROPERTY_SHAPE: &str =
    include_str!("fixtures/write-intent-on-property-shape.ttl");

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
fn forgejo_label_min_length_and_node_kind_are_captured_not_dropped() {
    // contreforts/contreforts-workspace#60, phase F item W1: sh:minLength
    // and sh:nodeKind used to fall through PropertyShape's constraint
    // match into the bare `_ => {}` arm (model.rs, around what was line
    // 350 before W1) and vanish, exactly like sh:in/sh:pattern/
    // sh:minInclusive/sh:maxInclusive did before contreforts-config-api#27
    // item 1 gave those four their own fields.
    //
    // Pinned against the real Forgejo declaration's own `label` property
    // (crates/contreforts-connector-forgejo/declaration.ttl:185-194, in
    // the tree at ad35b4a), mirrored verbatim in this fixture at
    // forgejo-declaration.ttl:176-186: `sh:minLength 1` and
    // `sh:nodeKind sh:Literal`.
    let declaration =
        validate(FORGEJO_DECLARATION).expect("forgejo-declaration.ttl must validate clean");

    let label = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("/forgejo#label"))
        .expect("label property present");
    assert_eq!(
        label.min_length,
        Some(1),
        "forgejo:label's real sh:minLength 1 must be captured, not dropped"
    );
    assert_eq!(
        label.node_kind.as_deref(),
        Some("http://www.w3.org/ns/shacl#Literal"),
        "forgejo:label's real sh:nodeKind sh:Literal must be captured as the full SHACL \
         vocabulary IRI, not dropped"
    );
}

#[test]
fn unhandled_constraint_is_recorded_by_name_not_silently_dropped() {
    // contreforts/contreforts-workspace#60, phase F item W1's own point,
    // not incidental to it: the bare `_ => {}` arm that used to swallow
    // every constraint kind this crate has not grown a field for is
    // replaced by one that RECORDS the dropped constraint's discriminant
    // into `PropertyShape.unhandled`. Without this, an unrecognised SHACL
    // construct is a field that silently fails to render -- the exact
    // "absence presenting as success" defect class this epic exists to
    // close, and the one W4's later *total* digest depends on being able
    // to name.
    //
    // sh:maxLength is the proof constraint: it is a real `Constraint`
    // variant shacl-rust 0.2.9 exposes (`Constraint::MaxLength`), but this
    // crate only grew a field for its sibling sh:minLength in W1, not
    // sh:maxLength -- so it must still land in `unhandled`, named, rather
    // than vanish.
    let declaration = validate(SYNTHETIC_UNHANDLED_CONSTRAINT)
        .expect("synthetic-unhandled-constraint.ttl must validate clean");

    let label = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("/synthetic-unhandled-test#label"))
        .expect("label property present");
    assert!(
        label
            .unhandled
            .iter()
            .any(|name| name.contains("MaxLength")),
        "sh:maxLength must be named in PropertyShape.unhandled, got: {:?}",
        label.unhandled
    );

    // Non-vacuous-scope proof: a property with no unrecognised constraint
    // at all must NOT accumulate anything in `unhandled` -- the arm
    // records what it actually drops, it does not mark every property as
    // unhandled regardless of content.
    let token = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("/synthetic-unhandled-test#token"))
        .expect("token property present");
    assert!(
        token.unhandled.is_empty(),
        "token carries no unrecognised constraint; unhandled must stay empty, got: {:?}",
        token.unhandled
    );
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
fn core_namespace_subject_that_defines_a_skos_concept_is_rejected_once_the_carve_out_tightens() {
    // contreforts/contreforts-workspace#83 (E2), item 3: now that core ships its own canonical
    // scheme (concepts.ttl), the alignment-stub carve-out above must close. A `core:` subject
    // asserting `rdf:type skos:Concept` (exactly what `real_declarations_alignment_stub_carve_out
    // _is_not_rejected` above still proves is accepted, pre-tightening) must become a D2
    // violation once a connector is the one minting it, not core.
    //
    // This is the mutation-target test for the tightening itself: today, before the tightening
    // lands, core-ns-defines-concept.ttl's `core:test-defined-concept a skos:Concept` triple is
    // still waved through by the untightened carve-out, so `result.is_err()` below is FALSE and
    // this test fails -- a genuine (non-compile-error) RED, proven by running it against
    // develop's current lint::core_ns::check.
    let result = validate(CORE_NS_DEFINES_CONCEPT);
    assert!(
        result.is_err(),
        "a core:-namespaced subject asserting rdf:type skos:Concept must be rejected once the \
         D2 carve-out tightens to 'reference, never define' -- got Ok, meaning the carve-out \
         still accepts a connector-defined core: concept"
    );
    let violations = result.unwrap_err();
    let d2: Vec<_> = violations
        .iter()
        .filter(|v| v.rule() == Rule::D2CoreNamespace)
        .collect();
    assert_eq!(
        d2.len(),
        1,
        "expected exactly one D2 violation for the connector-defined core: concept, got: {violations}"
    );
    assert!(
        d2[0].subject().unwrap().contains("test-defined-concept"),
        "the violation must name the offending subject IRI; got subject {:?}",
        d2[0].subject()
    );
}

#[test]
fn every_subject_concepts_ttl_itself_defines_is_permitted_by_d2_without_hardcoding_the_scheme() {
    // contreforts/contreforts-core#26 (corrects contreforts/contreforts-workspace#83's E2 over-
    // broad carve-out removal, which blocks contreforts/contreforts-config-api#35): the D2 lint
    // must permit exactly the core: subjects `CONCEPTS_TTL` itself defines, not zero of them --
    // #35 unions `CONCEPTS_TTL` into contreforts-product's assembled `PRODUCT_GRAPH_TTL`, which
    // makes those subjects legitimately appear in a graph D2 inspects, and today's D2 (any
    // core:-namespaced subject at all, no exception) rejects every one of them.
    //
    // This test never names an individual concept or hardcodes a count: it parses
    // `contreforts_declaration::CONCEPTS_TTL` itself, AT TEST TIME, so a 16th concept added to
    // concepts.ttl tomorrow is covered by this exact, unmodified test the next time it runs --
    // proving the permitted set tracks the scheme rather than a frozen copy of it, the same
    // coupling `contreforts-kg/tests/core_concepts_coupling.rs` already guards on the Rust side.
    // Confirmed RED against today's (pre-#26) lint::core_ns::check before this fix: 16 D2
    // violations, one per core:-namespaced subject concepts.ttl defines (core:CoreConcepts plus
    // its 15 concepts) -- not a compile error, a genuine behavioural failure.
    let concepts_graph = shacl_rust::rdf::read_graph_from_string(CONCEPTS_TTL, "turtle")
        .expect("contreforts-declaration's own concepts.ttl must parse as Turtle");
    let concept_subject_count = concepts_graph
        .iter()
        .filter_map(|t| match t.subject {
            oxigraph::model::NamedOrBlankNodeRef::NamedNode(n) => Some(n.as_str().to_string()),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    assert!(
        concept_subject_count > 0,
        "concepts.ttl must define at least one subject, or this test would vacuously pass"
    );

    // A synthetic declaration that reproduces CONCEPTS_TTL's own triples verbatim, alongside an
    // otherwise-valid minimal shape (the structural lint requires at least one sh:NodeShape
    // carrying sh:targetClass) -- exactly the "a connector restates core's scheme verbatim"
    // case the issue's own text says is accepted and deliberately not defended against, since a
    // graph-scoped lint cannot distinguish who authored a given triple.
    let declaration_with_concepts_scheme = format!("{SYNTHETIC_MINIMAL}\n{CONCEPTS_TTL}");
    let result = validate(&declaration_with_concepts_scheme);
    let d2_violations: Vec<&contreforts_declaration::Violation> = match &result {
        Ok(_) => Vec::new(),
        Err(violations) => violations
            .iter()
            .filter(|v| v.rule() == Rule::D2CoreNamespace)
            .collect(),
    };
    assert!(
        d2_violations.is_empty(),
        "every one of concepts.ttl's own {concept_subject_count} subject(s) must be permitted \
         by D2 -- got D2 violation(s): {d2_violations:?}"
    );
    assert!(
        result.is_ok(),
        "a declaration reproducing concepts.ttl's own triples verbatim, alongside an otherwise-\
         valid minimal shape, must validate cleanly once D2 permits core's own scheme; got: {}",
        result.err().map(|v| v.to_string()).unwrap_or_default()
    );
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

// contreforts/contreforts-kg#30 ("What to build" item 1): the
// contreforts:entityKind term and its lint. Mechanically identical in
// shape to D15's contreforts:configField -- see the fixtures' own doc
// comments for exactly what each proves and why.

#[test]
fn real_fixtures_carry_no_entity_kind_yet_and_still_validate() {
    // Neither real fixture (forgejo-declaration.ttl, o365-declaration.ttl,
    // this crate's own D14 test data -- NOT the live connector repos'
    // declaration.ttl, which already carry contreforts:entityKind as of
    // contreforts-connector-forgejo#14/contreforts-connector-o365#10,
    // ahead of this term's own vocabulary.ttl definition landing here)
    // uses contreforts:entityKind at all. Absence must be legal, mirroring
    // D15's own `real_declarations_carry_no_config_field_yet_and_still_validate`
    // above. Unlike that test, this one is already true today without any
    // lint change -- validate() runs no entityKind check yet at all -- so
    // it is not itself a RED test; it is a regression pin, run here so a
    // future change to the new lint that starts misfiring on these two
    // real fixtures fails a test whose name says exactly what broke.
    for (label, ttl) in [("o365", O365_DECLARATION), ("forgejo", FORGEJO_DECLARATION)] {
        let result = validate(ttl);
        assert!(
            result.is_ok(),
            "{label}'s fixture (no contreforts:entityKind annotations at all) must still \
             validate clean once the entityKind duplicate lint is wired in; got: {}",
            result.err().map(|v| v.to_string()).unwrap_or_default()
        );
    }
}

#[test]
fn duplicate_entity_kind_within_one_connector_namespace_is_rejected_naming_both_classes() {
    let result = validate(ENTITY_KIND_CONFLICT);
    assert!(
        result.is_err(),
        "two rdfs:Class subjects in the same connector's own namespace naming the same \
         contreforts:entityKind value must be rejected -- it is ambiguous which class a \
         synced document's rdf:type should resolve to, and \
         EntityDeclarations::resolve_class_iri (contreforts-kg) silently overwrites on \
         exactly this fact pattern today"
    );
    let violations = result.unwrap_err();
    let entity_kind: Vec<_> = violations
        .iter()
        .filter(|v| v.rule() == Rule::EntityKind)
        .collect();
    assert_eq!(
        entity_kind.len(),
        1,
        "expected exactly one duplicate-entityKind violation, got: {violations}"
    );

    let rendered = violations.to_string();
    for needle in ["PrimaryAccount", "SecondaryAccount", "\"customer\""] {
        assert!(
            rendered.contains(needle),
            "rejection message must name both offending classes and the shared value; \
             missing {needle:?} in:\n{rendered}"
        );
    }

    println!("duplicate-entityKind rejection message:\n{rendered}");
}

#[test]
fn same_entity_kind_value_across_two_different_connectors_is_not_rejected() {
    // The non-vacuous-scope proof: a naively graph-wide duplicate check
    // (mirroring lint::core_ns's own graph-wide style -- correct for D2,
    // wrong here) would reject this file, since
    // contreforts-config-api/src/routes/product_graph.rs:83 calls
    // contreforts_declaration::declarations() against
    // contreforts_product::PRODUCT_GRAPH_TTL -- the union of every enabled
    // connector's own declaration -- and declarations() shares validate()'s
    // own Part-3 lint pipeline over that WHOLE unioned graph before ever
    // splitting per connector shape (validate.rs's run_pipeline). Two
    // different connectors legitimately reusing the same
    // contreforts:entityKind value (both minting an EntityKind::CUSTOMER
    // document) must validate clean; only a conflict WITHIN one
    // connector's own namespace (entity-kind-conflict.ttl, above) is the
    // bug this lint exists to catch.
    let result = validate(ENTITY_KIND_TWO_CONNECTORS_SAME_VALUE);
    assert!(
        result.is_ok(),
        "two different connectors' own classes sharing one contreforts:entityKind value \
         must NOT be rejected -- only a conflict within one connector's own namespace is a \
         bug; got: {}",
        result.err().map(|v| v.to_string()).unwrap_or_default()
    );
}

// ── W2 (contreforts/contreforts-core#33): the sh:PropertyGroup table and the node shape's ──
// own label/description. `Declaration.groups: Vec<GroupDescriptor>` is read the same way
// `category`/`ui_shape`/`config_field` already are (sh::PROPERTY_GROUP/GROUP/ORDER plus
// `shacl_rust::utils::{get_string_value, get_integer_value}`), not via SPARQL -- SPARQL is
// only needed for `sh:group`/`sh:order`/`sh:defaultValue` on PROPERTY shapes, which
// `presentation_by_node`'s existing query already covers and which these tests do not touch
// (see `presentation_by_node`'s own SPARQL query, unedited by this change).
//
// The single most important discipline in this block: assert on group IRIs, never on
// labels. Only 10 distinct label strings cover the 17 real sh:PropertyGroup subjects
// tree-wide ("Connection" alone appears 6 times) -- a label-keyed set would silently
// under-count 17 as 10 while reporting green.

/// A `GroupDescriptor`'s IRI local name (the fragment after `#`), its own label, and its own
/// order -- the same three fields the issue's "Done when" bullets pin, in the same shape,
/// so a mismatch names exactly which group and which field is wrong rather than just "the
/// vec differs".
fn group_summary(group: &GroupDescriptor) -> (&str, Option<&str>, Option<i64>) {
    let local_name = group.iri.rsplit('#').next().unwrap_or(group.iri.as_str());
    (local_name, group.label.as_deref(), group.order)
}

#[test]
fn o365_declaration_reports_its_three_groups_by_iri_in_order() {
    let declaration = validate(O365_DECLARATION).expect("hardened O365 declaration validates");
    let summaries: Vec<_> = declaration.groups.iter().map(group_summary).collect();
    assert_eq!(
        summaries,
        vec![
            ("ConnectionGroup", Some("Connection"), Some(1)),
            ("AuthenticationGroup", Some("Authentication"), Some(2)),
            ("ScopeGroup", Some("Scope"), Some(3)),
        ],
        "O365 declares exactly 3 sh:PropertyGroup subjects (declaration.ttl:164-174), each \
         with its own rdfs:label and sh:order, sorted by (order, iri); got: {summaries:?}"
    );
    for group in &declaration.groups {
        assert!(
            group.declared,
            "every one of O365's 3 groups is typed `a sh:PropertyGroup`, so `declared` must \
             be true for all of them; {} was not",
            group.iri
        );
    }
}

#[test]
fn forgejo_declaration_reports_its_one_group() {
    let declaration = validate(FORGEJO_DECLARATION).expect("forgejo declaration validates");
    let summaries: Vec<_> = declaration.groups.iter().map(group_summary).collect();
    assert_eq!(
        summaries,
        vec![("ConnectionGroup", Some("Connection"), Some(1))],
        "forgejo declares exactly 1 sh:PropertyGroup subject (declaration.ttl:167-169); \
         got: {summaries:?}"
    );
}

#[test]
fn synthetic_minimal_declaration_reports_a_declared_group_with_no_presentation_text() {
    // synthetic-minimal-declaration.ttl:13 -- `synth:ConnectionGroup a sh:PropertyGroup .`
    // with no rdfs:label and no sh:order: the totality case already sitting on disk. A
    // declared group with no presentation text must still be REPORTED, not dropped for
    // looking empty.
    let declaration = validate(SYNTHETIC_MINIMAL).expect("synthetic-minimal validates");
    let summaries: Vec<_> = declaration.groups.iter().map(group_summary).collect();
    assert_eq!(
        summaries,
        vec![("ConnectionGroup", None, None)],
        "synth:ConnectionGroup declares neither rdfs:label nor sh:order, but IS typed \
         a sh:PropertyGroup, so it must appear once with both fields None, not be dropped; \
         got: {summaries:?}"
    );
    assert!(
        declaration.groups[0].declared,
        "synth:ConnectionGroup is typed `a sh:PropertyGroup` on disk, so declared must be \
         true, not false -- false is reserved for a group referenced but never typed"
    );
}

#[test]
fn dangling_sh_group_reference_is_reported_with_declared_false_not_dropped() {
    // dangling-group-reference.ttl: dangling:label carries `sh:group dangling:PhantomGroup`,
    // but dangling:PhantomGroup is never typed `a sh:PropertyGroup` anywhere in that graph
    // (grep -c 'PropertyGroup' on the fixture is 0). Before W2 this left no trace anywhere
    // in the model at all -- exactly the "absence presenting as success" defect class this
    // epic exists to close. `declared: bool` exists so this case is visible instead.
    let declaration =
        validate(DANGLING_GROUP_REFERENCE).expect("dangling-group-reference.ttl validates");
    assert_eq!(
        declaration.groups.len(),
        1,
        "exactly one sh:group value is referenced in this fixture (dangling:PhantomGroup), \
         and it must still surface as one GroupDescriptor even though it is never typed a \
         sh:PropertyGroup; got: {:?}",
        declaration.groups
    );
    let phantom = &declaration.groups[0];
    assert!(
        phantom.iri.ends_with("#PhantomGroup"),
        "the one group reported must be dangling:PhantomGroup itself, not some other \
         subject; got iri {}",
        phantom.iri
    );
    assert!(
        !phantom.declared,
        "dangling:PhantomGroup is referenced by sh:group but never typed \
         `a sh:PropertyGroup` anywhere in the fixture -- declared must be false, not true \
         (true would mean the dangling reference was silently treated as a real group)"
    );
    assert_eq!(
        phantom.label, None,
        "an undeclared group subject carries no rdfs:label to read"
    );
    assert_eq!(
        phantom.order, None,
        "an undeclared group subject carries no sh:order to read"
    );
}

#[test]
fn label_and_description_are_none_for_every_existing_fixture_today() {
    // W3 (gated on this issue) is what fills these in on the real connector node shapes.
    // Pinned here so W3 landing out of order, or landing silently, is visible: this test
    // must start failing the moment any of these three fixtures' own node shape gains
    // sh:name, sh:description, or an sh:name-less rdfs:label.
    for (name, declaration_ttl) in [
        ("O365", O365_DECLARATION),
        ("forgejo", FORGEJO_DECLARATION),
        ("synthetic-minimal", SYNTHETIC_MINIMAL),
    ] {
        let declaration = validate(declaration_ttl)
            .unwrap_or_else(|e| panic!("{name} declaration must validate clean: {e}"));
        assert_eq!(
            declaration.label, None,
            "{name}'s own node shape carries no sh:name/rdfs:label today; label must be None"
        );
        assert_eq!(
            declaration.description, None,
            "{name}'s own node shape carries no sh:description today; description must be \
             None"
        );
    }
}

#[test]
fn sh_name_wins_over_rdfs_label_for_the_node_shapes_own_label() {
    // shacl-rust-0.2.9's apply_common_shape_properties (parser/mod.rs:110-114) sets
    // Shape.name from sh:name, falling back to rdfs:label only when sh:name is absent. No
    // connector node shape carries sh:name today, so nothing is shadowed in the tree -- but
    // W3 is about to add rdfs:label to every connector's node shape, so this precedence
    // must be pinned NOW, before a future sh:name could silently displace it unnoticed.
    let declaration =
        validate(NODE_SHAPE_NAME_VS_LABEL).expect("node-shape-name-vs-label.ttl validates");
    assert_eq!(
        declaration.label.as_deref(),
        Some("Name Wins"),
        "the node shape carries both sh:name \"Name Wins\" and rdfs:label \"Label Loses\" -- \
         sh:name must win, per shacl-rust's own fallback order; got {:?}",
        declaration.label
    );
}

#[test]
fn groups_with_no_declared_order_sort_last_not_first() {
    // group-ordering-none-last.ttl declares GroupB (order 2) and GroupA (order 1) before
    // GroupNoOrder (no sh:order) in FIXTURE order -- but GroupNoOrder must still sort AFTER
    // both in the RESULT, per "sorted by (order, iri) with None order last". A naive
    // `Option<i64>` derived-Ord sort would put None FIRST (None < Some(_)), which this
    // fixture is deliberately shaped to catch: GroupNoOrder is declared before GroupA/GroupB
    // in the source, so a sort-by-declaration-order bug and a None-sorts-first bug would
    // both put it somewhere other than last.
    let declaration =
        validate(GROUP_ORDERING_NONE_LAST).expect("group-ordering-none-last.ttl validates");
    let summaries: Vec<_> = declaration.groups.iter().map(group_summary).collect();
    assert_eq!(
        summaries,
        vec![
            ("GroupA", Some("First"), Some(1)),
            ("GroupB", Some("Second"), Some(2)),
            ("GroupNoOrder", Some("No order"), None),
        ],
        "groups must sort by (order, iri) with a None order sorted last, regardless of the \
         order groups were declared or referenced in; got: {summaries:?}"
    );
}

// contreforts-core#35: `build_groups` (src/model.rs) is called with `Declaration`'s flat,
// top-level `properties` only, before `variants` is even folded in -- so a group referenced
// ONLY inside an sh:xone alternative's own property currently produces a `PropertyShape.group`
// with no matching `GroupDescriptor` anywhere in `Declaration.groups`. Zero real fixtures
// exercise this today (all 45 real sh:group triples tree-wide precede their own connector's
// sh:xone list), so this is exercised only by the synthetic fixture below.
#[test]
fn a_group_referenced_only_by_a_variant_property_still_gets_a_group_descriptor() {
    let declaration = validate(SYNTHETIC_VARIANT_ONLY_GROUP)
        .expect("synthetic-variant-only-group.ttl must validate clean");

    const VARIANT_ONLY_GROUP_IRI: &str =
        "https://contreforts.ds-labs.org/ontologies/variant-only-group-test#VariantOnlyGroup";

    // Non-vacuity guard: vog:VariantOnlyGroup must NOT be reachable from the flat properties
    // list at all. If it leaked in there, even the CURRENT, unfixed `build_groups` (which only
    // ever walks `properties`) would already report a descriptor for it, and the assertion
    // below on `declaration.groups` would pass for the wrong reason -- proving nothing about
    // whether variants are actually walked.
    assert!(
        !declaration
            .properties
            .iter()
            .any(|p| p.group.as_deref() == Some(VARIANT_ONLY_GROUP_IRI)),
        "vog:VariantOnlyGroup must not be referenced by any FLAT top-level property -- it is \
         deliberately variant-scoped only; if this assertion ever fails, the fixture itself is \
         broken and the test below is vacuous"
    );

    // The variant property itself really does carry the group, resolved via the same
    // presentation map the flat path uses (build_property_shape is shared by both).
    let fast_alternative = declaration
        .variants
        .iter()
        .find(|v| v.discriminant_value == "fast")
        .expect("the \"fast\" sh:xone alternative is present");
    let fast_field = fast_alternative
        .properties
        .iter()
        .find(|p| p.path.ends_with("#fastField"))
        .expect("fastField is present in the \"fast\" alternative");
    assert_eq!(
        fast_field.group.as_deref(),
        Some(VARIANT_ONLY_GROUP_IRI),
        "the fast alternative's own copy of fastField must carry sh:group -- confirming the \
         PropertyShape.group side of the defect is real, not just a fixture-authoring mistake"
    );

    // The point of #35 itself: that group reference must be resolvable to a GroupDescriptor,
    // not merely present as a dangling `PropertyShape.group` nothing else in `Declaration`
    // explains.
    let summaries: Vec<_> = declaration.groups.iter().map(group_summary).collect();
    assert!(
        summaries
            .iter()
            .any(|(local, _, _)| *local == "VariantOnlyGroup"),
        "vog:VariantOnlyGroup, referenced only inside the \"fast\" sh:xone alternative's own \
         fastField, must still produce a GroupDescriptor in Declaration.groups -- build_groups \
         must walk variants[].properties too (unioned by IRI with the flat set), not only the \
         flat properties list; got: {summaries:?}"
    );
}

// contreforts-core#31: `build_property_shape` (src/model.rs) captures sh:minLength via
// `u32::try_from(c.0).ok()` -- a negative value fails that conversion and silently becomes
// `None`, indistinguishable from "no sh:minLength declared at all". SHACL forbids a negative
// sh:minLength, but shacl-rust 0.2.9's own parser (verified by reading
// parser/constraints/min_length.rs: a bare `get_integer_value(...).map(Constraint::MinLength)`,
// no sign check at all) does not enforce that at parse time, so the malformed value reaches
// this crate's model construction unchanged. No real declaration.ttl carries a negative
// sh:minLength (all 7 real values are `1`), so this is exercised only by the synthetic fixture
// below.
#[test]
fn negative_min_length_is_recorded_as_invalid_not_absent() {
    let declaration = validate(SYNTHETIC_NEGATIVE_MIN_LENGTH)
        .expect("synthetic-negative-min-length.ttl must validate clean");

    let label = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("#label"))
        .expect("nml:label property present");
    assert_eq!(
        label.min_length, None,
        "a malformed (negative) sh:minLength must not populate the valid-value field -- it is \
         not a legal minLength"
    );
    assert_eq!(
        label.min_length_invalid,
        Some(-1),
        "nml:label's sh:minLength -1 is malformed, not absent -- the raw i32 value must be \
         recorded on a field of its own, so a malformed value is distinguishable from a \
         genuinely absent sh:minLength (see nml:plainField below, where both fields are None). \
         Not folded into `unhandled`: per phase F W1, that field's contract is 'a constraint \
         KIND this crate has no field for' -- sh:minLength DOES have a field \
         (`PropertyShape::min_length`); a malformed VALUE of a constraint that has a field is a \
         different failure mode and would corrupt that contract if folded in"
    );
    assert!(
        !label.unhandled.iter().any(|c| c == "MinLength"),
        "a malformed sh:minLength must not be recorded in `unhandled` -- that field is reserved \
         for constraint kinds with no dedicated field at all, and sh:minLength has one"
    );

    // Contrast: a property with no sh:minLength at all must leave BOTH fields None, not be
    // confused with the malformed case above.
    let plain = declaration
        .properties
        .iter()
        .find(|p| p.path.ends_with("#plainField"))
        .expect("nml:plainField property present");
    assert_eq!(
        plain.min_length, None,
        "nml:plainField declares no sh:minLength at all; min_length must be None"
    );
    assert_eq!(
        plain.min_length_invalid, None,
        "nml:plainField declares no sh:minLength at all; min_length_invalid must also be None -- \
         only a MALFORMED value populates it, not a genuinely absent one"
    );
}

// ---------------------------------------------------------------------
// D8's create-versus-update asymmetry, declaration side
// (contreforts/contreforts-workspace#19, "D8 amended -- 2026-08-11")
// ---------------------------------------------------------------------

#[test]
fn a_write_intent_pair_validates_and_is_readable_as_two_scoped_declarations() {
    // Catches two things at once, both of which would make the mechanism useless while leaving
    // every other test green:
    //   1. the pipeline rejecting the very arrangement D8 prescribes (two node shapes over one
    //      sh:targetClass) -- e.g. a coverage lint that counted shapes instead of intents;
    //   2. `Declaration::intent_scope` being hard-coded to `Always`, which would leave the two
    //      members of a pair indistinguishable to every consumer downstream of `declarations()`
    //      and make a form generator render the connector twice with no way to choose.
    validate(WRITE_INTENT_PAIR).expect("a complete create/update pair must validate clean");

    let per_shape =
        declarations(WRITE_INTENT_PAIR).expect("the same pipeline, split back out per shape");
    assert_eq!(per_shape.len(), 2, "one Declaration per node shape");

    let create = per_shape
        .iter()
        .find(|d| d.intent_scope == IntentScope::CreateOnly)
        .expect("the create half must be identifiable by its intent_scope");
    let update = per_shape
        .iter()
        .find(|d| d.intent_scope == IntentScope::UpdateOnly)
        .expect("the update half must be identifiable by its intent_scope");

    assert_eq!(
        create.target_class, update.target_class,
        "a pair is two shapes over ONE class -- that is what makes selecting between them \
         necessary in the first place"
    );

    // The one triple that differs, read back through the model: the secret is required at
    // create and not at update. If this ever reads the same on both sides, the fixture stopped
    // expressing the asymmetry and every D8 test above it is testing nothing.
    let min_count = |d: &contreforts_declaration::Declaration| {
        d.properties
            .iter()
            .find(|p| p.path.ends_with("#apiToken"))
            .expect("apiToken present on both halves")
            .min_count
    };
    assert_eq!(min_count(create), Some(1), "required when created");
    assert_eq!(
        min_count(update),
        None,
        "absent means unchanged when updated -- contreforts:secret makes the value write-only, \
         so an update carrying nothing for it is not clearing it"
    );
}

#[test]
fn a_declaration_scoped_to_create_with_no_update_shape_is_rejected() {
    // Catches: the D8 coverage lint being deleted or never wired into the pipeline. A SHACL
    // constraint is universal over its RESOLVED target set, so the uncovered intent resolves
    // zero focus nodes and conforms -- this declaration would not reject updates, it would
    // accept every one of them unchecked. Nothing else in this file or in SHACL can see that.
    let violations = validate(WRITE_INTENT_CREATE_ONLY)
        .expect_err("a create shape with no update shape must be rejected");

    let d8: Vec<_> = violations
        .iter()
        .filter(|v| v.rule() == Rule::D8WriteIntent)
        .collect();
    assert_eq!(
        d8.len(),
        1,
        "expected exactly one write-intent coverage violation, got: {violations}"
    );

    let rendered = violations.to_string();
    for needle in ["WriteIntentConnector", "update"] {
        assert!(
            rendered.contains(needle),
            "the rejection must name the class and the uncovered intent; missing {needle:?} \
             in:\n{rendered}"
        );
    }

    println!("write-intent coverage rejection message:\n{rendered}");
}

#[test]
fn write_intent_on_a_property_shape_is_rejected_by_the_meta_shapes() {
    // Catches: META-6's sh:targetClass half being dropped. The selector only ever consults
    // shapes that target a class, so the term on a property shape is read by nobody -- the
    // declaration would validate, the server would start, and "this field is create-only" would
    // be quietly false. An annotation with no effect is worth a violation.
    let violations = validate(WRITE_INTENT_ON_PROPERTY_SHAPE)
        .expect_err("contreforts:writeIntent on a property shape must be rejected");

    assert!(
        violations.iter().any(|v| v.rule() == Rule::MetaShape),
        "the misplacement is a meta-shape (META-6) violation, got: {violations}"
    );

    println!("misplaced-writeIntent rejection message:\n{violations}");
}
