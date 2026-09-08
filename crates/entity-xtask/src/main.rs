//! Repository-only checks that do not belong in the public `entity` command.

use clap::{Parser, Subcommand};
use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

const MINIMUM_PROTOCOL: (u64, u64, u64) = (0, 26, 0);

#[derive(Debug, Parser)]
#[command(name = "entity-xtask")]
#[command(about = "Repository-only Entity Runtime checks")]
struct Cli {
    #[command(subcommand)]
    command: Check,
}

#[derive(Debug, Subcommand)]
enum Check {
    /// Refuse a protocol compatibility command too old for the planning journal.
    ProtocolVersion,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Check::ProtocolVersion => check_protocol_version(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, message)) => {
            eprintln!("{message}");
            ExitCode::from(code)
        }
    }
}

fn check_protocol_version() -> Result<(), (u8, String)> {
    let binary = find_on_path("protocol").ok_or_else(|| {
        (
            2,
            "protocol is not on PATH; install the compatibility command from the AEP workspace"
                .to_owned(),
        )
    })?;
    let output = Command::new(&binary)
        .arg("--version")
        .output()
        .map_err(|error| {
            (
                2,
                format!("protocol at {} could not run: {error}", binary.display()),
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
                "protocol at {} exited {} while reporting its version: {}",
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
                "protocol at {} printed no semantic version: {:?}",
                binary.display(),
                rendered.trim()
            ),
        )
    })?;
    if version < MINIMUM_PROTOCOL {
        return Err((
            1,
            format!(
                "protocol at {} reports {}; this store needs at least {}",
                binary.display(),
                render_version(version),
                render_version(MINIMUM_PROTOCOL)
            ),
        ));
    }
    println!(
        "protocol {} at {} (needs {})",
        render_version(version),
        binary.display(),
        render_version(MINIMUM_PROTOCOL)
    );
    Ok(())
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

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
    fn protocol_versions_are_extracted_without_accepting_partial_numbers() {
        assert_eq!(parse_version("protocol 0.40.0\n"), Some((0, 40, 0)));
        assert_eq!(parse_version("protocol version forty"), None);
        assert_eq!(parse_version("protocol 0.40"), None);
    }
}
