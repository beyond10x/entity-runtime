//! Repository-only checks that do not belong in the public `entity` command.

use clap::{Parser, Subcommand};
use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

const MINIMUM_AEP_PROTOCOL: (u64, u64, u64) = (0, 26, 0);

#[derive(Debug, Parser)]
#[command(name = "entity-xtask")]
#[command(about = "Repository-only Entity Runtime checks")]
struct Cli {
    #[command(subcommand)]
    command: Check,
}

#[derive(Debug, Subcommand)]
enum Check {
    /// Refuse an `aep` command too old for the planning journal.
    AepVersion {
        /// The `aep` the caller's shell resolves — `$(command -v aep)` in the Taskfile. Passed in
        /// because `cargo run` prepends its own directories to this process's PATH, so a lookup
        /// from here can find an `aep` the next Taskfile line, run by the shell, will not. An empty
        /// value means the shell found none, and is refused as such; clap's own path parser calls
        /// an empty value missing, so `shell_path` accepts it and the check says what it means.
        #[arg(long, value_parser = shell_path)]
        binary: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Check::AepVersion { binary } => check_aep_version(binary),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, message)) => {
            eprintln!("{message}");
            ExitCode::from(code)
        }
    }
}

// `aep` is the binary the gate actually runs against the planning store, so it is the one whose
// version matters. There is deliberately no fallback to the older `protocol` name: a machine
// carrying a stale `aep` beside a current `protocol` would otherwise pass while writing the store
// with the build this guard exists to refuse.
fn check_aep_version(binary: Option<PathBuf>) -> Result<(), (u8, String)> {
    let binary = match binary {
        Some(path) if path.as_os_str().is_empty() => {
            return Err((
                2,
                "aep is not on the shell's PATH; install the AEP CLI, which is what reads and \
                 writes this store"
                    .to_owned(),
            ))
        }
        Some(path) => path,
        None => find_on_path("aep").ok_or_else(|| {
            (
                2,
                "aep is not on PATH; install the AEP CLI, which is what reads and writes this store"
                    .to_owned(),
            )
        })?,
    };
    let output = Command::new(&binary)
        .arg("--version")
        .output()
        .map_err(|error| {
            (
                2,
                format!("aep at {} could not run: {error}", binary.display()),
            )
        })?;
    let rendered = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).into_owned()
    } else {
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    if !output.status.success() {
        return Err((
            1,
            format!(
                "aep at {} exited {} while reporting its version: {}",
                binary.display(),
                output.status,
                rendered.trim()
            ),
        ));
    }
    let version = parse_version(&rendered).ok_or_else(|| {
        (
            1,
            format!(
                "aep at {} printed no semantic version: {:?}",
                binary.display(),
                rendered.trim()
            ),
        )
    })?;
    if version < MINIMUM_AEP_PROTOCOL {
        return Err((
            1,
            format!(
                "aep at {} reports {}; this store needs at least {}",
                binary.display(),
                render_version(version),
                render_version(MINIMUM_AEP_PROTOCOL)
            ),
        ));
    }
    println!(
        "aep implements protocol {} at {} (needs {})",
        render_version(version),
        binary.display(),
        render_version(MINIMUM_AEP_PROTOCOL)
    );
    Ok(())
}

/// `$(command -v aep)` verbatim, empty included: the emptiness is the finding.
fn shell_path(value: &str) -> Result<PathBuf, String> {
    Ok(PathBuf::from(value))
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

// Tolerant of whatever precedes the digits: `aep --version` prints the protocol version it
// implements rather than its own name, so the leading word is not something to match on.
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    text.split(|character: char| !(character.is_ascii_digit() || character == '.'))
        .filter(|part| !part.is_empty())
        .find_map(|candidate| {
            let mut parts = candidate.split('.');
            let version = (
                parts.next()?.parse().ok()?,
                parts.next()?.parse().ok()?,
                parts.next()?.parse().ok()?,
            );
            parts.next().is_none().then_some(version)
        })
}

fn render_version(version: (u64, u64, u64)) -> String {
    format!("{}.{}.{}", version.0, version.1, version.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_aep_version_line_is_extracted_without_accepting_partial_numbers() {
        assert_eq!(parse_version("protocol 0.54.0\n"), Some((0, 54, 0)));
        assert_eq!(parse_version("aep version fifty-four"), None);
        assert_eq!(parse_version("protocol 0.54"), None);
    }
}
