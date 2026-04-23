use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// The kinds of entities that adapters can sync.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityKind {
    Customer,
    Contact,
    Invoice,
    Company,
    CustomerGroup,
    Territory,
    Project,
    Issue,
}

/// Returned by `pennylane_path()` for entity kinds Pennylane does not expose.
/// Adapters should short-circuit on this sentinel with an explicit error.
pub const PENNYLANE_UNSUPPORTED: &str = "__unsupported__";

impl EntityKind {
    /// Canonical doctype label — also the ERPNext doctype name where applicable.
    /// Used by the graph indexer as the RDF class label and subject-IRI segment.
    pub fn erpnext_doctype(&self) -> &str {
        match self {
            Self::Customer => "Customer",
            Self::Contact => "Contact",
            Self::Invoice => "Sales Invoice",
            Self::Company => "Company",
            Self::CustomerGroup => "Customer Group",
            Self::Territory => "Territory",
            Self::Project => "Project",
            Self::Issue => "Issue",
        }
    }

    /// Pennylane API path segment for this entity, or the `PENNYLANE_UNSUPPORTED`
    /// sentinel for kinds that have no Pennylane analogue.
    pub fn pennylane_path(&self) -> &str {
        match self {
            Self::Customer => "customers",
            Self::Contact => "contacts",
            Self::Invoice => "customer_invoices",
            Self::Company => "companies",
            Self::CustomerGroup => "customer_groups",
            Self::Territory => "territories",
            Self::Project | Self::Issue => PENNYLANE_UNSUPPORTED,
        }
    }

    /// Lowercase string identifier.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Customer => "customer",
            Self::Contact => "contact",
            Self::Invoice => "invoice",
            Self::Company => "company",
            Self::CustomerGroup => "customer-group",
            Self::Territory => "territory",
            Self::Project => "project",
            Self::Issue => "issue",
        }
    }
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for EntityKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "customer" => Ok(Self::Customer),
            "contact" => Ok(Self::Contact),
            "invoice" => Ok(Self::Invoice),
            "company" => Ok(Self::Company),
            "customer-group" => Ok(Self::CustomerGroup),
            "territory" => Ok(Self::Territory),
            "project" => Ok(Self::Project),
            "issue" => Ok(Self::Issue),
            _ => Err(format!("unknown entity kind: '{s}' (expected customer|contact|invoice|company|customer-group|territory|project|issue)")),
        }
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

    #[test]
    fn entity_kind_as_str_roundtrip() {
        for kind in [
            EntityKind::Customer,
            EntityKind::Contact,
            EntityKind::Invoice,
            EntityKind::Company,
            EntityKind::CustomerGroup,
            EntityKind::Territory,
            EntityKind::Project,
            EntityKind::Issue,
        ] {
            let s = kind.as_str();
            let parsed = EntityKind::from_str(s).expect("parses back");
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn entity_kind_from_str_is_case_insensitive() {
        assert_eq!(EntityKind::from_str("Customer").unwrap(), EntityKind::Customer);
        assert_eq!(EntityKind::from_str("INVOICE").unwrap(), EntityKind::Invoice);
        assert_eq!(EntityKind::from_str("customer-group").unwrap(), EntityKind::CustomerGroup);
    }

    #[test]
    fn entity_kind_from_str_rejects_unknown() {
        let err = EntityKind::from_str("unknown-thing").unwrap_err();
        assert!(err.contains("unknown entity kind"));
    }

    #[test]
    fn entity_kind_adapter_paths() {
        assert_eq!(EntityKind::Invoice.erpnext_doctype(), "Sales Invoice");
        assert_eq!(EntityKind::Invoice.pennylane_path(), "customer_invoices");
        assert_eq!(EntityKind::CustomerGroup.erpnext_doctype(), "Customer Group");
        assert_eq!(EntityKind::CustomerGroup.pennylane_path(), "customer_groups");
        assert_eq!(EntityKind::Project.erpnext_doctype(), "Project");
        assert_eq!(EntityKind::Issue.erpnext_doctype(), "Issue");
        assert_eq!(EntityKind::Project.pennylane_path(), PENNYLANE_UNSUPPORTED);
        assert_eq!(EntityKind::Issue.pennylane_path(), PENNYLANE_UNSUPPORTED);
    }

    #[test]
    fn entity_kind_display_matches_as_str() {
        assert_eq!(EntityKind::Customer.to_string(), "customer");
        assert_eq!(EntityKind::CustomerGroup.to_string(), "customer-group");
    }

    #[test]
    fn document_serde_roundtrip() {
        let doc = Document {
            name: "Acme".into(),
            remote_id: "CUST-001".into(),
            kind: EntityKind::Customer,
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
}
