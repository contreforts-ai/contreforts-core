//! D15 -- the two existential `contreforts:configField` rules (Part 3 of
//! contreforts/contreforts-core#16). Rust, for the reason contreforts-
//! core#14 already established and #16 reuses verbatim: a SHACL constraint
//! is universally quantified over its resolved target set, so an absence
//! of any offending value conforms vacuously -- these two rules are about
//! *other* property shapes on the same node, which no `sh:property`
//! constraint can see.
//!
//!   1. No two property shapes on the same node shape may name the same
//!      `contreforts:configField` -- two declared properties silently
//!      overwriting one config field, an outcome that depends on
//!      iteration order.
//!   2. A property shape carrying `contreforts:secret true` must also
//!      carry `contreforts:configField` -- narrowed below.
//!
//! ## Rule 2 is scoped to node shapes that already use `configField` --
//! ## a narrowing the issue's own text does not state, and a genuine
//! ## contradiction in it
//!
//! contreforts-core#16's Part 3 states rule 2 unconditionally: "A property
//! shape carrying `contreforts:secret true` must also carry
//! `contreforts:configField`." Taken literally, that rejects BOTH real
//! fixtures this crate's own tests validate against:
//! `forgejo-declaration.ttl`'s `token` and `o365-declaration.ttl`'s
//! `clientSecret`/`token`/`refreshToken` all carry `contreforts:secret
//! true`, and, correctly per the issue's own "Explicitly out of scope"
//! section, carry no `contreforts:configField` at all -- annotating them
//! is step 2, "its own repo, its own PR, after this lands." The issue's
//! own Tests section separately requires "Both real fixtures still
//! validate clean (neither carries the new term yet -- absence must be
//! legal)."
//!
//! Read literally, those two requirements cannot both hold: the exact same
//! fact pattern -- `contreforts:secret true`, no `contreforts:configField`
//! anywhere on the property, and the containing declaration using
//! `configField` nowhere else either -- is required to both PASS (both
//! real fixtures) and FAIL (a synthetic declaration built to exercise this
//! rule) with nothing stated in the issue to tell the two cases apart.
//! This is a genuine contradiction in the issue text, reported as a
//! finding rather than silently worked around -- see the PR this lint
//! shipped in.
//!
//! The narrowing implemented here: rule 2 only fires on a node shape that
//! *already* carries `contreforts:configField` on at least one of its own
//! property shapes. A connector that has not started stating its runtime-
//! config surface via `configField` at all is unaffected by this specific
//! check -- there is no generic adapter yet reading a partial
//! `configField` mapping for such a connector, for it to silently corrupt
//! (the issue's own motivation section: contreforts-config-api#6's
//! hand-written `build_forgejo` adapter is exactly today's state for both
//! real fixtures, unchanged by this PR). A node shape that HAS started --
//! carries `configField` on even one field -- is held to the rule fully:
//! every `contreforts:secret true` property on that same node shape must
//! be accounted for. This preserves both real fixtures' passing tests
//! *and* the synthetic rejection case the issue's own Tests section asks
//! for; see `declaration/tests/fixtures/config-field-secret-without-field.ttl`.
//!
//! Rule 1 needed no such narrowing: neither real fixture uses
//! `contreforts:configField` at all yet, so rule 1 never fires on them
//! regardless of scoping.

use std::collections::HashMap;

use oxigraph::model::{Graph, NamedNodeRef};
use shacl_rust::Shape;
use shacl_rust::utils::{get_boolean_value, get_string_value};

use crate::error::Violation;

const CONFIG_FIELD_PREDICATE: NamedNodeRef<'static> = NamedNodeRef::new_unchecked(
    "https://contreforts.ds-labs.org/ontologies/declaration#configField",
);
const SECRET_PREDICATE: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("https://contreforts.ds-labs.org/ontologies/declaration#secret");

fn config_field(graph: &Graph, shape: &Shape<'_>) -> Option<String> {
    get_string_value(graph, shape.node, CONFIG_FIELD_PREDICATE)
}

fn is_secret(graph: &Graph, shape: &Shape<'_>) -> bool {
    get_boolean_value(graph, shape.node, SECRET_PREDICATE).unwrap_or(false)
}

fn shape_label(shape: &Shape<'_>) -> String {
    shape.get_name()
}

/// Rule 1: within one node shape's own direct `sh:property` children, no
/// two may declare the same `contreforts:configField`. Walking only the
/// direct children (not recursing into further-nested shapes) matches
/// what "the same node shape" means -- an `sh:xone` alternative or an
/// `sh:node`-nested shape is itself a separate node shape in `shapes`,
/// checked separately by this same loop.
fn check_duplicates(shapes: &[Shape<'_>], graph: &Graph) -> Vec<Violation> {
    let mut violations = Vec::new();

    for shape in shapes {
        let mut seen: HashMap<String, &Shape<'_>> = HashMap::new();
        for prop in &shape.property_shapes {
            let Some(field) = config_field(graph, prop) else {
                continue;
            };
            match seen.get(field.as_str()) {
                Some(previous) => {
                    violations.push(Violation::d15_config_field(
                        Some(shape_label(shape)),
                        format!(
                            "node shape \"{}\" has two property shapes naming the same \
                             contreforts:configField \"{field}\": \"{}\" and \"{}\" -- two \
                             declared properties feeding one config field is a silent overwrite \
                             whose outcome depends on iteration order. Give each property its \
                             own contreforts:configField, or remove the annotation from \
                             whichever one does not really reach the connector's runtime \
                             config.",
                            shape_label(shape),
                            shape_label(previous),
                            shape_label(prop),
                        ),
                    ));
                }
                None => {
                    seen.insert(field, prop);
                }
            }
        }
    }

    violations
}

/// Rule 2, narrowed as documented in this module's doc comment above:
/// within a node shape that already uses `contreforts:configField` on at
/// least one of its own property shapes, every `contreforts:secret true`
/// property shape on that same node shape must carry
/// `contreforts:configField` too.
fn check_secret_without_config_field(shapes: &[Shape<'_>], graph: &Graph) -> Vec<Violation> {
    let mut violations = Vec::new();

    for shape in shapes {
        let uses_config_field = shape
            .property_shapes
            .iter()
            .any(|p| config_field(graph, p).is_some());
        if !uses_config_field {
            continue;
        }

        for prop in &shape.property_shapes {
            if is_secret(graph, prop) && config_field(graph, prop).is_none() {
                violations.push(Violation::d15_config_field(
                    Some(shape_label(prop)),
                    format!(
                        "property shape \"{}\" on node shape \"{}\" carries \
                         contreforts:secret true but no contreforts:configField, even though \
                         this node shape already uses contreforts:configField elsewhere -- a \
                         secret that reaches no config field is either a mislabelled storage \
                         field or a credential the connector can never use. Add \
                         contreforts:configField naming the runtime config struct field it \
                         supplies, or drop contreforts:secret if it is not really \
                         configuration.",
                        shape_label(prop),
                        shape_label(shape),
                    ),
                ));
            }
        }
    }

    violations
}

/// Runs both D15 `contreforts:configField` lints over `shapes` (the
/// declaration's own parsed shapes, not the meta-shapes) and `graph` (the
/// declaration's own graph -- needed because `contreforts:configField` and
/// `contreforts:secret` are not part of SHACL, so `shacl-rust`'s typed
/// `Shape` API has no field for either; see `model.rs`'s own comment on
/// the same limitation).
pub(crate) fn check(shapes: &[Shape<'_>], graph: &Graph) -> Vec<Violation> {
    let mut violations = check_duplicates(shapes, graph);
    violations.extend(check_secret_without_config_field(shapes, graph));
    violations
}
