# contreforts-declaration

The declaration vocabulary, its meta-shapes, and the D14 `sh:xone`
cross-exclusion lint. Part of the bootstrap epic
(contreforts/contreforts-workspace#20), phase C item C1 --
contreforts/contreforts-core#14.

A connector describes itself in Turtle
(contreforts-connector-forgejo#9 is the pilot). This crate defines what a
valid self-description *is*, and checks it at build time:

```rust
pub fn validate(turtle: &str) -> Result<Declaration, Violations>;
```

`validate` parses a declaration, runs it against `meta-shapes.ttl` (SHACL),
and runs the D14 (`sh:xone` cross-exclusion), D2 (`core:` namespace) and
structural lints that SHACL cannot express as constraints on itself. See
`src/lib.rs` for the full design writeup and the reasoning behind each
piece.

## This nesting is provisional

This crate lives nested inside `contreforts-core`, at `declaration/` -- the
workspace member path is `crates/contreforts-core/declaration`. Every other
crate in this org is one-crate-per-repo; this one breaks that deliberately,
to avoid standing up a new repository before the mechanism has proven
itself. It is expected to be replaced by, or extended into, its own
repository once phase C is complete and the shape of this crate has
stopped moving -- do not mistake this nesting for a considered end state.

This crate does not depend on `contreforts-core` in any way (no `path =
"../"`, no use of anything core exports), specifically so that promotion
to its own repository is a `git mv` plus one `Cargo.toml` line, not a
refactor. See contreforts/contreforts-core#14 for the full discussion.

## Building and testing

This crate is not itself listed in `contreforts-workspace`'s root
`Cargo.toml` `members` yet -- that is a separate PR, sequenced after this
one merges (see the issue). To build or test it locally before that PR
lands, add `"crates/contreforts-core/declaration"` to that `members` list
and run, from the workspace root:

```bash
cargo test -p contreforts-declaration
```
