//! contreforts/contreforts-kg#30 -- the `contreforts:entityKind` duplicate-value lint.
//! Mechanically identical to `lint::config_field`'s rule 1 (see that module's own doc comment):
//! a SHACL constraint is universally quantified over its resolved target set, so an absence of
//! any offending value conforms vacuously -- this is a rule about *other* subjects elsewhere in
//! the same declaration, which no `sh:property` constraint (or any other single-focus-node
//! SHACL shape) can see. Enforced here as a Rust lint instead, for the same reason.
//!
//!   No two `rdfs:Class` subjects under one connector's own namespace may name the same
//!   `contreforts:entityKind` value -- two declared classes both claiming to be the same kind
//!   of entity is ambiguous ("which class does a synced document's `rdf:type` resolve to?"),
//!   and `contreforts-kg`'s own `EntityDeclarations::resolve_class_iri`
//!   (`crates/contreforts-kg/src/entity_declarations.rs`) has exactly this defect today: its
//!   `resolved.insert((source, kind_value), ...)` is a plain `HashMap::insert`, so the second
//!   class parsed silently overwrites the first and the winner depends on Turtle iteration
//!   order. This lint is what makes that fact pattern unreachable from a declaration that
//!   passes `validate`/`declarations` -- it does not change `contreforts-kg` itself, which is a
//!   different repo and out of this lint's scope.
//!
//! ## Scope: the class IRI's own namespace, not graph-wide
//!
//! "One connector's own declaration" cannot mean a node shape here (unlike `config_field`'s
//! rule 1, which groups by node shape): entity classes are not `sh:property` children of
//! anything, and there is no SHACL structure tying an `rdfs:Class` like `forgejo:Organization`
//! to its connector's own node shape (`forgejo:ForgejoConnectorShape`) at all -- confirmed by
//! reading both real declarations directly (`crates/contreforts-connector-forgejo/
//! declaration.ttl`, `crates/contreforts-connector-o365/declaration.ttl`): the entity classes
//! and the connector's config node shape are structurally separate subjects, related only by
//! sharing one connector-owned namespace.
//!
//! The namespace **is** the right grouping, and it must not be graph-wide, verified directly
//! (not merely inherited from a1's claim) against the one real production caller that unions
//! multiple connectors into one graph before validating it:
//! `crates/contreforts-config-api/src/routes/product_graph.rs:83` calls
//! `contreforts_declaration::declarations(contreforts_product::PRODUCT_GRAPH_TTL, ...)`, and
//! `declarations()` shares `validate.rs`'s `run_pipeline`, which runs every Part-3 lint
//! (including this one) against the whole unioned `declaration_graph` before ever splitting by
//! connector shape. A graph-wide duplicate check would therefore run against every enabled
//! connector's classes unioned, and would misfire the moment two *different* connectors
//! legitimately reuse the same `EntityKind::as_str()` value (e.g. two connectors both minting an
//! `EntityKind::CUSTOMER` document -- ordinary and correct, not a conflict). The other real
//! caller, `crates/contreforts-config-api/product/src/assemble.rs:204`, calls `validate()` once
//! per connector's own Turtle individually, before unioning -- so per-namespace scoping is
//! *also* exactly right there; it just degenerates to "the whole graph handed to that call",
//! since one call already covers one connector alone.
//!
//! `declaration/tests/fixtures/entity-kind-two-connectors-same-value.ttl` is the fixture that
//! pins this: two different connector namespaces, each with one class naming the same
//! `entityKind` value, must validate clean. `declaration/tests/fixtures/entity-kind-conflict.ttl`
//! is the opposite fixture: two classes under the SAME namespace naming the same value must be
//! rejected, naming both classes and the shared value.

use std::collections::BTreeMap;

use oxigraph::model::{Graph, NamedNodeRef, NamedOrBlankNodeRef, TermRef};

use crate::error::Violation;

const ENTITY_KIND_PREDICATE: NamedNodeRef<'static> = NamedNodeRef::new_unchecked(
    "https://contreforts.ds-labs.org/ontologies/declaration#entityKind",
);

/// "One connector's own namespace", for this lint's purposes: the class IRI's prefix up to and
/// including its last `#` or `/` -- matching every real declaration's own namespace convention
/// (both `forgejo-declaration.ttl` and `o365-declaration.ttl` mint their config fields, node
/// shape, and entity classes alike under one connector-owned namespace; see this module's own
/// doc comment above for why that, and not a node shape, is the right grouping).
fn namespace_of(class_iri: &str) -> &str {
    let cut = class_iri
        .rfind(['#', '/'])
        .map_or(class_iri.len(), |i| i + 1);
    &class_iri[..cut]
}

/// Runs the `contreforts:entityKind` duplicate-value lint over the whole declaration graph.
/// Graph-wide iteration, per-namespace grouping: every `(subject, "entityKind", value)` triple
/// in the graph is collected regardless of where it sits, then partitioned by the subject
/// class's own namespace before duplicates are compared -- see this module's own doc comment
/// for why grouping by namespace (not skipping the grouping entirely) is the one correctness
/// requirement here.
pub(crate) fn check(graph: &Graph) -> Vec<Violation> {
    // Collected into a Vec and sorted before comparison, not compared while walking
    // `graph.triples_for_predicate` directly: oxigraph's own triple iteration order is an
    // interning artifact, not a stable contract, and this lint's violation messages (which
    // name two specific classes) must not depend on it -- the exact class of bug D15's own
    // rule 1 exists to catch, applied here to this lint's own implementation.
    let mut entries: Vec<(String, String, String)> = Vec::new();
    for triple in graph.triples_for_predicate(ENTITY_KIND_PREDICATE) {
        let NamedOrBlankNodeRef::NamedNode(subject) = triple.subject else {
            continue;
        };
        let TermRef::Literal(value) = triple.object else {
            continue;
        };
        let class_iri = subject.as_str().to_string();
        let namespace = namespace_of(&class_iri).to_string();
        entries.push((namespace, value.value().to_string(), class_iri));
    }
    entries.sort();

    let mut violations = Vec::new();
    let mut seen: BTreeMap<(String, String), String> = BTreeMap::new();
    for (namespace, value, class_iri) in entries {
        let key = (namespace.clone(), value.clone());
        match seen.get(&key) {
            Some(previous_iri) => {
                violations.push(Violation::entity_kind(
                    Some(namespace.clone()),
                    format!(
                        "two rdfs:Class subjects under the same connector namespace ({namespace}) \
                         name the same contreforts:entityKind \"{value}\": <{previous_iri}> and \
                         <{class_iri}> -- ambiguous which class a synced document's rdf:type \
                         should resolve to. Give each class its own contreforts:entityKind, or \
                         remove the annotation from whichever one does not really type a synced \
                         document."
                    ),
                ));
            }
            None => {
                seen.insert(key, class_iri);
            }
        }
    }

    violations
}
