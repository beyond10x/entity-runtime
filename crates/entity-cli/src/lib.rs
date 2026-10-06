//! The `entity` command's command-line definition, as a library.
//!
//! The `entity` binary is built from `src/main.rs`; this library holds only [`cli::Cli`], the clap
//! definition it parses, so that the documentation generator (`entity-runtime-docs`) renders the
//! CLI reference from the definition the binary actually uses instead of from a copy.

pub mod cli;
