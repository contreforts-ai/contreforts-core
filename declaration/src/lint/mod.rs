//! The declaration-graph analyses that are not, or cannot be, expressed as
//! meta-shapes.ttl SHACL constraints. See each submodule's own doc comment
//! for why it lives here instead of in SHACL:
//!
//!   - [`xone`] -- D14, the reason this crate exists: SHACL cannot express
//!     a constraint about *other* alternatives' fields.
//!   - [`core_ns`] -- D2's core:-namespace alignment-stub rule: a
//!     graph-wide check, implemented in Rust for reliability rather than
//!     shacl-rust 0.2.9's less-proven SPARQL-target surface.
//!   - [`structural`] -- existential ("at least one X exists") checks,
//!     which a universally-quantified SHACL constraint cannot express at
//!     all.
//!   - [`config_field`] -- D15 (contreforts/contreforts-core#16): no two
//!     property shapes on one node shape may share a
//!     `contreforts:configField`, and (narrowed -- see that module's own
//!     doc comment for a contradiction found in the issue text) a
//!     `contreforts:secret true` property shape must carry one too, once
//!     its node shape uses the term at all.
//!   - [`entity_kind`] -- contreforts/contreforts-kg#30: no two
//!     `rdfs:Class` subjects under one connector's own namespace may name
//!     the same `contreforts:entityKind` value -- graph-wide iteration,
//!     scoped by namespace rather than by node shape (entity classes have
//!     no SHACL structure tying them to one), and deliberately NOT
//!     graph-wide overall; see that module's own doc comment for the real
//!     production caller (`contreforts_declaration::declarations` over
//!     `contreforts-product`'s unioned graph) that would false-positive
//!     under a graph-wide scope.

pub(crate) mod config_field;
pub(crate) mod core_ns;
pub(crate) mod entity_kind;
pub(crate) mod structural;
pub(crate) mod xone;
