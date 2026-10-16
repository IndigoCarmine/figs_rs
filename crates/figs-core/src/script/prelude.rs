//! The framework prelude: opinionated components (`grid`, `panels`, `title`,
//! `page("a4", root)`, …) written in Rhai on top of the core builders.
//!
//! The core builders registered in [`super`] stay explicit (1px-level freedom);
//! the prelude is the concise, paternalistic layer above them. Its source
//! (`prelude.rhai`) is compiled once and *merged in front of* every user script,
//! so its functions are callable without an `import`, and a user `fn` with the
//! same name and arity replaces the prelude's — which is how the default hooks
//! (`base_size()`, `gap()`, `label_style()`) are customised.

use std::sync::OnceLock;

use rhai::{Engine, EvalAltResult, AST};

/// The framework source, embedded at build time.
const PRELUDE_SRC: &str = include_str!("prelude.rhai");

static PRELUDE: OnceLock<AST> = OnceLock::new();

/// Compile `src` and merge it after the (cached) prelude. User functions
/// override prelude functions of the same name and arity; the user script's
/// statements (and so its return value) are unchanged.
pub(super) fn compile_with_prelude(engine: &Engine, src: &str) -> Result<AST, Box<EvalAltResult>> {
    let prelude = PRELUDE.get_or_init(|| {
        engine
            .compile(PRELUDE_SRC)
            .expect("the bundled prelude.rhai must compile")
    });
    let user = engine.compile(src)?;
    Ok(prelude.merge(&user))
}
