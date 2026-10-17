//! cie-semantic — the typed fact vocabulary (PRD §14) and the language
//! table. Adapters speak these fact families; the engine consumes them.

pub mod families;
pub mod facts;
pub mod graph;
pub mod lang;

pub use facts::*;
pub use families::{AsyncFact, AsyncKind, CallFact, CallKind, EffectFact, ExceptionFact, ExceptionKind};
pub use lang::{lang_for_path, LangId, LanguageTable};
