//! TOML schema: the flat, id-keyed document format and its resolution into a
//! typed, index-addressed tree.
//!
//! The on-disk format is a flat map of nodes (`[nodes.<id>]`) whose tree
//! structure lives entirely in `children = ["id", ...]` arrays. See
//! [`parse`] for the raw deserialization layer and [`resolve`] for validation
//! and conversion into the engine's [`resolve::Document`].

pub mod parse;
pub mod resolve;

pub use parse::{NodeKind, RawDocument, RawNode, RawPage};
pub use resolve::{
    Common, Document, ImageProps, Node, NodeType, PageInfo, RectProps, ResolveError, TextProps,
};
