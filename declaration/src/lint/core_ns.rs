//! D2 -- a declaration must not mint a term under the shared
//! `https://contreforts.ds-labs.org/ontologies/core#` namespace as a
//! subject, with one deliberate exception.
//!
//! **The alignment-stub carve-out.** contreforts-connector-forgejo#9 found
//! that both real declarations legitimately reproduce a minimal `core:`
//! SKOS stub as alignment targets, because contreforts-core#11 (which
//! would let a declaration *import* core's real concept scheme instead of
//! restating it) has not landed yet:
//!
//! ```turtle
//! core:CoreConcepts a skos:ConceptScheme .
//! core:Customer a skos:Concept ; skos:inScheme core:CoreConcepts ; skos:prefLabel "Customer"@en .
//! ```
//!
//! Of the three choices the issue names -- a whitelist, a
//! `skos:ConceptScheme` carve-out, or deferring the rule to C7 -- this
//! crate implements the **carve-out**: a `core:`-namespaced subject is
//! allowed if and only if the graph asserts it `rdf:type skos:Concept` or
//! `rdf:type skos:ConceptScheme`. Anything else minted under `core:` as a
//! subject -- a class, a property, a shape, a plain resource with no SKOS
//! typing -- is rejected.
//!
//! Why the carve-out and not a whitelist: a whitelist of the *specific*
//! IRIs the two real files happen to reproduce (`core:Customer`,
//! `core:Calendar`, ...) is exactly the "restates what these two files
//! happen to do" failure mode Part 2 warns about -- the third connector's
//! own alignment target would be a namespace collision by construction. A
//! carve-out on *shape* (is this a SKOS concept/scheme?) generalizes to
//! any future connector's alignment stub without a crate change. Why not
//! deferring to C7: the issue's Part 5 asks this crate's own tests to
//! prove the rejection today, and both real fixtures are already
//! available to test it against -- there is no reason to ship the rule
//! disabled when it can be shipped correct.
//!
//! Implemented as a Rust structural lint, not a meta-shapes.ttl SHACL
//! shape: the check is graph-wide ("does this subject, wherever it
//! appears, look like anything other than an alignment stub"), and
//! shacl-rust 0.2.9's SPARQL-based targeting (`sh:target`/
//! `sh:SPARQLTarget`) is exactly the advanced-feature surface
//! contreforts-workspace#21 flagged as maturity risk -- a Rust loop over
//! the graph's own triples is more reliable and produces a message naming
//! the exact offending subject, matching D14's own approach.

use std::collections::BTreeSet;

use oxigraph::model::{NamedNodeRef, TermRef};

use crate::error::Violation;

const CORE_NS: &str = "https://contreforts.ds-labs.org/ontologies/core#";
const RDF_TYPE: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
const SKOS_CONCEPT: &str = "http://www.w3.org/2004/02/skos/core#Concept";
const SKOS_CONCEPT_SCHEME: &str = "http://www.w3.org/2004/02/skos/core#ConceptScheme";

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

    let mut violations = Vec::new();
    for subject_iri in core_subjects {
        let subject = NamedNodeRef::new(&subject_iri).expect("collected from a valid graph term");
        let is_alignment_stub = graph
            .objects_for_subject_predicate(subject, RDF_TYPE)
            .any(|term| {
                matches!(
                    term,
                    TermRef::NamedNode(n) if n.as_str() == SKOS_CONCEPT || n.as_str() == SKOS_CONCEPT_SCHEME
                )
            });

        if !is_alignment_stub {
            violations.push(Violation::d2_core_namespace(
                Some(format!("<{subject_iri}>")),
                format!(
                    "this subject is minted under the shared core: namespace \
                     ({CORE_NS}), which is reserved for the SKOS alignment-stub \
                     carve-out (rdf:type skos:Concept or skos:ConceptScheme). A \
                     connector's own vocabulary belongs in its own namespace, not core:."
                ),
            ));
        }
    }

    violations
}
