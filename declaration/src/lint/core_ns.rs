//! D2 -- a declaration must not mint a term under the shared
//! `https://contreforts.ds-labs.org/ontologies/core#` namespace as a
//! subject.
//!
//! **The alignment-stub carve-out has closed (contreforts/contreforts-workspace#83, phase E /
//! E2).** contreforts-connector-forgejo#9 found that both real declarations legitimately
//! reproduced a minimal `core:` SKOS stub as alignment targets, because contreforts-core#11
//! (which would let a declaration *import* core's real concept scheme instead of restating it)
//! had not landed yet:
//!
//! ```turtle
//! core:CoreConcepts a skos:ConceptScheme .
//! core:Customer a skos:Concept ; skos:inScheme core:CoreConcepts ; skos:prefLabel "Customer"@en .
//! ```
//!
//! This module used to carve out exactly that shape: a `core:`-namespaced subject was allowed
//! if and only if the graph asserted it `rdf:type skos:Concept` or `rdf:type
//! skos:ConceptScheme`. #83 fills the gap the carve-out existed to paper over -- core now ships
//! its own canonical scheme, `concepts.ttl` (`contreforts_declaration::CONCEPTS_TTL`), unioned
//! into the Part 2 meta-shapes validation graph by `validate.rs`'s `run_meta_shapes` -- so a
//! connector has nothing left to legitimately reproduce, and the carve-out closes entirely.
//!
//! **What is legal now: reference, never define.** A declaration may still REFERENCE a `core:`
//! concept -- as the OBJECT of `skos:exactMatch`/`skos:closeMatch`/`skos:inScheme`, e.g.
//! `forgejo:Issue skos:exactMatch core:issue .` -- because this check only ever looks at
//! SUBJECTS. It may no longer DEFINE one: asserting `rdf:type skos:Concept` (or
//! `skos:ConceptScheme`) on a `core:`-namespaced subject is now rejected exactly like minting
//! any other term there (a class, a property, a shape, a plain resource) always was. `core:` is
//! core's namespace to define terms in, full stop; an extension's own vocabulary belongs in its
//! own namespace, per D2.
//!
//! Why this stays a Rust structural lint, not a meta-shapes.ttl SHACL shape: the check is
//! graph-wide ("does any subject, wherever it appears, live under core:"), and shacl-rust
//! 0.2.9's SPARQL-based targeting (`sh:target`/`sh:SPARQLTarget`) is exactly the advanced-feature
//! surface contreforts-workspace#21 flagged as maturity risk -- a Rust loop over the graph's own
//! triples is more reliable and produces a message naming the exact offending subject, matching
//! D14's own approach.
//!
//! Note for anyone touching how `concepts.ttl` is wired in: this check runs against
//! `run_pipeline`'s own `declaration_graph` (Part 3), which is NEVER unioned with anything else
//! (see `validate.rs`'s own comments on `CONCEPTS_TTL` and `run_meta_shapes`). `concepts.ttl`
//! must stay that way -- unioned only into Part 2's meta-shapes data graph -- or core's own 15
//! concepts would look like a connector-defined `core:` subject and this lint would reject
//! every declaration outright.

use std::collections::BTreeSet;

use crate::error::Violation;

const CORE_NS: &str = "https://contreforts.ds-labs.org/ontologies/core#";

/// Runs the D2 core:-namespace check over the whole declaration graph.
pub(crate) fn check(graph: &oxigraph::model::Graph) -> Vec<Violation> {
    let mut core_subjects: BTreeSet<String> = BTreeSet::new();
    for triple in graph.iter() {
        if let oxigraph::model::NamedOrBlankNodeRef::NamedNode(subject) = triple.subject
            && subject.as_str().starts_with(CORE_NS)
        {
            core_subjects.insert(subject.as_str().to_string());
        }
    }

    core_subjects
        .into_iter()
        .map(|subject_iri| {
            Violation::d2_core_namespace(
                Some(format!("<{subject_iri}>")),
                format!(
                    "this subject is minted under the shared core: namespace \
                     ({CORE_NS}), which is reserved for core's own canonical concept \
                     scheme (concepts.ttl). A connector may REFERENCE a core: concept \
                     (as the object of skos:exactMatch/closeMatch/inScheme) but may not \
                     DEFINE one -- a connector's own vocabulary belongs in its own \
                     namespace, not core:."
                ),
            )
        })
        .collect()
}
