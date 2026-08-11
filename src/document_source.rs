//! A source of **documents**, as opposed to a connector to a business system.
//!
//! contreforts-core#4. The document corpus being ported — 7 912 files, 173 930 chunks — needs an
//! ingestion abstraction, and [`crate::traits::ContrefortsConnector`] is the wrong one on three
//! counts: it assumes a writable remote (`push`, `update`) where a filesystem corpus behind an
//! NFS mount is read-only by policy; [`crate::models::EntityKind`]'s business terms contain no
//! document, and adding one would make a document an entity everywhere the kind is matched, for
//! the benefit of a single connector; and `fetch_content` answers "give me the text attached to
//! this entity", not "walk this tree and tell me what changed".
//!
//! Implementing it there would produce an adapter honouring three of six methods. This trait is
//! smaller and says what it means.
//!
//! ## Scope
//!
//! The trait and its types, nothing else. The filesystem implementation is `contreforts-source-fs`
//! and the ingestion loop is `contreforts-rag`; both need this to compile against, which is why it
//! lives in core.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// What a source knows about one document **without opening it**.
///
/// Everything here is cheap to obtain — a `stat` and a digest — because the point of the type is
/// to decide whether opening the document is necessary at all. That question is answered against
/// `contreforts-rag`'s delta manifest, and answering it wrongly costs the whole extraction.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceRef {
    /// Stable identity, and the deletion key downstream.
    ///
    /// Load-bearing in both directions: the manifest is keyed on it, and it is what a prune
    /// removes. A source that renumbers or re-cases these strings between runs makes every
    /// document look deleted and re-added.
    pub path: String,
    /// Digest of the **source bytes**, not of the extracted text.
    pub sha256: String,
    pub mtime: DateTime<Utc>,
    pub size_bytes: u64,
    /// The extractor version this document was *last known* to have been read with.
    ///
    /// See [`Extraction::parser_version`] for why this appears twice and why collapsing the two
    /// re-extracts the entire PDF corpus on every pass.
    pub parser_version: String,
}

/// One labelled part of an extracted document.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// `page-12`, `sheet-Budget`, a heading title — or `None` where the format has no such
    /// structure.
    ///
    /// Kept as a distinct field rather than folded into the text because RAG **citations** are
    /// built from it. Flattening an extraction to one `String` trades a reader's ability to find
    /// the passage for a tidier type, and the corpus is exactly the kind where that matters: a
    /// 2 400-chunk PDF cited as "the PDF" is not a citation.
    pub label: Option<String>,
    pub text: String,
}

impl Section {
    pub fn new(label: Option<String>, text: impl Into<String>) -> Self {
        Self {
            label,
            text: text.into(),
        }
    }

    /// A section from a format with no internal structure to name.
    pub fn unlabelled(text: impl Into<String>) -> Self {
        Self::new(None, text)
    }
}

/// The result of opening and reading one document.
#[derive(Clone, Debug, PartialEq)]
pub struct Extraction {
    pub sections: Vec<Section>,
    /// The extractor version that produced **these sections**, which is not necessarily the one
    /// on the [`SourceRef`].
    ///
    /// The two differ exactly when it matters. The production loop computes the version twice,
    /// before and after extraction, because a document that was just extracted has populated the
    /// extraction cache and must be recorded at the *new* version. Carrying one value instead
    /// means every PDF compares unequal on the following pass and is re-extracted — the failure
    /// is invisible, because the run succeeds; it just never converges.
    pub parser_version: String,
}

impl Extraction {
    /// Total characters across all sections. Characters, not bytes — see
    /// `contreforts-rag::chunk`.
    pub fn char_count(&self) -> usize {
        self.sections.iter().map(|s| s.text.chars().count()).sum()
    }

    /// Whether anything was extracted at all.
    ///
    /// A document with sections that are all empty is empty: the distinction between "no
    /// sections" and "sections holding nothing" is an artefact of the extractor, never of the
    /// document, and a caller that treated them differently would index blank chunks from one
    /// and not the other.
    pub fn is_empty(&self) -> bool {
        self.sections.iter().all(|s| s.text.trim().is_empty())
    }
}

/// Why a document could not be read.
///
/// The split between the two variants is the whole point of the type, and it comes from an
/// incident rather than from taste.
#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    /// This document cannot be read, and retrying will not change that.
    ///
    /// A corrupt file, an unsupported format, a password-protected archive. The caller may record
    /// the failure and move on.
    #[error("cannot extract {path}: {message}")]
    Failed { path: String, message: String },

    /// Extraction did not happen **for a reason unrelated to this document** — GPU saturation, a
    /// killed subprocess, a session budget overrun.
    ///
    /// The caller must write **nothing** to its delta state and retry on a later pass. Recording
    /// it as a document-level failure, or indexing whatever degraded fallback the extractor
    /// produced under pressure, freezes that document at lower quality silently and durably: the
    /// manifest then says it was indexed, so no future run reopens it. That is the failure this
    /// variant exists to make unrepresentable, and it is why a single error type would not do.
    #[error("extraction of {path} deferred: {message}")]
    Deferred { path: String, message: String },
}

impl ExtractError {
    /// Whether the caller should leave its delta state untouched and try again later.
    pub fn is_retryable(&self) -> bool {
        matches!(self, ExtractError::Deferred { .. })
    }

    /// The document this error is about.
    pub fn path(&self) -> &str {
        match self {
            ExtractError::Failed { path, .. } | ExtractError::Deferred { path, .. } => path,
        }
    }
}

/// A corpus of documents that can be walked and read.
///
/// Read-only by construction: there is no `push`, and there is no plan for one. A source that
/// needed to write would be a [`crate::traits::ContrefortsConnector`].
#[async_trait]
pub trait DocumentSource: Send + Sync {
    /// Stable name for this source, recorded on every chunk it produces.
    fn source_name(&self) -> &str;

    /// Every document this source currently holds, with enough metadata to decide whether it
    /// needs reading.
    ///
    /// Returning an **empty** list from a source that should hold documents is indistinguishable
    /// from a source that legitimately holds none, and downstream that reads as "every document
    /// was deleted". Implementations that walk a mount must therefore fail rather than return
    /// empty when a root is unreachable — the deletion-ratio guard in `contreforts-rag::manifest`
    /// exists because this exact mistake was made once already.
    async fn discover(&self) -> Result<Vec<SourceRef>, ExtractError>;

    /// Open one document and return its labelled sections.
    async fn extract(&self, source: &SourceRef) -> Result<Extraction, ExtractError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_ref(path: &str) -> SourceRef {
        SourceRef {
            path: path.into(),
            sha256: "abc".into(),
            mtime: DateTime::from_timestamp(1_774_889_491, 0).unwrap(),
            size_bytes: 42,
            parser_version: "texte-1".into(),
        }
    }

    struct Fake;

    #[async_trait]
    impl DocumentSource for Fake {
        fn source_name(&self) -> &str {
            "fake"
        }
        async fn discover(&self) -> Result<Vec<SourceRef>, ExtractError> {
            Ok(vec![a_ref("/a.pdf")])
        }
        async fn extract(&self, source: &SourceRef) -> Result<Extraction, ExtractError> {
            if source.path.ends_with(".busy") {
                return Err(ExtractError::Deferred {
                    path: source.path.clone(),
                    message: "GPU saturated".into(),
                });
            }
            Ok(Extraction {
                sections: vec![Section::new(Some("page-1".into()), "hello")],
                parser_version: "texte-2".into(),
            })
        }
    }

    /// The distinction the type exists for: a deferred extraction is retryable, a failed one is
    /// not.
    ///
    /// If these collapsed, the caller would record a document as indexed after an extraction that
    /// never ran — and the manifest entry means no future pass reopens it. Silent, durable, and
    /// invisible in a green run.
    #[test]
    fn deferred_is_retryable_and_failed_is_not() {
        let deferred = ExtractError::Deferred {
            path: "/a.pdf".into(),
            message: "budget exhausted".into(),
        };
        let failed = ExtractError::Failed {
            path: "/b.pdf".into(),
            message: "not a PDF".into(),
        };
        assert!(deferred.is_retryable());
        assert!(!failed.is_retryable());
        assert_eq!(deferred.path(), "/a.pdf");
        assert_eq!(failed.path(), "/b.pdf");
    }

    /// Both messages must name the document. An extraction log that says "deferred" without
    /// saying which file is unactionable at 8 000 documents.
    #[test]
    fn both_errors_name_the_document_in_their_message() {
        for e in [
            ExtractError::Deferred {
                path: "/some/deep/path.pdf".into(),
                message: "m".into(),
            },
            ExtractError::Failed {
                path: "/some/deep/path.pdf".into(),
                message: "m".into(),
            },
        ] {
            assert!(e.to_string().contains("/some/deep/path.pdf"), "{e}");
        }
    }

    /// `parser_version` is carried independently on the two types, and the fake proves the shapes
    /// can genuinely disagree.
    ///
    /// A single shared value would make this assertion unwritable, which is the point: the loop
    /// computes it before and after extraction, and a freshly extracted document must be recorded
    /// at the *new* version or it is re-extracted on every subsequent pass.
    #[tokio::test]
    async fn the_extraction_may_report_a_newer_parser_version_than_the_ref() {
        let source = Fake;
        let refs = source.discover().await.unwrap();
        let extracted = source.extract(&refs[0]).await.unwrap();
        assert_eq!(refs[0].parser_version, "texte-1");
        assert_eq!(extracted.parser_version, "texte-2");
    }

    #[tokio::test]
    async fn a_transient_failure_surfaces_as_deferred() {
        let err = Fake.extract(&a_ref("/x.busy")).await.unwrap_err();
        assert!(
            err.is_retryable(),
            "GPU saturation is not this document's fault and must not be recorded as its failure"
        );
    }

    /// Sections keep their labels, because citations are built from them.
    #[test]
    fn a_section_label_is_optional_but_preserved() {
        let e = Extraction {
            sections: vec![
                Section::new(Some("page-12".into()), "body"),
                Section::unlabelled("tail"),
            ],
            parser_version: "texte-1".into(),
        };
        assert_eq!(e.sections[0].label.as_deref(), Some("page-12"));
        assert_eq!(e.sections[1].label, None);
        assert_eq!(e.char_count(), 8);
        assert!(!e.is_empty());
    }

    /// No sections and blank sections are one state.
    ///
    /// The difference is an artefact of which extractor ran, never of the document, and a caller
    /// that distinguished them would index blank chunks from one shape and not the other.
    #[test]
    fn an_extraction_of_only_blank_sections_is_empty() {
        let blank = Extraction {
            sections: vec![Section::unlabelled("   \n\t "), Section::unlabelled("")],
            parser_version: "texte-1".into(),
        };
        let none = Extraction {
            sections: vec![],
            parser_version: "texte-1".into(),
        };
        assert!(blank.is_empty());
        assert!(none.is_empty());
        assert_eq!(blank.char_count(), 6);
    }

    /// `char_count` counts characters, not bytes — the trap `contreforts-rag::chunk` is built
    /// against, restated here because this is where the number originates.
    #[test]
    fn char_count_counts_characters_not_bytes() {
        let e = Extraction {
            sections: vec![Section::unlabelled("Éléphant")],
            parser_version: "texte-1".into(),
        };
        assert_eq!(e.char_count(), 8);
        assert_eq!("Éléphant".len(), 10);
    }
}
