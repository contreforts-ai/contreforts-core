use std::borrow::Cow;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

/// The kind of entity a connector is asked to sync — an open, string-backed token.
///
/// `EntityKind` used to be a closed enum of core's 15 business terms, which meant a
/// connector could never introduce a new entity type without a PR against this crate
/// and an N+1 cascade through every consumer. It is now an open newtype: any term, from
/// any connector, constructs one via [`EntityKind::new`]. Core's own 15 terms remain
/// available as associated constants (below) — under
/// `contreforts/contreforts-workspace#19` D2 they are `skos:Concept` terms in core's own
/// alignment scheme, not a closed list of the only kinds that may exist. Naming its own
/// scheme is core's job; it is no longer the *only* way to name a kind.
///
/// See `contreforts/contreforts-core#11` for the full design record and
/// `contreforts/contreforts-core#18` for this implementation.
///
/// # Fallback policy for kinds a connector does not handle
///
/// Losing the closed enum also loses compiler-enforced exhaustiveness: nothing stops a
/// `match kind { .. }` in a connector from silently falling through. That fallback
/// behaviour is decided once, here, rather than independently — and inconsistently — by
/// each connector crate:
///
/// - **A connector** asked for a kind it does not handle (in `pull`, `get` or
///   `fetch_content`) must return [`crate::error::ConnectorError::UnsupportedKind`],
///   naming both the kind and the connector. It must **never** silently return an empty
///   result or an empty collection — that is exactly the failure this design exists to
///   eliminate. See `ConnectorError::UnsupportedKind` for the shared error shape every
///   connector should use.
/// - **The indexer and reconciler** (`contreforts-kg`) treat an unrecognised term as an
///   **error**, not a skip. A kind that reaches IRI construction is, by definition, one
///   the system intends to store, so silently omitting it there would produce a
///   malformed IRI or a missing `rdf:type` rather than a clean failure.
/// - There is no longer a "kind the graph holds but the type can't represent" case:
///   `EntityKind` can carry any term losslessly. Code that used to treat
///   `parse::<EntityKind>()` failure as "unknown kind, skip with a warning" (e.g.
///   `contreforts-rag/src/content.rs:192`) has nothing left to fail on — that fallback
///   path should be deleted, not preserved, when this lands downstream.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityKind(Cow<'static, str>);

impl EntityKind {
    // Core's own 15 terms, kept as associated constants so construction of a known kind
    // stays a compile-checked, greppable identifier rather than a string literal that
    // can typo silently (`EntityKind::new("Invoce")` would compile and run).
    //
    // ## The string values are the old `as_str()` forms, deliberately
    //
    // Do not "tidy" these to CamelCase. The enum this replaces had **two** strings per
    // variant, and they went to different durable places:
    //
    //   - `as_str()` -> `"invoice"`, `"calendar-event"` — written into vector-store chunk
    //     metadata by `contreforts-rag/src/content.rs` (`metadata["kind"]`) and read back
    //     on the refresh path;
    //   - `erpnext_doctype()` -> `"Sales Invoice"`, `"Calendar Event"` — written into RDF
    //     as `doctype_iri(..)`, i.e. `core:Sales%20Invoice`, on every synced entity.
    //
    // One constant cannot preserve both. Keeping the `as_str()` form was chosen for two
    // reasons (contreforts/contreforts-core#18, decided 2026-07-29):
    //
    //   1. It keeps the serde representation identical. `#[serde(transparent)]` over these
    //      values reproduces the old kebab-case wire format exactly, so anything that
    //      round-tripped an `EntityKind` through JSON still does.
    //   2. `"Sales Invoice"` in a `core:` IRI is ERPNext's doctype vocabulary leaking into
    //      the shared ontology — precisely the debt contreforts/contreforts-core#12 exists
    //      to remove. Dropping it here removes it rather than re-blessing it.
    //
    // The consequence, which is real and accepted: **stored `rdf:type` and subject IRIs
    // change** from `core:Sales%20Invoice` to `core:invoice`. Entity RDF is re-derivable by
    // re-sync — unlike embeddings, which are not — so this is paid by a re-sync rather than
    // a hand-written migration, and it belongs to phase E's S3/S4 row.
    pub const CUSTOMER: Self = Self(Cow::Borrowed("customer"));
    pub const CONTACT: Self = Self(Cow::Borrowed("contact"));
    pub const INVOICE: Self = Self(Cow::Borrowed("invoice"));
    pub const COMPANY: Self = Self(Cow::Borrowed("company"));
    pub const CUSTOMER_GROUP: Self = Self(Cow::Borrowed("customer-group"));
    pub const TERRITORY: Self = Self(Cow::Borrowed("territory"));
    pub const PROJECT: Self = Self(Cow::Borrowed("project"));
    pub const ISSUE: Self = Self(Cow::Borrowed("issue"));
    pub const ITEM: Self = Self(Cow::Borrowed("item"));
    pub const QUOTATION: Self = Self(Cow::Borrowed("quotation"));
    pub const MEETING: Self = Self(Cow::Borrowed("meeting"));
    pub const RESOLUTION: Self = Self(Cow::Borrowed("resolution"));
    pub const CALENDAR: Self = Self(Cow::Borrowed("calendar"));
    pub const CALENDAR_EVENT: Self = Self(Cow::Borrowed("calendar-event"));
    pub const INTERACTION: Self = Self(Cow::Borrowed("interaction"));

    /// Construct an `EntityKind` for any term — core's own or an extension's.
    ///
    /// This is the escape hatch that keeps `EntityKind` open: a connector shipping a
    /// vocabulary core has never heard of (a mail message, a GRC risk scenario, …) names
    /// its own terms here and needs no change to this crate.
    pub fn new(term: impl Into<Cow<'static, str>>) -> Self {
        Self(term.into())
    }

    /// The term as a string. Stable and lossless: round-trips through
    /// [`EntityKind::new`] / [`std::str::FromStr`] for any input, not just core's 15.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for EntityKind {
    // Infallible: an open type has nothing to reject. Kept so call sites written against
    // the old `.parse::<EntityKind>()` continue to compile; the `Err` arm they used to
    // need is now unreachable rather than removed out from under them.
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s.to_string()))
    }
}

/// Generic document that can represent an entity from any adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Canonical name / human-readable identifier.
    pub name: String,
    /// ID assigned by the remote system.
    pub remote_id: String,
    /// The entity kind.
    pub kind: EntityKind,
    /// Source adapter name (e.g., "erpnext", "pennylane").
    pub source: String,
    /// When this document was last modified in the source system.
    pub modified: Option<NaiveDateTime>,
    /// All fields as a flat JSON object.
    pub fields: serde_json::Value,
}

/// A unit of authoritative content fetched from a remote system for RAG ingestion.
///
/// Distinct from `Document` (which models structured business data) — a `RagDocument`
/// carries free-form prose (issue body, comment text, doc snippet) tied back to the
/// originating entity IRI for graph-spread retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagDocument {
    /// Fully-qualified graph IRI of the entity this content belongs to.
    pub entity_iri: String,
    /// Adapter `source_name()` that produced this content.
    pub source: String,
    /// Entity kind (Issue, Customer, …).
    pub kind: EntityKind,
    /// Stable identifier for the structural part within the entity
    /// (e.g. `"body"`, `"comment[42]"`, `"metadata"`).
    pub structural_path: String,
    /// Raw text content. Empty strings are allowed (callers may emit a body chunk
    /// even when the issue body is empty so refresh detection still works).
    pub text: String,
    /// Optional canonical URL for citation (e.g. `https://forge/owner/repo/issues/7`).
    pub url: Option<String>,
    /// When this content was fetched from the remote system.
    pub fetched_at: DateTime<Utc>,
    /// Optional cache validator surfaced by the remote (HTTP ETag, last-modified, etc.).
    pub etag: Option<String>,
}

/// Metadata about a sync operation for state tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub kind: EntityKind,
    pub source: String,
    pub last_synced: Option<NaiveDateTime>,
    pub count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const CORE_TERMS: [EntityKind; 15] = [
        EntityKind::CUSTOMER,
        EntityKind::CONTACT,
        EntityKind::INVOICE,
        EntityKind::COMPANY,
        EntityKind::CUSTOMER_GROUP,
        EntityKind::TERRITORY,
        EntityKind::PROJECT,
        EntityKind::ISSUE,
        EntityKind::ITEM,
        EntityKind::QUOTATION,
        EntityKind::MEETING,
        EntityKind::RESOLUTION,
        EntityKind::CALENDAR,
        EntityKind::CALENDAR_EVENT,
        EntityKind::INTERACTION,
    ];

    #[test]
    fn core_constants_round_trip_through_as_str_and_new() {
        for kind in CORE_TERMS {
            let s = kind.as_str().to_string();
            assert_eq!(EntityKind::new(s.clone()), kind);
            assert_eq!(EntityKind::new(s), kind, "as_str is stable across calls");
        }
    }

    #[test]
    fn a_term_core_has_never_heard_of_constructs_and_carries_its_own_string() {
        // A connector's own vocabulary term -- core has no constant for it, and should
        // not need one.
        let extension = EntityKind::new("RiskScenario");
        assert_eq!(extension.as_str(), "RiskScenario");
        // Usable wherever a constant is: equality, Display, cloning, hashing all work
        // identically to a core term.
        assert_eq!(extension, EntityKind::new("RiskScenario".to_string()));
        assert_ne!(extension, EntityKind::CUSTOMER);
        assert_eq!(extension.to_string(), "RiskScenario");
    }

    #[test]
    fn display_and_fromstr_are_lossless_for_arbitrary_terms() {
        for term in ["Customer", "RiskScenario", "mail-message", "Weird Term!"] {
            let kind = EntityKind::new(term.to_string());
            assert_eq!(kind.to_string(), term);

            let parsed = EntityKind::from_str(term).expect("open type never fails to parse");
            assert_eq!(parsed, kind);
            assert_eq!(parsed.as_str(), term);
        }
    }

    #[test]
    fn document_serde_roundtrip() {
        let doc = Document {
            name: "Acme".into(),
            remote_id: "CUST-001".into(),
            kind: EntityKind::CUSTOMER,
            source: "erpnext".into(),
            modified: None,
            fields: serde_json::json!({ "email": "x@y.z" }),
        };
        let json = serde_json::to_string(&doc).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, doc.name);
        assert_eq!(back.remote_id, doc.remote_id);
        assert_eq!(back.kind, doc.kind);
        assert_eq!(back.fields, doc.fields);
    }

    #[test]
    fn entity_kind_serializes_transparently_as_its_string() {
        // The wire representation is the term string itself, not a wrapper object --
        // required for contreforts-rag's round trip through the graph, where the kind is
        // stored as a bare string.
        // `"customer"`, not `"Customer"`: this is the *old* kebab-case wire format,
        // preserved on purpose so an EntityKind that round-tripped through JSON before
        // this refactor still does. See the constants' own comment for why.
        let json = serde_json::to_string(&EntityKind::CUSTOMER).unwrap();
        assert_eq!(json, "\"customer\"");
        assert_eq!(
            serde_json::to_string(&EntityKind::CALENDAR_EVENT).unwrap(),
            "\"calendar-event\"",
            "the hyphenated form is the one vector-store chunk metadata holds"
        );
        // A connector-shipped term is carried verbatim -- core does not case-fold it.
        let json = serde_json::to_string(&EntityKind::new("RiskScenario")).unwrap();
        assert_eq!(json, "\"RiskScenario\"");
        // And it round-trips.
        let back: EntityKind = serde_json::from_str("\"calendar-event\"").unwrap();
        assert_eq!(back, EntityKind::CALENDAR_EVENT);
    }
}
