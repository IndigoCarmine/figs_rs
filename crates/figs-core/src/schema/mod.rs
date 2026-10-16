//! TOML schema: the flat, id-keyed document format and its resolution into a
//! typed, index-addressed tree.
//!
//! The on-disk format is a flat map of nodes (`[nodes.<id>]`) whose tree
//! structure lives entirely in `children = ["id", ...]` arrays. See
//! [`parse`] for the raw deserialization layer and [`resolve`] for validation
//! and conversion into the engine's [`resolve::Document`].

pub mod parse;
pub mod resolve;
pub mod span;

pub use parse::{
    ArcRadius, NodeKind, PageHeight, RawDocument, RawGlow, RawNode, RawPage, RawSegment, RawShadow,
};
pub use resolve::{
    Common, Decoration, Document, EllipseProps, ImageProps, Node, NodeType, PageInfo, PathProps,
    RectProps, ResolveError, TextProps,
};
pub use span::{parse_family_list, FontStyle, RawSpan, Script, SpanStyle, TextSpan};
