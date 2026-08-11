//! D8's create-versus-update asymmetry (contreforts/contreforts-workspace#19,
//! "D8 amended -- 2026-08-11"): a required secret is required when a connector is
//! **created**, and absent-means-unchanged when it is **updated**.
//!
//! One SHACL shape cannot say both -- `sh:minCount 1` on `forgejo:token` either holds or it
//! does not. The decision is **two shapes per connector**: a create shape carrying
//! `sh:minCount 1` on its required secrets, and an update shape without it. This module owns
//! the two halves that makes possible: the term a declaration uses to say which of the two a
//! node shape is ([`WRITE_INTENT_PREDICATE`], read by [`scope_of`]), and the type a caller uses
//! to say which one it wants ([`WriteIntent`]).
//!
//! ## Why the rule is in the declaration and not in the write path
//!
//! The rejected alternative was one shape plus an out-of-band rule in the write path: "if this
//! is an update, skip `sh:minCount` on secret fields". It was rejected because this project is
//! a framework -- third parties write connectors, and a connector author writes Turtle. A rule
//! living in the write path lives in *contreforts'* Rust, hard-coded for *other people's*
//! fields, and an external connector author could not express "my secret is required at create"
//! at all.
//!
//! The same argument is why nothing here is specific to secrets. This module never looks at
//! `contreforts:secret`, never looks at `sh:minCount`, and has no idea that D8 is about
//! credentials: it selects whole shapes by declared intent, and what differs between the two
//! shapes is entirely the declaration's business. Special-casing "drop `sh:minCount` on
//! `contreforts:secret true` properties when updating" would have re-introduced exactly the
//! hard-coded-policy-in-contreforts'-Rust problem the two-shape design exists to avoid, one
//! layer down.
//!
//! ## Absence means "applies to both", deliberately
//!
//! A node shape with no `contreforts:writeIntent` is [`IntentScope::Always`]: it validates
//! creates and updates alike. This is the opposite of the "explicit opt-in, absence is
//! meaningful" rule `contreforts:configField` and `contreforts:entityKind` established, and the
//! difference is intentional -- absence here has to mean exactly what the pre-D8, single-shape
//! world meant, or the ten connectors that have not been migrated yet (migrating them is a
//! separate step, see contreforts/contreforts-workspace#19) would stop validating on the day
//! this term landed. It is also the honest reading: a declaration that never mentions the
//! create/update distinction is not asserting anything about it.
//!
//! The permissive default is safe *only* because it is the declaration's default, not the
//! caller's: a caller cannot omit its [`WriteIntent`] (see that type's own doc comment), and a
//! declaration that scopes one shape but forgets the other is rejected outright rather than
//! silently conforming to nothing -- see [`coverage_gaps`].

use std::collections::BTreeMap;

use oxigraph::model::{Graph, NamedNodeRef, NamedOrBlankNodeRef, TermRef};
use shacl_rust::Shape;
use shacl_rust::core::target::Target;

/// `contreforts:writeIntent` -- the sixth vocabulary term. Not part of SHACL, so
/// `shacl-rust`'s typed `Shape` API has no field for it under any name and it is read straight
/// off the graph, exactly as `contreforts:secret`/`category`/`uiShape`/`configField` are (see
/// `model.rs`'s own note on the same limitation).
pub(crate) const WRITE_INTENT_PREDICATE: NamedNodeRef<'static> = NamedNodeRef::new_unchecked(
    "https://contreforts.ds-labs.org/ontologies/declaration#writeIntent",
);

const CREATE_VALUE: &str = "create";
const UPDATE_VALUE: &str = "update";

/// What a caller is about to do: bring a connector into existence, or change one that already
/// exists. Chosen by the caller at the call site, never inferred from the data.
///
/// **There is deliberately no `Default`, no `Option`, and no `validate()` overload that omits
/// it**, for the same reason `contreforts_vecdb::StoreAccess` has none: the two modes differ in
/// how *permissive* they are, so a forgotten mode would silently resolve to the permissive one
/// and a required secret would stop being required at create -- a failure that produces no
/// error, no log line and no failing test, only a connector saved without its credential.
/// Making it a required argument turns "which mode did you mean?" into a compile error at every
/// call site, which is the only place the question can actually be answered.
///
/// Not `#[non_exhaustive]`, again mirroring `StoreAccess` and for its reason: create/update is
/// a closed binary (it is the verb the API was called with, and there is no third one), and a
/// downstream `match` forced to carry a wildcard arm would silently fold a hypothetical third
/// mode into whichever branch the arm happened to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WriteIntent {
    /// The connector does not exist yet. Shapes scoped [`IntentScope::CreateOnly`] apply, which
    /// is where a required secret's `sh:minCount 1` lives.
    Create,
    /// The connector exists and is being changed. Shapes scoped [`IntentScope::CreateOnly`] do
    /// **not** apply, so an absent secret is absent-means-unchanged rather than a violation --
    /// D8's write-only rule (the API never serves a secret back, so a form that submits nothing
    /// for it is submitting "unchanged", not "clear").
    Update,
}

impl WriteIntent {
    /// Both intents, for a caller (or a coverage check) that has to consider each in turn.
    /// An array rather than an iterator so exhaustiveness is visible at the definition site:
    /// adding a third intent would leave this array stale, which is why `WriteIntent` is closed.
    pub const ALL: [WriteIntent; 2] = [WriteIntent::Create, WriteIntent::Update];

    /// The `contreforts:writeIntent` literal a declaration writes to scope a shape to this
    /// intent. Shared with [`IntentScope::from_declared`] so the reader and the error messages
    /// can never disagree about the spelling.
    pub fn declared_value(self) -> &'static str {
        match self {
            WriteIntent::Create => CREATE_VALUE,
            WriteIntent::Update => UPDATE_VALUE,
        }
    }
}

impl std::fmt::Display for WriteIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.declared_value())
    }
}

/// Which write intents a *declared node shape* participates in -- the declaration's side of the
/// same distinction [`WriteIntent`] is the caller's side of.
///
/// Three cases rather than two, because a shape can decline to take a position: a declaration
/// that never mentions `contreforts:writeIntent` is [`Always`](Self::Always), which is what
/// every declaration on disk today is. See this module's doc comment for why the absent case is
/// permissive here and required at the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntentScope {
    /// No `contreforts:writeIntent` on the shape: it validates both creates and updates. Every
    /// unmigrated connector declaration is this.
    Always,
    /// `contreforts:writeIntent "create"` -- the shape carrying `sh:minCount 1` on required
    /// secrets.
    CreateOnly,
    /// `contreforts:writeIntent "update"` -- the same connector without that `sh:minCount`, so
    /// an absent secret means unchanged.
    UpdateOnly,
}

impl IntentScope {
    /// Whether a shape with this scope is one of the shapes `intent` is validated against. The
    /// whole of "the validator selects by verb": everything else in this file exists to compute
    /// the two arguments to this one function.
    pub fn admits(self, intent: WriteIntent) -> bool {
        match (self, intent) {
            (IntentScope::Always, _) => true,
            (IntentScope::CreateOnly, WriteIntent::Create) => true,
            (IntentScope::UpdateOnly, WriteIntent::Update) => true,
            (IntentScope::CreateOnly, WriteIntent::Update) => false,
            (IntentScope::UpdateOnly, WriteIntent::Create) => false,
        }
    }

    /// The `contreforts:writeIntent` value that produces this scope, or `None` for
    /// [`Always`](Self::Always), which is produced by the term's absence and has no value.
    pub fn declared_value(self) -> Option<&'static str> {
        match self {
            IntentScope::Always => None,
            IntentScope::CreateOnly => Some(CREATE_VALUE),
            IntentScope::UpdateOnly => Some(UPDATE_VALUE),
        }
    }

    fn from_declared(value: &str) -> Option<Self> {
        match value {
            CREATE_VALUE => Some(IntentScope::CreateOnly),
            UPDATE_VALUE => Some(IntentScope::UpdateOnly),
            _ => None,
        }
    }
}

/// Every `contreforts:writeIntent` object on `node`, as a displayable string.
///
/// Deliberately not `shacl_rust::utils::get_string_value` (which every other
/// `contreforts:`-term read in this crate uses) on two counts, both of which would otherwise
/// turn a malformed declaration into a silently permissive one:
///   - it calls `object_for_subject_predicate`, which returns an arbitrary *one* of several
///     values, so a shape declaring both `"create"` and `"update"` would resolve to whichever
///     the graph iterated first rather than being reported as the contradiction it is;
///   - it maps a non-literal object to `None`, indistinguishable from the term being absent --
///     and absent means [`IntentScope::Always`], the permissive answer.
///
/// Here a non-literal is stringified instead, so it fails [`IntentScope::from_declared`] and is
/// named in an error.
fn declared_values(graph: &Graph, node: NamedOrBlankNodeRef<'_>) -> Vec<String> {
    graph
        .objects_for_subject_predicate(node, WRITE_INTENT_PREDICATE)
        .map(|term| match term {
            TermRef::Literal(literal) => literal.value().to_string(),
            other => other.to_string(),
        })
        .collect()
}

/// The [`IntentScope`] `node` declares, or a message naming what is wrong with what it declares.
///
/// `Err` is never "the shape is fine but I could not tell": it is a malformed declaration --
/// an unrecognised value, or more than one. Both are also rejected by meta-shape META-6 during
/// [`crate::validate`], and the overlap is deliberate rather than redundant:
/// [`crate::ConnectorValidator`] never runs the meta-shapes (it parses a shapes graph and
/// nothing else -- see its own module docs), so if this reader treated a bad value as `Always`
/// the server would start with a shape scoped to nothing in particular and validate every write
/// against it. The cost is that a connector's own `build.rs` reports one mistake twice, once
/// from META-6 and once from `lint::write_intent`; accepted, because the alternative is a
/// silently permissive server.
pub(crate) fn scope_of(
    graph: &Graph,
    node: NamedOrBlankNodeRef<'_>,
) -> Result<IntentScope, String> {
    let values = declared_values(graph, node);
    match values.as_slice() {
        [] => Ok(IntentScope::Always),
        [only] => IntentScope::from_declared(only).ok_or_else(|| {
            format!(
                "contreforts:writeIntent has the unrecognised value {only:?} -- it must be \
                 \"{CREATE_VALUE}\" (the shape that validates a connector being created, where a \
                 required secret carries sh:minCount 1) or \"{UPDATE_VALUE}\" (the shape that \
                 validates a change to an existing connector, where an absent secret means \
                 unchanged). Omit the term entirely for a shape that applies to both."
            )
        }),
        many => Err(format!(
            "contreforts:writeIntent is declared {} times ({}) -- a shape is the create shape or \
             the update shape, not both. Omit the term entirely for a shape that applies to both.",
            many.len(),
            many.join(", "),
        )),
    }
}

/// One `sh:targetClass` that has shapes for one write intent and none for the other.
pub(crate) struct CoverageGap {
    /// The `sh:targetClass` IRI nothing validates under [`Self::uncovered`].
    pub class_iri: String,
    /// The intent with no applicable shape.
    pub uncovered: WriteIntent,
    /// The scoped shapes that *do* target `class_iri`, named so the message can point at the
    /// half that was written rather than only at the half that was not.
    pub covering_shapes: Vec<String>,
}

impl CoverageGap {
    /// The one message both callers print -- `lint::write_intent` as a [`crate::Violation`]
    /// during declaration validation, and `ConnectorValidator::new` as a construction error at
    /// server startup. Written once here so the two can never drift into describing the same
    /// defect differently.
    pub fn message(&self) -> String {
        format!(
            "no shape validates a connector of class <{}> being {}d: {} target it, and every one \
             of them is scoped to the other intent by contreforts:writeIntent ({}). A SHACL \
             shape that resolves no focus nodes conforms vacuously, so leaving this half \
             unwritten would not reject {}s -- it would accept every one of them unchecked. \
             Declare the missing contreforts:writeIntent \"{}\" shape, or remove \
             contreforts:writeIntent so the shape that exists applies to both.",
            self.class_iri,
            self.uncovered,
            self.covering_shapes.len(),
            self.covering_shapes.join(", "),
            self.uncovered,
            self.uncovered.declared_value(),
        )
    }
}

/// Every `(target class, intent)` pair in `shapes` that no shape validates -- the existential
/// check that makes the two-shape design safe to select over.
///
/// This is the failure the two-shape design introduces and nothing else catches. `sh:minCount 1`
/// disappearing is visible; a whole *shape* disappearing is not, because SHACL constraints are
/// universally quantified over a resolved target set, so an intent with zero applicable shapes
/// does not fail -- it conforms, vacuously, for every instance. A connector that declares only
/// its create shape would therefore have every update accepted with nothing checked at all,
/// which is strictly worse than the pre-D8 behaviour it replaced. Same reason `lint::structural`
/// and the D15 existential rules are Rust rather than SHACL (see `lint/mod.rs`).
///
/// Inert for a declaration that does not use `contreforts:writeIntent`: an
/// [`IntentScope::Always`] shape admits both intents, so a class with one unscoped shape can
/// never appear here. The ten unmigrated connectors are unaffected.
///
/// A "connector declares create-only on purpose, because it forbids updates" reading was
/// considered and rejected: it is indistinguishable from having forgotten the update shape, and
/// the two want opposite outcomes (reject every update / accept every update). A declaration
/// that means to forbid updates should say so with an update shape that forbids them, which is
/// expressible, rather than by an absence, which is not.
///
/// Returns empty when any shape's `contreforts:writeIntent` is unreadable: that is reported by
/// [`scope_of`]'s own `Err` at the same call sites, and computing coverage from a scope nobody
/// could read would pile a speculative second violation on top of the real one.
pub(crate) fn coverage_gaps(shapes: &[Shape<'_>], graph: &Graph) -> Vec<CoverageGap> {
    // BTreeMap, not HashMap: the gaps are rendered into an error message a human reads and a
    // test asserts on, so the order has to be the same on every run.
    let mut by_class: BTreeMap<String, Vec<(String, IntentScope)>> = BTreeMap::new();

    for shape in shapes {
        let Ok(scope) = scope_of(graph, shape.node) else {
            return Vec::new();
        };
        for target in &shape.targets {
            let Target::Class(NamedOrBlankNodeRef::NamedNode(class_node)) = target else {
                continue;
            };
            by_class
                .entry(class_node.as_str().to_string())
                .or_default()
                .push((shape.node.to_string(), scope));
        }
    }

    let mut gaps = Vec::new();
    for (class_iri, shapes_for_class) in by_class {
        for intent in WriteIntent::ALL {
            if shapes_for_class
                .iter()
                .any(|(_, scope)| scope.admits(intent))
            {
                continue;
            }
            gaps.push(CoverageGap {
                class_iri: class_iri.clone(),
                uncovered: intent,
                covering_shapes: shapes_for_class
                    .iter()
                    .map(|(node, _)| node.clone())
                    .collect(),
            });
        }
    }
    gaps
}

#[cfg(test)]
mod tests {
    use super::*;
    use shacl_rust::parser::parse_shapes;
    use shacl_rust::rdf::read_graph_from_string;

    /// Two shapes over one class, scoped create and update -- the D8 amended shape of a
    /// migrated declaration, reduced to the two triples this module actually reads.
    const PAIRED: &str = r#"
        @prefix sh:          <http://www.w3.org/ns/shacl#> .
        @prefix contreforts: <https://contreforts.ds-labs.org/ontologies/declaration#> .
        @prefix w:           <https://contreforts.ds-labs.org/ontologies/widget#> .

        w:CreateShape a sh:NodeShape ;
            sh:targetClass w:Widget ;
            contreforts:writeIntent "create" ;
            sh:property [ sh:path w:token ; sh:minCount 1 ] .

        w:UpdateShape a sh:NodeShape ;
            sh:targetClass w:Widget ;
            contreforts:writeIntent "update" ;
            sh:property [ sh:path w:token ] .
    "#;

    fn gaps_of(ttl: &str) -> Vec<CoverageGap> {
        let graph = read_graph_from_string(ttl, "turtle").expect("valid turtle");
        let shapes = parse_shapes(&graph).expect("well-formed SHACL");
        coverage_gaps(&shapes, &graph)
    }

    #[test]
    fn a_complete_pair_has_no_coverage_gap() {
        // Catches: a coverage check that fires on the very arrangement D8 prescribes -- which
        // would make the two-shape design unusable rather than merely unenforced.
        assert!(gaps_of(PAIRED).is_empty());
    }

    #[test]
    fn a_create_only_declaration_leaves_update_uncovered() {
        // Catches: deleting `coverage_gaps`'s body (or its call sites). Without it a
        // create-only declaration validates every update against zero shapes and conforms
        // vacuously -- the silent-acceptance failure this function exists for.
        let create_only = PAIRED
            .split("w:UpdateShape")
            .next()
            .expect("split always yields a first part");
        let gaps = gaps_of(create_only);
        assert_eq!(gaps.len(), 1, "exactly the update half is missing");
        assert_eq!(gaps[0].uncovered, WriteIntent::Update);
        assert!(gaps[0].class_iri.ends_with("#Widget"));
    }

    #[test]
    fn an_unscoped_shape_covers_both_intents() {
        // Catches: reading absence as anything other than "applies to both" -- which would
        // report a gap for all ten unmigrated connector declarations at once.
        let unscoped = r#"
            @prefix sh: <http://www.w3.org/ns/shacl#> .
            @prefix w:  <https://contreforts.ds-labs.org/ontologies/widget#> .
            w:Shape a sh:NodeShape ;
                sh:targetClass w:Widget ;
                sh:property [ sh:path w:token ] .
        "#;
        assert!(gaps_of(unscoped).is_empty());
    }

    #[test]
    fn an_unrecognised_write_intent_value_is_an_error_not_a_permissive_default() {
        // Catches: `scope_of` falling back to `Always` on a typo. That fallback is invisible --
        // the declaration still validates, the server still starts, and the create shape
        // silently applies to updates too.
        let typo = r#"
            @prefix sh:          <http://www.w3.org/ns/shacl#> .
            @prefix contreforts: <https://contreforts.ds-labs.org/ontologies/declaration#> .
            @prefix w:           <https://contreforts.ds-labs.org/ontologies/widget#> .
            w:Shape a sh:NodeShape ;
                sh:targetClass w:Widget ;
                contreforts:writeIntent "creaate" ;
                sh:property [ sh:path w:token ] .
        "#;
        let graph = read_graph_from_string(typo, "turtle").expect("valid turtle");
        let shapes = parse_shapes(&graph).expect("well-formed SHACL");
        let shape = shapes
            .iter()
            .find(|s| s.node.to_string().ends_with("Shape>"))
            .expect("the one node shape");
        let message = scope_of(&graph, shape.node).expect_err("a typo must not resolve");
        assert!(
            message.contains("creaate"),
            "the message must quote what was written, got: {message}"
        );
    }

    #[test]
    fn declaring_both_intents_on_one_shape_is_an_error() {
        // Catches: reading the term with a single-value graph lookup, which would pick one of
        // the two arbitrarily and silently scope the shape to whichever the graph iterated
        // first.
        let both = r#"
            @prefix sh:          <http://www.w3.org/ns/shacl#> .
            @prefix contreforts: <https://contreforts.ds-labs.org/ontologies/declaration#> .
            @prefix w:           <https://contreforts.ds-labs.org/ontologies/widget#> .
            w:Shape a sh:NodeShape ;
                sh:targetClass w:Widget ;
                contreforts:writeIntent "create", "update" ;
                sh:property [ sh:path w:token ] .
        "#;
        let graph = read_graph_from_string(both, "turtle").expect("valid turtle");
        let shapes = parse_shapes(&graph).expect("well-formed SHACL");
        let shape = shapes
            .iter()
            .find(|s| s.node.to_string().ends_with("Shape>"))
            .expect("the one node shape");
        assert!(scope_of(&graph, shape.node).is_err());
    }

    #[test]
    fn scope_admits_exactly_its_own_intent() {
        // Catches: an `admits` written as `!=` or with the two arms transposed -- a selection
        // that runs the create shape on updates and the update shape on creates, which passes
        // any test that only ever checks one direction.
        assert!(IntentScope::CreateOnly.admits(WriteIntent::Create));
        assert!(!IntentScope::CreateOnly.admits(WriteIntent::Update));
        assert!(IntentScope::UpdateOnly.admits(WriteIntent::Update));
        assert!(!IntentScope::UpdateOnly.admits(WriteIntent::Create));
        assert!(IntentScope::Always.admits(WriteIntent::Create));
        assert!(IntentScope::Always.admits(WriteIntent::Update));
    }
}
