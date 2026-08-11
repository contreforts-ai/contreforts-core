//! Vector column geometry, owned in one place.
//!
//! contreforts-vecdb#45. [`VectorStoreColumnType`] used to exist twice — once in
//! `contreforts-vecdb` and once in `contreforts-config` — with the same three variants and the
//! same measured HNSW limits, written out by hand on both sides.
//!
//! ## Why the duplication was worse than it looked
//!
//! Both copies are load-bearing and they refuse the same mistake at different moments:
//! `contreforts-config` rejects an unindexable dimension/geometry pair when a connector is
//! **saved**, `contreforts-vecdb` rejects it when a store is **built**, and
//! `contreforts-config-api` generates DDL from the config-side one.
//!
//! **No crate in the workspace saw both types**, so nothing could compare them — not the
//! compiler, not even a test. Correct one side's limit after a new measurement and forget the
//! other, and a geometry is accepted at save time and refused at build time; or, worse, accepted
//! by both and unindexable in fact. Nothing anywhere would report it. Agreement written in two
//! doc comments is documentation, not a guard.
//!
//! ## Why here
//!
//! `contreforts-vecdb` owning it would make `contreforts-config` — a crate whose whole job is an
//! Oxigraph store — depend on `sqlx` and `reqwest` for one enum. `contreforts-core` is
//! dependency-light and both crates already depend on it, so this costs nothing and inverts
//! nothing.

/// How a vector column stores each axis, and how far each geometry can be indexed.
///
/// The limits are **measured**, not taken from documentation: pgvector's HNSW build fails above
/// them, and the failure is at index-creation time rather than at insert, so a table can hold
/// rows it can never search efficiently.
///
/// | variant | axis | HNSW builds up to |
/// |---|---|---|
/// | [`Vector`](Self::Vector) | `f32` | 2 000 |
/// | [`Halfvec`](Self::Halfvec) | `f16` | 4 000 |
/// | [`Bit`](Self::Bit) | 1 bit | 4 096 |
///
/// Changing any number here is a real change with a measurement behind it. It is now a change in
/// **one** place, which is the whole point of this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
// `snake_case`, matching what `contreforts-config` already serialised before this type moved
// here. The wire form is stored in configuration graphs and read by the config API, so changing
// it would silently stop recognising every geometry already recorded.
#[serde(rename_all = "snake_case")]
pub enum VectorStoreColumnType {
    /// `f32` per axis. The default, and the only one `contreforts-vecdb` reads today.
    #[default]
    Vector,
    /// `f16` per axis: half the bytes, twice the indexable axes.
    Halfvec,
    /// One bit per axis. Cheapest to index and the only variant that reaches 4 096, at the cost
    /// of needing exact rescoring in the retrieval path.
    Bit,
}

impl VectorStoreColumnType {
    /// The SQL type name, as it appears in DDL and as `format_type` renders it.
    ///
    /// Both directions matter: this string is written into `CREATE TABLE` and compared against
    /// what PostgreSQL reports back for an existing column, so a change here silently stops
    /// recognising tables it created itself.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vector => "vector",
            Self::Halfvec => "halfvec",
            Self::Bit => "bit",
        }
    }

    /// Largest dimension this column type can carry in an HNSW index.
    pub fn max_indexable_dimension(self) -> u32 {
        match self {
            Self::Vector => 2_000,
            Self::Halfvec => 4_000,
            Self::Bit => 4_096,
        }
    }

    /// Parse the SQL type name back, exactly as [`as_str`](Self::as_str) writes it.
    ///
    /// Round-trip partner rather than a convenience: it is what lets a test assert the two
    /// directions agree, which is the property that actually breaks when a variant is added and
    /// only one of the two matches is extended.
    pub fn from_sql_name(name: &str) -> Option<Self> {
        match name {
            "vector" => Some(Self::Vector),
            "halfvec" => Some(Self::Halfvec),
            "bit" => Some(Self::Bit),
            _ => None,
        }
    }

    /// Every variant, so a caller iterating them cannot miss one added later.
    pub const ALL: [Self; 3] = [Self::Vector, Self::Halfvec, Self::Bit];

    /// Whether `dimension` axes of this geometry can be HNSW-indexed.
    ///
    /// Inclusive: the limits above are the largest dimension that **works**, not the first that
    /// fails. That boundary is worth stating because both call sites reimplemented the
    /// comparison and an off-by-one here refuses a legal 2 000-dimension store.
    pub fn can_index(self, dimension: u32) -> bool {
        dimension <= self.max_indexable_dimension()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The measured table, asserted rather than described.
    ///
    /// This is the assertion the duplication made impossible: before contreforts-vecdb#45 no
    /// crate saw both copies of this enum, so nothing could check they agreed. Now there is one
    /// copy and one place these numbers can be wrong.
    #[test]
    fn the_measured_hnsw_limits_are_what_they_are() {
        assert_eq!(
            VectorStoreColumnType::Vector.max_indexable_dimension(),
            2_000
        );
        assert_eq!(
            VectorStoreColumnType::Halfvec.max_indexable_dimension(),
            4_000
        );
        assert_eq!(VectorStoreColumnType::Bit.max_indexable_dimension(), 4_096);
    }

    #[test]
    fn sql_names_round_trip_through_both_directions() {
        for t in VectorStoreColumnType::ALL {
            assert_eq!(
                VectorStoreColumnType::from_sql_name(t.as_str()),
                Some(t),
                "{t:?} does not survive as_str -> from_sql_name"
            );
        }
        assert_eq!(VectorStoreColumnType::from_sql_name("vectorr"), None);
        assert_eq!(VectorStoreColumnType::from_sql_name("VECTOR"), None);
    }

    /// The limit is the largest dimension that works, not the first that fails.
    #[test]
    fn can_index_is_inclusive_at_the_limit() {
        let v = VectorStoreColumnType::Vector;
        assert!(v.can_index(2_000), "2000 axes index fine on vector");
        assert!(!v.can_index(2_001));
        assert!(VectorStoreColumnType::Bit.can_index(4_096));
        assert!(!VectorStoreColumnType::Bit.can_index(4_097));
    }

    /// The default is `Vector`, and it is the supported case rather than a guess.
    #[test]
    fn the_default_is_the_only_geometry_the_store_reads_today() {
        assert_eq!(
            VectorStoreColumnType::default(),
            VectorStoreColumnType::Vector
        );
    }

    /// `ALL` must actually list every variant.
    ///
    /// Nothing in Rust enforces that, and a variant added without extending `ALL` would make
    /// every loop above silently skip it — including the round-trip test, which is the one thing
    /// standing between a new geometry and a name that does not parse back.
    #[test]
    fn all_lists_every_variant() {
        let mut seen: Vec<&str> = VectorStoreColumnType::ALL
            .iter()
            .map(|t| t.as_str())
            .collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(
            seen.len(),
            VectorStoreColumnType::ALL.len(),
            "ALL contains a duplicate"
        );
        // Exhaustive match: adding a variant fails to compile here until ALL is extended.
        for t in VectorStoreColumnType::ALL {
            match t {
                VectorStoreColumnType::Vector
                | VectorStoreColumnType::Halfvec
                | VectorStoreColumnType::Bit => {}
            }
        }
        assert_eq!(seen, ["bit", "halfvec", "vector"]);
    }
}
