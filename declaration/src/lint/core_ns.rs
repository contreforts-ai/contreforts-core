//! D2 -- a declaration must not mint a term under the shared
//! `https://contreforts.ds-labs.org/ontologies/core#` namespace as a
//! subject, EXCEPT for the exact subjects core's own canonical scheme
//! (`concepts.ttl`, `crate::CONCEPTS_TTL`) defines.
//!
//! **History, corrected by contreforts/contreforts-core#26.** contreforts-connector-forgejo#9
//! found that both real declarations legitimately reproduced a minimal `core:` SKOS stub as
//! alignment targets, before core shipped a real scheme of its own to reference instead:
//!
//! ```turtle
//! core:CoreConcepts a skos:ConceptScheme .
//! core:Customer a skos:Concept ; skos:inScheme core:CoreConcepts ; skos:prefLabel "Customer"@en .
//! ```
//!
//! contreforts/contreforts-workspace#83 (E2) meant to tighten that into "reference, never
//! define": a declaration may still point at a `core:` concept as an object, but may no longer
//! assert `rdf:type skos:Concept`/`skos:ConceptScheme` on a `core:`-namespaced subject itself.
//! What actually shipped went further than specified and rejected *every* `core:`-namespaced
//! subject unconditionally, with no exception at all -- including core's own scheme, the one
//! legitimate case. That went unnoticed because every `.ttl` on disk was checked, and none
//! defines a `core:` subject outside `concepts.ttl` and two negative-test fixtures --
//! `contreforts-product`'s aggregate `PRODUCT_GRAPH_TTL` (assembled at build time, on no `.ttl`
//! on disk) is the graph that actually surfaces the gap, once
//! contreforts/contreforts-config-api#35 unions `CONCEPTS_TTL` into it.
//!
//! **What is legal now: reference anything, define only what core itself defines.** A
//! declaration may REFERENCE a `core:` concept -- as the OBJECT of
//! `skos:exactMatch`/`skos:closeMatch`/`skos:inScheme`, e.g. `forgejo:Issue skos:exactMatch
//! core:issue .` -- because this check only ever looks at SUBJECTS. Among subjects, exactly
//! those [`permitted_core_subjects`] returns -- every subject `crate::CONCEPTS_TTL` itself
//! defines, derived from that Turtle at runtime rather than hardcoded as a list of however many
//! concepts core ships today, so a 16th concept added to the scheme needs no matching edit
//! here -- are not violations; every other `core:`-namespaced subject still is, exactly as
//! minting any other term there (a class, a property, a shape, a plain resource) always was.
//! `core:` is core's namespace to define terms in, full stop; an extension's own vocabulary
//! belongs in its own namespace, per D2.
//!
//! **Deliberately not defended against** (contreforts-core#26's own text): a declaration that
//! restates one of `CONCEPTS_TTL`'s own triples verbatim now passes, indistinguishably from
//! core's own scheme legitimately appearing in an aggregate graph. That is idempotent -- it
//! contributes nothing a union of the two would not already carry -- and rejecting it would
//! require knowing *who authored* a triple, which a graph-scoped lint over the assembled result
//! cannot do.
//!
//! Why this stays a Rust structural lint, not a meta-shapes.ttl SHACL shape: the check is
//! graph-wide ("does any subject, wherever it appears, live under core:"), and shacl-rust
//! 0.2.9's SPARQL-based targeting (`sh:target`/`sh:SPARQLTarget`) is exactly the advanced-feature
//! surface contreforts-workspace#21 flagged as maturity risk -- a Rust loop over the graph's own
//! triples is more reliable and produces a message naming the exact offending subject, matching
//! D14's own approach.
//!
//! Note for anyone touching how `concepts.ttl` is wired in: this check runs against
//! `run_pipeline`'s own `declaration_graph` (Part 3), which `validate.rs`'s own `run_meta_shapes`
//! never unions `CONCEPTS_TTL` into -- that stays true, and is orthogonal to this module's own
//! permitted-subject exception above: [`permitted_core_subjects`] reads `crate::CONCEPTS_TTL`
//! directly to compute what to allow, it does not rely on `concepts.ttl`'s triples having been
//! folded into whatever graph is passed to [`check`].

use std::collections::BTreeSet;
use std::sync::OnceLock;

use crate::error::Violation;

const CORE_NS: &str = "https://contreforts.ds-labs.org/ontologies/core#";

/// Every `core:`-namespaced subject `crate::CONCEPTS_TTL` itself asserts a triple about --
/// core's own scheme (`core:CoreConcepts`) plus each of its concepts -- computed once, from
/// `CONCEPTS_TTL`'s own Turtle, not hardcoded as a list or a count. Growing the scheme (a 16th
/// concept) changes what this returns automatically, with no edit needed here.
fn permitted_core_subjects() -> &'static BTreeSet<String> {
    static PERMITTED: OnceLock<BTreeSet<String>> = OnceLock::new();
    PERMITTED.get_or_init(|| {
        let concepts_graph = shacl_rust::rdf::read_graph_from_string(crate::CONCEPTS_TTL, "turtle")
            .unwrap_or_else(|e| {
                panic!(
                    "contreforts-declaration's own concepts.ttl failed to parse -- this is \
                         a bug in contreforts-declaration itself, not in any declaration being \
                         validated: {e}"
                )
            });
        concepts_graph
            .iter()
            .filter_map(|triple| match triple.subject {
                oxigraph::model::NamedOrBlankNodeRef::NamedNode(subject) => {
                    let iri = subject.as_str();
                    iri.starts_with(CORE_NS).then(|| iri.to_string())
                }
                _ => None,
            })
            .collect()
    })
}

/// Runs the D2 core:-namespace check over the whole declaration graph.
pub(crate) fn check(graph: &oxigraph::model::Graph) -> Vec<Violation> {
    let permitted = permitted_core_subjects();
    let mut core_subjects: BTreeSet<String> = BTreeSet::new();
    for triple in graph.iter() {
        if let oxigraph::model::NamedOrBlankNodeRef::NamedNode(subject) = triple.subject
            && subject.as_str().starts_with(CORE_NS)
            && !permitted.contains(subject.as_str())
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
