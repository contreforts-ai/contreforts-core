//! Tests for the shared platform vocabulary in its new home, `contreforts_core::namespaces`.
//!
//! Relocated here from `contreforts-kg::namespaces` (contreforts/contreforts-workspace#58,
//! comment 7833, item D3b -- correcting comment 7791's D3b row, which had put this vocabulary in
//! a not-yet-existing `contreforts-config` crate). 7833's usage table showed `CORE_NS`, `RDF`,
//! `DATA_NS`, `company_graph_iri` and `company_iri` are used on **both** sides of the future
//! config/KG store split, so they move down into `contreforts-core` instead of sideways into
//! config: neither store crate should have to depend on the other to reach its own vocabulary.
//!
//! Moved here: the six base constants (`CORE_NS`, `DATA_NS`, `SCHEMA`, `RDF`, `RDFS`, `XSD`),
//! `CONFIG_GRAPH`, and the IRI builders used by both sides or by config alone
//! (`company_graph_iri`, `company_iri`, `connector_iri`, `sparql_template_iri`,
//! `knowledge_base_iri`, `agent_iri`, `group_mapping_iri`).
//!
//! Two of the tests below (`company_graph_iri`'s and `connector_iri`'s) are ported near-verbatim
//! from `contreforts-kg/src/namespaces.rs`'s own `#[cfg(test)]` module (there since before this
//! move, at line 164): only the module path changed, from bare `company_graph_iri(...)` /
//! `connector_iri(...)` (called from inside the same module) to
//! `contreforts_core::namespaces::company_graph_iri(...)` /
//! `contreforts_core::namespaces::connector_iri(...)` (called from an external integration test).
//! The other two tests in that old module -- `test_doc_iri_with_spaces` and
//! `test_doctype_iri_with_special_chars` -- cover `doc_iri` and `doctype_iri`, which are **not**
//! moving (they are entity/instance builders that stay in `contreforts-kg`, live targets of
//! `contreforts/contreforts-kg#30` and phase E); they are not ported here, and must not be, per
//! 7833's explicit split.
//!
//! Every other test in this file is new: literal-value pins for all seven moved constants (the
//! values every IRI in the system is built from -- a silent change to one is unrecoverable), and
//! coverage of the five moved builders the old test module never exercised at all
//! (`company_iri`, `sparql_template_iri`, `knowledge_base_iri`, `agent_iri`,
//! `group_mapping_iri`), including their `urlencoding`-based escaping behaviour on inputs with
//! spaces and slashes.
//!
//! Unlike the old `contreforts-kg` test module, these assertions do not re-parse each output
//! through `oxigraph::model::NamedNode` -- `contreforts-core` has no `oxigraph` dependency today,
//! and adding one (even as a dev-dependency) purely to validate a string in a test would itself
//! be a small implementation decision, which is not this file's job. Plain string equality
//! against a hand-typed literal is enough to catch a change in shape or encoding, which is what
//! these tests are for.
//!
//! ## RED discipline (contreforts-workspace#58; see `contreforts-kg/CONTRIBUTING.md` §3 -- a
//! compile error against a not-yet-existing symbol counts as the spec)
//!
//! This file does not compile yet: `contreforts_core::namespaces` does not exist until a2's move
//! lands. That is the intended RED for this step -- see the task report for the exact `rustc`
//! output.

use contreforts_core::namespaces::{
    CONFIG_GRAPH, CORE_NS, DATA_NS, RDF, RDFS, SCHEMA, XSD, agent_iri, company_graph_iri,
    company_iri, connector_iri, group_mapping_iri, knowledge_base_iri, sparql_template_iri,
};

// ---------------------------------------------------------------------------------------------
// Literal values of the seven moved constants.
//
// These are asserted against hand-typed string literals, never against each other or against
// anything derived from the constant itself -- the whole point is to catch an accidental edit to
// the literal during the move, not to check that the constant equals itself.
// ---------------------------------------------------------------------------------------------

#[test]
fn core_ns_literal_value() {
    assert_eq!(CORE_NS, "https://contreforts.ds-labs.org/ontologies/core#");
}

#[test]
fn data_ns_literal_value() {
    assert_eq!(DATA_NS, "https://contreforts.ds-labs.org/data/");
}

#[test]
fn schema_literal_value() {
    assert_eq!(SCHEMA, "http://schema.org/");
}

#[test]
fn rdf_literal_value() {
    assert_eq!(RDF, "http://www.w3.org/1999/02/22-rdf-syntax-ns#");
}

#[test]
fn rdfs_literal_value() {
    assert_eq!(RDFS, "http://www.w3.org/2000/01/rdf-schema#");
}

#[test]
fn xsd_literal_value() {
    assert_eq!(XSD, "http://www.w3.org/2001/XMLSchema#");
}

#[test]
fn config_graph_literal_value() {
    assert_eq!(
        CONFIG_GRAPH,
        "https://contreforts.ds-labs.org/data/graph/config"
    );
}

// ---------------------------------------------------------------------------------------------
// `company_graph_iri` -- ported near-verbatim from `contreforts-kg/src/namespaces.rs:183-188`
// (the old `test_company_graph_iri`). Would catch: a change to the company-graph IRI shape, or
// to `DATA_NS`, breaking either the literal format or `NamedNode` validity.
// ---------------------------------------------------------------------------------------------

#[test]
fn company_graph_iri_builds_expected_iri() {
    let iri = company_graph_iri("acme");
    assert_eq!(iri, "https://contreforts.ds-labs.org/data/acme/");
}

/// New: `company_graph_iri` is one of the builders whose current implementation goes through
/// `urlencoding::encode` on its argument. This pins that escaping behaviour explicitly, since
/// nothing else in the ported test above exercises it (the ported test's input, "acme", has
/// nothing to escape). Would catch: `contreforts-core` porting the builder without its
/// `urlencoding` call, or swapping in a different encoding scheme.
#[test]
fn company_graph_iri_urlencodes_its_argument() {
    let iri = company_graph_iri("acme corp/qa");
    assert_eq!(
        iri,
        "https://contreforts.ds-labs.org/data/acme%20corp%2Fqa/"
    );
}

// ---------------------------------------------------------------------------------------------
// `connector_iri` -- ported near-verbatim from `contreforts-kg/src/namespaces.rs:203-216` (the
// old `test_connector_iri_singleton_and_labelled_shapes`). Would catch: either of the two shapes
// (singleton, two-segment vs. labelled, three-segment) being collapsed or altered -- these IRIs
// are stored data, per the moved doc comment's own warning.
// ---------------------------------------------------------------------------------------------

#[test]
fn connector_iri_singleton_and_labelled_shapes() {
    let singleton = connector_iri("erpnext", "acme", None);
    assert_eq!(
        singleton,
        "https://contreforts.ds-labs.org/data/connector/erpnext/acme"
    );

    let labelled = connector_iri("forgejo", "acme", Some("main"));
    assert_eq!(
        labelled,
        "https://contreforts.ds-labs.org/data/connector/forgejo/acme/main"
    );
}

/// New: `connector_iri`'s doc comment says `kind` is inserted "verbatim, unencoded", while
/// `company_slug` and `label` go through `urlencoding::encode`. This pins that asymmetry: a
/// `kind` containing a character that would need escaping is NOT escaped (matching "former
/// per-connector functions"), while `company_slug`/`label` are. Would catch: the asymmetry being
/// smoothed away (e.g. `kind` accidentally gaining encoding, or losing it for the other two).
#[test]
fn connector_iri_encodes_slug_and_label_but_not_kind() {
    let labelled = connector_iri("vector_store", "acme/qa", Some("primary team"));
    assert_eq!(
        labelled,
        "https://contreforts.ds-labs.org/data/connector/vector_store/acme%2Fqa/primary%20team"
    );
}

// ---------------------------------------------------------------------------------------------
// New coverage: the five moved builders the pre-move test module never exercised.
// ---------------------------------------------------------------------------------------------

/// Would catch: a change to `company_iri`'s path shape or a lost `urlencoding` call.
#[test]
fn company_iri_builds_expected_iri_and_encodes_slug() {
    let iri = company_iri("acme co");
    assert_eq!(
        iri,
        "https://contreforts.ds-labs.org/data/company/acme%20co"
    );
}

/// Would catch: a change to `sparql_template_iri`'s path shape, segment order, or escaping.
#[test]
fn sparql_template_iri_builds_expected_iri_and_encodes_label() {
    let iri = sparql_template_iri("acme", "top customers/eu");
    assert_eq!(
        iri,
        "https://contreforts.ds-labs.org/data/template/sparql/acme/top%20customers%2Feu"
    );
}

/// Would catch: a change to `knowledge_base_iri`'s path shape, segment order, or escaping.
#[test]
fn knowledge_base_iri_builds_expected_iri_and_encodes_label() {
    let iri = knowledge_base_iri("acme", "support kb");
    assert_eq!(
        iri,
        "https://contreforts.ds-labs.org/data/kb/acme/support%20kb"
    );
}

/// Would catch: a change to `agent_iri`'s path shape, segment order, or escaping.
#[test]
fn agent_iri_builds_expected_iri_and_encodes_label() {
    let iri = agent_iri("acme", "billing/support");
    assert_eq!(
        iri,
        "https://contreforts.ds-labs.org/data/agent/acme/billing%2Fsupport"
    );
}

/// Would catch: a change to `group_mapping_iri`'s four-segment shape, segment order, or escaping
/// of any of its four independently-encoded arguments.
#[test]
fn group_mapping_iri_builds_expected_iri_and_encodes_every_segment() {
    let iri = group_mapping_iri("gitlab", "acme corp", "main/eu", "team a/b");
    assert_eq!(
        iri,
        "https://contreforts.ds-labs.org/data/mapping/gitlab/acme%20corp/main%2Feu/team%20a%2Fb"
    );
}
