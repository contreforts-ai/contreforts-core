//! Tests for `contreforts_declaration::CONCEPTS_TTL`, core's own canonical SKOS concept scheme
//! (contreforts/contreforts-workspace#83, phase E / E2).
//!
//! ## What this file checks, and what it does not
//!
//! This file pins `concepts.ttl`'s own shape in isolation: it exists, it defines exactly one
//! `skos:ConceptScheme` (`core:CoreConcepts`), exactly 15 `skos:Concept`s under it, each with a
//! `skos:inScheme` back-reference and a `skos:prefLabel`. The 15 expected local names below are
//! a hand-written list -- deliberately: the one test that must NOT compare against a
//! hand-written list (a 16th `EntityKind` constant added without a matching concept would not
//! move a hand-written list, and that test would keep passing for the wrong reason) is the
//! Rust<->Turtle coupling test, and it cannot live here. `contreforts-declaration`'s own
//! `Cargo.toml` says twice, in comments, that this crate "must depend on nothing about the
//! superproject" and must "stand on its own even outside that workspace" -- it cannot depend on
//! `contreforts-core` to reach `EntityKind`, and `contreforts-core`'s own `exclude =
//! ["declaration"]` (Cargo.toml:8) confirms neither crate depends on the other (verified
//! directly against `develop` at `1b0e1b5`, 2026-07-31, not assumed from the issue body, which
//! was stale in exactly this way for the E1 chain). `contreforts-kg` depends on both
//! (`Cargo.toml`: `contreforts-core = { workspace = true }`, `contreforts-declaration = { path =
//! "../contreforts-core/declaration" }`), so the authoritative equality check --
//! `concepts.ttl`'s concept set literally equals `EntityKind::CORE_TERMS`'s 15 `as_str()` values,
//! read from Rust, not retyped here -- lives in `contreforts-kg/tests/
//! core_concepts_coupling.rs`. Nothing in this file substitutes for that one.
//!
//! ## RED discipline (contreforts/contreforts-workspace#83)
//!
//! `contreforts_declaration::CONCEPTS_TTL` does not exist yet -- a2's job is to add
//! `crates/contreforts-core/declaration/src/concepts.ttl` and `pub const CONCEPTS_TTL: &str =
//! include_str!("concepts.ttl");` to `declaration/src/lib.rs`, unioned into `run_pipeline`
//! exactly as `META_SHAPES_TTL` is. Every `use` of `CONCEPTS_TTL` below fails to compile until
//! then -- a legitimate RED per this repo's own `contreforts-kg/CONTRIBUTING.md` §3 ("a compile
//! error against a not-yet-existing symbol counts").

use std::collections::BTreeSet;

use contreforts_declaration::{CONCEPTS_TTL, validate};
use oxigraph::model::{NamedNodeRef, NamedOrBlankNodeRef, TermRef};

const CORE_NS: &str = "https://contreforts.ds-labs.org/ontologies/core#";
const RDF_TYPE: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
const SKOS_CONCEPT: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("http://www.w3.org/2004/02/skos/core#Concept");
const SKOS_CONCEPT_SCHEME: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("http://www.w3.org/2004/02/skos/core#ConceptScheme");
const SKOS_IN_SCHEME: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("http://www.w3.org/2004/02/skos/core#inScheme");
const SKOS_PREF_LABEL: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("http://www.w3.org/2004/02/skos/core#prefLabel");

/// The 15 `EntityKind::as_str()` values, per contreforts-core#18 / models.rs:76-90 and this
/// issue's "What to build" item 1. Hand-written here deliberately -- see this file's own module
/// doc comment for why that is fine for a shape-only pin but would not be fine for the coupling
/// test (which lives in `contreforts-kg` instead, reading `EntityKind::CORE_TERMS` from Rust).
const EXPECTED_CORE_TERMS: [&str; 15] = [
    "customer",
    "contact",
    "invoice",
    "company",
    "customer-group",
    "territory",
    "project",
    "issue",
    "item",
    "quotation",
    "meeting",
    "resolution",
    "calendar",
    "calendar-event",
    "interaction",
];

const ENTITY_KIND_EXTENSION_TERM: &str = include_str!("fixtures/entity-kind-extension-term.ttl");

fn parsed_concepts_graph() -> oxigraph::model::Graph {
    shacl_rust::rdf::read_graph_from_string(CONCEPTS_TTL, "turtle")
        .expect("CONCEPTS_TTL must be valid Turtle")
}

#[test]
fn concepts_ttl_defines_exactly_one_scheme_named_core_concepts() {
    let graph = parsed_concepts_graph();
    let scheme_subjects: BTreeSet<String> = graph
        .iter()
        .filter_map(|triple| {
            let NamedOrBlankNodeRef::NamedNode(subject) = triple.subject else {
                return None;
            };
            if triple.predicate == RDF_TYPE
                && matches!(triple.object, TermRef::NamedNode(n) if n == SKOS_CONCEPT_SCHEME)
            {
                Some(subject.as_str().to_string())
            } else {
                None
            }
        })
        .collect();

    assert_eq!(
        scheme_subjects,
        BTreeSet::from([format!("{CORE_NS}CoreConcepts")]),
        "concepts.ttl must define exactly one skos:ConceptScheme, core:CoreConcepts; got {scheme_subjects:?}"
    );
}

#[test]
fn concepts_ttl_defines_exactly_the_fifteen_expected_concepts() {
    let graph = parsed_concepts_graph();
    let mut core_subjects: BTreeSet<String> = BTreeSet::new();
    for triple in graph.iter() {
        if let NamedOrBlankNodeRef::NamedNode(subject) = triple.subject
            && subject.as_str().starts_with(CORE_NS)
        {
            core_subjects.insert(subject.as_str().to_string());
        }
    }

    let concept_local_names: BTreeSet<String> = core_subjects
        .into_iter()
        .filter(|iri| {
            let subject = NamedNodeRef::new(iri).expect("collected from a valid graph term");
            graph
                .objects_for_subject_predicate(subject, RDF_TYPE)
                .any(|term| matches!(term, TermRef::NamedNode(n) if n == SKOS_CONCEPT))
        })
        .map(|iri| iri.trim_start_matches(CORE_NS).to_string())
        .collect();

    let expected: BTreeSet<String> = EXPECTED_CORE_TERMS.iter().map(|s| s.to_string()).collect();

    assert_eq!(
        concept_local_names, expected,
        "concepts.ttl's skos:Concept set must equal exactly the 15 EntityKind::as_str() values"
    );
}

#[test]
fn every_concept_carries_in_scheme_and_a_pref_label() {
    let graph = parsed_concepts_graph();
    for local_name in EXPECTED_CORE_TERMS {
        let iri = format!("{CORE_NS}{local_name}");
        let subject = NamedNodeRef::new(&iri).expect("well-formed IRI");

        let in_scheme_target = graph
            .objects_for_subject_predicate(subject, SKOS_IN_SCHEME)
            .find_map(|term| match term {
                TermRef::NamedNode(n) => Some(n.as_str().to_string()),
                _ => None,
            });
        assert_eq!(
            in_scheme_target.as_deref(),
            Some(format!("{CORE_NS}CoreConcepts").as_str()),
            "core:{local_name} must carry skos:inScheme core:CoreConcepts"
        );

        let has_pref_label = graph
            .objects_for_subject_predicate(subject, SKOS_PREF_LABEL)
            .any(|term| matches!(term, TermRef::Literal(lit) if !lit.value().is_empty()));
        assert!(
            has_pref_label,
            "core:{local_name} must carry a non-empty skos:prefLabel"
        );
    }
}

#[test]
fn entity_kind_value_with_no_core_concept_still_validates() {
    // contreforts/contreforts-workspace#83 (E2), "What to build" item 2 / "Done when": the
    // scheme is authoritative for core's own 15 terms, NOT exhaustive of every legal
    // contreforts:entityKind value -- EntityKind is an open newtype (contreforts-core#18). A
    // connector minting "risk-scenario" (not one of the 15) must still validate clean; nothing
    // in E2 may add a closed-world check comparing entityKind values against concepts.ttl.
    //
    // The guard below (risk-scenario absent from CONCEPTS_TTL's own concept set) keeps this
    // fixture honest: if "risk-scenario" were ever accidentally promoted into concepts.ttl as a
    // 16th core concept, this test would stop proving anything about the open-extension case,
    // and this assertion catches that before the `validate` assertion below could paper over it.
    let concepts_local_names: BTreeSet<String> = {
        let graph = parsed_concepts_graph();
        let mut core_subjects: BTreeSet<String> = BTreeSet::new();
        for triple in graph.iter() {
            if let NamedOrBlankNodeRef::NamedNode(subject) = triple.subject
                && subject.as_str().starts_with(CORE_NS)
            {
                core_subjects.insert(subject.as_str().to_string());
            }
        }
        core_subjects
            .into_iter()
            .filter(|iri| {
                let subject = NamedNodeRef::new(iri).expect("collected from a valid graph term");
                graph
                    .objects_for_subject_predicate(subject, RDF_TYPE)
                    .any(|term| matches!(term, TermRef::NamedNode(n) if n == SKOS_CONCEPT))
            })
            .map(|iri| iri.trim_start_matches(CORE_NS).to_string())
            .collect()
    };
    assert!(
        !concepts_local_names.contains("risk-scenario"),
        "this fixture's premise (an entityKind value with NO matching core concept) is broken: \
         'risk-scenario' is now a core concept"
    );

    let result = validate(ENTITY_KIND_EXTENSION_TERM);
    assert!(
        result.is_ok(),
        "an entityKind value absent from core's concept scheme must still validate -- the \
         scheme is authoritative for core's 15, not exhaustive of every legal kind; got: {}",
        result.err().map(|v| v.to_string()).unwrap_or_default()
    );
}
