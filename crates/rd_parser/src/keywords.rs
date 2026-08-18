// crates/rd_parser/src/keywords.rs
//
// PHF-backed static maps for O(1) keyword/type classification.
//
// ## Design (lifted from DixScript pattern)
//
// Maps live in the binary's read-only data segment — zero runtime init,
// no heap allocation, no lock. Lookups are O(1) via compile-time perfect hashing.
//
// We use `phf::Map<&'static str, ()>` for membership checks (contains_key),
// and typed maps (e.g. `phf::Map<&str, SizeUnit>`) where the value is needed.
//
// TokenType dispatch still uses `match` (not PHF) because TokenType doesn't
// implement Hash — `match` compiles to a jump table which is faster anyway.

use phf::{phf_map, Map};
use ubel_stratum::ast::statements::SizeUnit;

// ── Type keyword maps ─────────────────────────────────────────────────────────

/// All primitive type names. Used as fast membership check before attempting
/// the full TypeKind construction match in parse_type.rs.
pub static PRIMITIVE_TYPES: Map<&'static str, ()> = phf_map! {
    "int"    => (), "uint"   => (), "long"   => (), "ulong"  => (),
    "short"  => (), "ushort" => (), "byte"   => (), "ubyte"  => (),
    "float"  => (), "double" => (), "bool"   => (), "char"   => (),
    "string" => (), "void"   => (),
    "i8"     => (), "i16"    => (), "i32"    => (), "i64"    => (),
    "u8"     => (), "u16"    => (), "u32"    => (), "u64"    => (),
    "f32"    => (), "f64"    => (),
    "isize"  => (), "usize"  => (),
};

/// Built-in generic collection type names.
pub static COLLECTION_TYPES: Map<&'static str, ()> = phf_map! {
    "List"       => (),
    "Dictionary" => (),
    "Set"        => (),
    "Queue"      => (),
    "Stack"      => (),
};

// ── @cfg composition operators ────────────────────────────────────────────────

/// The three valid composition operators inside `@cfg(...)`.
/// Also Idents in the token stream; detected by lexeme comparison.
pub static CFG_COMPOSE: Map<&'static str, ()> = phf_map! {
    "not" => (),
    "any" => (),
    "all" => (),
};

// ── Arena size units ──────────────────────────────────────────────────────────

/// Maps `"B"`, `"KB"`, `"MB"`, `"GB"` → `SizeUnit`.
/// Used in `with arena(256 KB)` parsing. Values are `Copy` so PHF can hold them.
pub static SIZE_UNITS: Map<&'static str, SizeUnit> = phf_map! {
    "B"  => SizeUnit::Bytes,
    "KB" => SizeUnit::KB,
    "MB" => SizeUnit::MB,
    "GB" => SizeUnit::GB,
};

// ── Public helpers ────────────────────────────────────────────────────────────

/// O(1) check: is `name` a primitive type keyword?
#[inline]
pub fn is_primitive_type(name: &str) -> bool {
    PRIMITIVE_TYPES.contains_key(name)
}

/// O(1) check: is `name` a built-in collection type?
#[inline]
pub fn is_collection_type(name: &str) -> bool {
    COLLECTION_TYPES.contains_key(name)
}

/// O(1) check: is `name` a @cfg composition operator (not/any/all)?
#[inline]
pub fn is_cfg_compose(name: &str) -> bool {
    CFG_COMPOSE.contains_key(name)
}

/// O(1) check + value: parse a size unit string → `SizeUnit`.
/// Returns `None` if `name` is not a valid unit.
#[inline]
pub fn parse_size_unit(name: &str) -> Option<SizeUnit> {
    SIZE_UNITS.get(name).copied()
}
