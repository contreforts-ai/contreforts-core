//! Violations: the crate's one error currency. Both real callers described
//! in contreforts/contreforts-core#14 Part 4 -- a connector's `build.rs`
//! (one declaration, wants a legible `panic!`) and the aggregator's
//! `build.rs` (C3 -- many declarations, wants every failure reported, not
//! just the first) -- work off the same `Vec<Violation>`.

use std::fmt;

/// Which stage of `validate()` produced a [`Violation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// The input was not parseable Turtle at all.
    Turtle,
    /// A `meta-shapes.ttl` SHACL violation (Part 2).
    MetaShape,
    /// The D14 `sh:xone` cross-exclusion lint (Part 3).
    D14Xone,
    /// The D2 `core:`-namespace alignment-stub lint.
    D2CoreNamespace,
    /// A graph-wide structural check that is not expressible as a
    /// per-node SHACL constraint (e.g. "at least one sh:NodeShape carries
    /// sh:targetClass").
    Structural,
    /// The D15 `contreforts:configField` existential lints (Part 3 of
    /// contreforts/contreforts-core#16): no two property shapes on one
    /// node shape may share a `configField`, and (narrowed -- see
    /// `lint::config_field`'s doc comment) a `contreforts:secret true`
    /// property shape on a node shape that already uses `configField`
    /// elsewhere must carry one too.
    D15ConfigField,
    /// The `contreforts:entityKind` duplicate-value lint
    /// (contreforts/contreforts-kg#30): no two `rdfs:Class` subjects under
    /// one connector's own namespace may name the same `entityKind` value
    /// -- see `lint::entity_kind`'s own doc comment for why the scope is
    /// per-namespace, not graph-wide.
    EntityKind,
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Rule::Turtle => "turtle",
            Rule::MetaShape => "meta-shape",
            Rule::D14Xone => "D14 (sh:xone cross-exclusion)",
            Rule::D2CoreNamespace => "D2 (core: namespace)",
            Rule::Structural => "structural",
            Rule::D15ConfigField => "D15 (contreforts:configField)",
            Rule::EntityKind => "entityKind (contreforts:entityKind)",
        };
        write!(f, "{s}")
    }
}

/// One thing wrong with a declaration.
///
/// `Display` renders a single self-contained line (or short paragraph) a
/// connector author can act on without reading contreforts-core#14: it
/// always names what rule fired, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    rule: Rule,
    /// The node the violation is about, when there is one to name -- a
    /// shape's IRI/blank-node id, or a focus node from a SHACL result.
    subject: Option<String>,
    message: String,
}

impl Violation {
    pub fn new(rule: Rule, subject: Option<String>, message: impl Into<String>) -> Self {
        Self {
            rule,
            subject,
            message: message.into(),
        }
    }

    pub fn turtle(message: impl Into<String>) -> Self {
        Self::new(Rule::Turtle, None, message)
    }

    pub fn meta_shape(subject: Option<String>, message: impl Into<String>) -> Self {
        Self::new(Rule::MetaShape, subject, message)
    }

    pub fn d14_xone(subject: Option<String>, message: impl Into<String>) -> Self {
        Self::new(Rule::D14Xone, subject, message)
    }

    pub fn d2_core_namespace(subject: Option<String>, message: impl Into<String>) -> Self {
        Self::new(Rule::D2CoreNamespace, subject, message)
    }

    pub fn structural(message: impl Into<String>) -> Self {
        Self::new(Rule::Structural, None, message)
    }

    pub fn d15_config_field(subject: Option<String>, message: impl Into<String>) -> Self {
        Self::new(Rule::D15ConfigField, subject, message)
    }

    pub fn entity_kind(subject: Option<String>, message: impl Into<String>) -> Self {
        Self::new(Rule::EntityKind, subject, message)
    }

    pub fn rule(&self) -> Rule {
        self.rule
    }

    pub fn subject(&self) -> Option<&str> {
        self.subject.as_deref()
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.subject {
            Some(subject) => write!(f, "[{}] {}: {}", self.rule, subject, self.message),
            None => write!(f, "[{}] {}", self.rule, self.message),
        }
    }
}

impl std::error::Error for Violation {}

/// A non-empty set of [`Violation`]s, returned by [`crate::validate`] on
/// failure. `Display` renders every violation, one per line, prefixed with
/// a count -- something `panic!("{violations}")` can print directly and a
/// human can act on, per Part 4's requirement, without every caller having
/// to write its own `.iter().map(...).join(...)` formatting first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violations(Vec<Violation>);

impl Violations {
    pub(crate) fn new(violations: Vec<Violation>) -> Self {
        debug_assert!(
            !violations.is_empty(),
            "Violations must not be constructed empty"
        );
        Self(violations)
    }

    pub fn as_slice(&self) -> &[Violation] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<Violation> {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Violation> {
        self.0.iter()
    }
}

impl IntoIterator for Violations {
    type Item = Violation;
    type IntoIter = std::vec::IntoIter<Violation>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Violations {
    type Item = &'a Violation;
    type IntoIter = std::slice::Iter<'a, Violation>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl fmt::Display for Violations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "declaration is invalid: {} violation(s)", self.0.len())?;
        for (i, v) in self.0.iter().enumerate() {
            writeln!(f, "  {}. {v}", i + 1)?;
        }
        Ok(())
    }
}

impl std::error::Error for Violations {}

impl From<Vec<Violation>> for Violations {
    fn from(v: Vec<Violation>) -> Self {
        Self::new(v)
    }
}
