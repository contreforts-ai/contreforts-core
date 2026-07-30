//! Shared platform vocabulary: RDF namespace constants and IRI builders used on both sides of
//! the future config/KG store split (or by the config side alone).
//!
//! Relocated here from `contreforts-kg::namespaces` (contreforts/contreforts-workspace#58,
//! comment 7833, item D3b -- correcting comment 7791's D3b row, which would have put this
//! vocabulary in a not-yet-existing `contreforts-config` crate instead). 7833's usage table
//! showed `CORE_NS`, `RDF`, `DATA_NS`, `company_graph_iri` and `company_iri` are used on **both**
//! sides of that split, so the vocabulary moves down into `contreforts-core` rather than sideways
//! into config: neither store crate should have to depend on the other just to reach its own
//! vocabulary. `contreforts-kg::namespaces` re-exports every item declared here, so its
//! downstream callers across `contreforts-kg`, `contreforts-config-api` and `contreforts-rag`
//! keep compiling unchanged.
//!
//! The entity and instance vocabulary (`doc_iri`, `doctype_iri`, `field_iri`,
//! `consolidated_doc_iri`, `consolidated_class_iri`, the predicate helpers, `interaction_iri`)
//! stays in `contreforts-kg`: `contreforts/contreforts-kg#30` and phase E actively edit that
//! vocabulary, and it is not used from the config side.

// `pub static`, not `pub const`, is deliberate here (contreforts/contreforts-workspace#58,
// comment 7875): a `const` has no stable address -- it is inlined at each use site, and its
// string data is subject to linker merging across crates, so an independently-declared copy
// with a byte-identical value compares EQUAL to the original under `std::ptr::eq`. That made
// the identity guards in `contreforts-kg/tests/namespaces_reexport.rs` decorative: they passed
// whether or not the re-export was genuine. A `static` item has a single, stable address shared
// by every name that refers to it (including a `pub use` re-export), so the same `ptr::eq`
// comparison style correctly distinguishes a real re-export from a coincidentally-equal copy.
// Verified safe to convert: nothing in the workspace uses these in a `const` context (`const
// fn`, or assigning into another `const`) or in a `match` pattern -- both illegal for `static`.
pub static CORE_NS: &str = "https://contreforts.ds-labs.org/ontologies/core#";
pub static DATA_NS: &str = "https://contreforts.ds-labs.org/data/";
pub static SCHEMA: &str = "http://schema.org/";
pub static RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub static RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
pub static XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// Named graph that holds all configuration triples (companies, connectors).
pub static CONFIG_GRAPH: &str = "https://contreforts.ds-labs.org/data/graph/config";

/// Named graph IRI for a company's data; also serves as the IRI prefix for that company's documents.
/// Format: `https://contreforts.ds-labs.org/data/{company}/`
pub fn company_graph_iri(company_slug: &str) -> String {
    format!("{DATA_NS}{}/", urlencoding::encode(company_slug))
}

/// IRI for a Company in the config graph.
/// Format: `{DATA_NS}company/{slug}`
pub fn company_iri(slug: &str) -> String {
    format!("{DATA_NS}company/{}", urlencoding::encode(slug))
}

/// IRI for a connector node, scoped to a company and optionally a label.
///
/// `kind` is the connector type (e.g. `"erpnext"`, `"forgejo"`) and is inserted verbatim,
/// unencoded, exactly as the former per-connector functions did.
///
/// Two distinct, and *not* interchangeable, shapes exist:
/// - Singleton connectors (`erpnext`, `pennylane` — at most one instance per company) pass
///   `label: None` and get a **two**-segment path: `{DATA_NS}connector/{kind}/{company_slug}`.
/// - Label-scoped connectors (every other kind) pass `label: Some(_)` and get a **three**-segment
///   path: `{DATA_NS}connector/{kind}/{company_slug}/{label}`.
///
/// This asymmetry mirrors durable, already-stored IRIs and must not be collapsed into a single
/// shape — doing so would silently change stored data.
pub fn connector_iri(kind: &str, company_slug: &str, label: Option<&str>) -> String {
    match label {
        Some(label) => format!(
            "{DATA_NS}connector/{kind}/{}/{}",
            urlencoding::encode(company_slug),
            urlencoding::encode(label)
        ),
        None => format!(
            "{DATA_NS}connector/{kind}/{}",
            urlencoding::encode(company_slug)
        ),
    }
}

/// IRI for a SPARQL Template node, scoped to a company and label.
pub fn sparql_template_iri(company_slug: &str, label: &str) -> String {
    format!(
        "{DATA_NS}template/sparql/{}/{}",
        urlencoding::encode(company_slug),
        urlencoding::encode(label)
    )
}

/// IRI for a KnowledgeBase node, scoped to a company and label.
pub fn knowledge_base_iri(company_slug: &str, label: &str) -> String {
    format!(
        "{DATA_NS}kb/{}/{}",
        urlencoding::encode(company_slug),
        urlencoding::encode(label)
    )
}

/// IRI for an Agent node, scoped to a company and label.
pub fn agent_iri(company_slug: &str, label: &str) -> String {
    format!(
        "{DATA_NS}agent/{}/{}",
        urlencoding::encode(company_slug),
        urlencoding::encode(label)
    )
}

/// IRI for a group→customer mapping.
pub fn group_mapping_iri(
    connector_type: &str,
    company_slug: &str,
    label: &str,
    group_path: &str,
) -> String {
    format!(
        "{DATA_NS}mapping/{}/{}/{}/{}",
        urlencoding::encode(connector_type),
        urlencoding::encode(company_slug),
        urlencoding::encode(label),
        urlencoding::encode(group_path)
    )
}
