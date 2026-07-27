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

pub(crate) mod core_ns;
pub(crate) mod structural;
pub(crate) mod xone;
