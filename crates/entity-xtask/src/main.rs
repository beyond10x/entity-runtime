//! Repository-only checks that do not belong in the public `entity` command.

use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
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
    /// Compare the pinned lifecycle fixtures with an AEP checkout.
    UpstreamPin {
        /// Path to an AEP checkout.
        checkout: PathBuf,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Check::ProtocolVersion => check_protocol_version(),
        Check::UpstreamPin { checkout } => check_upstream_pin(&checkout),
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

fn check_upstream_pin(checkout: &Path) -> Result<(), (u8, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("xtask is a workspace member");
    let fixture = root.join("crates/entity-yaml/tests/fixtures/aep-lifecycles");
    let upstream = checkout.join("artifacts/lifecycles");
    if !upstream.is_dir() {
        return Err((
            2,
            format!(
                "{} is not a directory: that checkout is not AEP",
                upstream.display()
            ),
        ));
    }
    let here = ladders_in(&fixture).map_err(|message| (2, message))?;
    let there = ladders_in(&upstream).map_err(|message| (2, message))?;
    let mut findings = Vec::new();
    for name in here
        .keys()
        .chain(there.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        match (here.get(name), there.get(name)) {
            (Some(_), None) => findings.push(format!(
                "`{name}` is pinned here and gone upstream — a kind was retired"
            )),
            (None, Some(_)) => findings.push(format!(
                "`{name}` is a ladder upstream ships and nothing here pins"
            )),
            (Some(left), Some(right)) if left != right => {
                findings.push(format!("`{name}` differs — the rungs moved upstream"));
            }
            _ => {}
        }
    }
    let pinned = pinned_commit(&fixture.join("PIN.md"));
    let head = upstream_head(checkout);
    println!(
        "{} pinned, {} upstream, {} finding(s); pinned at {pinned}, upstream at {head}",
        here.len(),
        there.len(),
        findings.len()
    );
    for finding in &findings {
        println!("  {finding}");
    }
    if findings.is_empty() {
        Ok(())
    } else {
        Err((
            1,
            "Refresh the pinned ladders, PIN.md, and matching examples, then run `task check`."
                .to_owned(),
        ))
    }
}

fn ladders_in(directory: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut pending = vec![directory.to_path_buf()];
    let mut found = BTreeMap::new();
    while let Some(current) = pending.pop() {
        let mut entries = fs::read_dir(&current)
            .map_err(|error| format!("reading {}: {error}", current.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("reading {}: {error}", current.display()))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("yaml" | "yml")
            ) {
                let name = path
                    .strip_prefix(directory)
                    .expect("walk stays below its root")
                    .to_string_lossy()
                    .into_owned();
                let bytes = fs::read(&path)
                    .map_err(|error| format!("reading {}: {error}", path.display()))?;
                found.insert(name, bytes);
            }
        }
    }
    Ok(found)
}

fn pinned_commit(pin: &Path) -> String {
    fs::read_to_string(pin)
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.contains("| pinned commit"))
                .and_then(|line| line.split('`').nth(1))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "unrecorded".to_owned())
}

fn upstream_head(checkout: &Path) -> String {
    let commit =
        git_output(checkout, &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let tag = git_output(checkout, &["describe", "--tags", "--exact-match", "HEAD"]);
    match tag {
        Some(tag) => format!("{commit} (tagged {tag})"),
        None => format!("{commit} (no tag on this commit)"),
    }
}

fn git_output(checkout: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new(OsString::from("git"))
        .arg("-C")
        .arg(checkout)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
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

    #[test]
    fn the_recorded_pin_is_read_from_the_pin_table() {
        let path = env::temp_dir().join(format!("entity-xtask-pin-{}", std::process::id()));
        fs::write(&path, "| pinned commit | `abcdef0` |\n").expect("write fixture");
        assert_eq!(pinned_commit(&path), "abcdef0");
        fs::remove_file(path).expect("remove fixture");
    }
}
