//! Shared library for the `figs` CLI: the render pipeline and the file watcher,
//! exposed so they can be tested independently of the binary.

pub mod pipeline;
pub mod watch;
