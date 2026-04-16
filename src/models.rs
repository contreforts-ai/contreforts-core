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
}

impl EntityKind {
    /// ERPNext doctype name for this entity.
    pub fn erpnext_doctype(&self) -> &str {
        match self {
            Self::Customer => "Customer",
            Self::Contact => "Contact",
            Self::Invoice => "Sales Invoice",
            Self::Company => "Company",
            Self::CustomerGroup => "Customer Group",
            Self::Territory => "Territory",
        }
    }

    /// Pennylane API path segment for this entity.
    pub fn pennylane_path(&self) -> &str {
        match self {
            Self::Customer => "customers",
            Self::Contact => "contacts",
            Self::Invoice => "customer_invoices",
            Self::Company => "companies",
            Self::CustomerGroup => "customer_groups",
            Self::Territory => "territories",
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
            _ => Err(format!("unknown entity kind: '{s}' (expected customer|contact|invoice|company|customer-group|territory)")),
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
