//! # contreforts-declaration
//!
//! The declaration vocabulary, its meta-shapes, and the D14 `sh:xone`
//! cross-exclusion lint -- contreforts/contreforts-core#14, phase C item
//! C1 of the bootstrap epic (contreforts/contreforts-workspace#20).
//!
//! A connector describes itself in Turtle
//! (contreforts-connector-forgejo#9 is the pilot). This crate defines what
//! a valid self-description *is*, and checks it at build time:
//! [`validate`] parses a declaration, runs it against `meta-shapes.ttl`
//! (SHACL), and runs the D14/D2/structural lints that SHACL cannot express
//! as constraints on itself.
//!
//! ## This nesting is provisional
//!
//! `contreforts-declaration` is a separate crate, but it currently lives
//! *nested inside* `contreforts-core`, at `declaration/` -- so the
//! workspace member path is `crates/contreforts-core/declaration`. Every
//! other crate in this org is one-crate-per-repo; this one breaks that
//! deliberately, to avoid standing up a new repository before the
//! mechanism has proven itself. It is expected to be replaced by, or
//! extended into, its own repository once phase C is complete and the
//! shape of this crate has stopped moving.
//!
//! Two things are true so that promotion is a `git mv` plus one
//! `Cargo.toml` line, not a rewrite:
//!   - This crate does not depend on `contreforts-core` -- no `path =
//!     "../"`, no use of anything core exports. `contreforts-core`'s own
//!     manifest only gains `exclude = ["declaration"]`.
//!   - Nothing about this crate's own manifest assumes it is a member of
//!     the outer `contreforts-workspace` superproject it happens to sit
//!     inside today (see `Cargo.toml`'s dependency comments).
//!
//! See contreforts/contreforts-core#14 for the full design discussion; do
//! not mistake this nesting for a considered end state.
//!
//! ## Why not inside `contreforts-core` itself
//!
//! A `build.rs` cannot use its own crate's normal dependencies -- only
//! `[build-dependencies]`. Every connector's build script needs this code,
//! so it has to be a separate crate they can build-depend on. Folding it
//! into `contreforts-core` would instead force `oxigraph` and a SHACL
//! validator into the one crate every other crate in the workspace
//! depends on.
//!
//! ## What is explicitly out of scope here
//!
//! No Turtle-to-Rust emitter (phase D, once `contreforts-kg` is
//! generalised), no aggregation/product-graph/collision-detection (C3),
//! and no connector wiring (C7) -- this crate is usable by a connector's
//! `build.rs`, but nothing here calls it from one yet.
//!
//! ## The form-schema digest
//!
//! [`form_schemas`] is the other consumer of [`Declaration`]: the **total**
//! function that turns a validated declaration into the renderable
//! [`FormSchema`] the config UI is generated from
//! (contreforts/contreforts-core#34, phase F item W4). "Total" is the whole
//! point -- a SHACL construct it does not understand becomes a named
//! [`DigestError`], never a silently missing form field. See
//! `form_schema.rs`'s own module documentation.
//!
//! ## Server-side connector-instance validation
//!
//! [`connector_validation`] holds the *other* half of this crate's SHACL usage: not "is this
//! declaration itself well-formed" (the rest of this crate), but "does a connector instance,
//! about to be written to a config store, conform to a declared connector's shape" --
//! relocated here from `contreforts-kg::connector_validation` by
//! contreforts/contreforts-workspace#58 (comment 7791), item D3a, since both `contreforts-kg`
//! and the future `contreforts-config` (D3c) need it and neither should have to depend on the
//! other to reach it. See that module's own docs for the case-1/case-2 undeclared-kind policy.
//!
//! ## Create versus update
//!
//! [`ConnectorValidator::validate`] takes a [`WriteIntent`] and validates against the shapes a
//! declaration scoped to that verb (contreforts/contreforts-workspace#19, "D8 amended --
//! 2026-08-11"): a required secret is required at create and absent-means-unchanged at update,
//! and one SHACL shape cannot state both, so a connector declares two. The term that scopes them
//! is `contreforts:writeIntent`; the rule stays in the connector author's Turtle rather than in
//! contreforts' write path, because a third party writing a connector must be able to state it
//! for their own fields. [`WriteIntent`] has no default -- see its own doc comment.

mod connector_validation;
mod error;
mod form_schema;
mod lint;
mod model;
mod validate;
mod write_intent;

pub use connector_validation::{
    ConnectorDeclarations, ConnectorIris, ConnectorValidationOutcome, ConnectorValidator,
    ConnectorValidatorError, ConnectorViolation,
};
pub use error::{Rule, Violation, Violations};
pub use form_schema::{
    Control, DigestError, FieldDescriptor, FormSchema, GroupSection, KNOWN_UI_SHAPES, SelectOption,
    VariantRule, form_schemas,
};
pub use model::{Declaration, DeclarationVariant, GroupDescriptor, PropertyShape};
pub use validate::{declarations, validate};
pub use write_intent::{IntentScope, WriteIntent};

/// The declaration vocabulary (Part 1): six terms in
/// `https://contreforts.ds-labs.org/ontologies/declaration#` --
/// `contreforts:secret`, `contreforts:category`, `contreforts:uiShape`,
/// `contreforts:configField` (the fourth, added by
/// contreforts/contreforts-core#16, D15), `contreforts:entityKind`
/// (the fifth, added by contreforts/contreforts-kg#30: names the
/// `EntityKind::as_str()` value an `rdfs:Class` is minted for, the same
/// "explicit opt-in, absence is meaningful" rule D15 already established),
/// and `contreforts:writeIntent` (the sixth, added by
/// contreforts/contreforts-workspace#19's amended D8: scopes a node shape to
/// the create or the update verb, so "this secret is required at create" is
/// something the declaration states rather than something contreforts'
/// write path hard-codes for other people's fields -- see [`WriteIntent`]).
/// Embedded so a caller can inspect or re-serve it without a filesystem
/// dependency on this crate's source layout.
pub const VOCABULARY_TTL: &str = include_str!("vocabulary.ttl");

/// The meta-shapes (Part 2): the SHACL shapes a connector declaration's
/// own graph must satisfy. [`validate`] runs these internally; exposed
/// here mainly so a caller (or this crate's own tests) can inspect exactly
/// what ran.
pub const META_SHAPES_TTL: &str = include_str!("meta_shapes.ttl");

/// Core's own canonical SKOS concept scheme (contreforts/contreforts-workspace#83, phase E /
/// E2): `core:CoreConcepts`, a `skos:ConceptScheme`, plus exactly one `skos:Concept` per one of
/// `EntityKind`'s 15 associated constants (core's own `models.rs`, unreachable from this crate --
/// see this file's own module doc comment on the deliberate non-dependency between
/// `contreforts-declaration` and `contreforts-core`). [`validate`] unions this into the Part 2
/// meta-shapes validation graph internally, exactly as [`META_SHAPES_TTL`] is -- never into the
/// plain declaration graph the D2 (`lint::core_ns`) and entityKind (`lint::entity_kind`) lints
/// see, or core's own 15 concepts would look like a connector defining them. Exposed here so a
/// caller (or this crate's own tests, and `contreforts-kg`'s cross-crate coupling test) can
/// inspect exactly what it contains.
///
/// Authoritative for core's own 15 terms, not exhaustive of every legal
/// `contreforts:entityKind` value -- `EntityKind` is an open newtype
/// (contreforts-core#18); a connector minting a term this scheme has no concept for is legal,
/// not a violation.
pub const CONCEPTS_TTL: &str = include_str!("concepts.ttl");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_defines_exactly_the_six_terms() {
        for term in [
            "secret",
            "category",
            "uiShape",
            "configField",
            "entityKind",
            "writeIntent",
        ] {
            assert!(
                VOCABULARY_TTL.contains(&format!("contreforts:{term} a rdf:Property")),
                "vocabulary.ttl should define contreforts:{term}"
            );
        }
    }
}
