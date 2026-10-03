//! Bind the checker executable to the sources it actually compiled.
#[path = "src/identity.rs"]
mod identity;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").ok_or("missing manifest directory")?,
    )
    .join("../..")
    .canonicalize()?;
    for name in ["core", "store", "executor", "shell", "query", "eventlog"] {
        println!(
            "cargo:rerun-if-changed={}",
            root.join(format!("crates/entity-{name}/src")).display()
        );
    }
    println!("cargo:rerun-if-changed=src");
    let digest = identity::source_identity(&root, |path| {
        println!("cargo:rerun-if-changed={}", path.display())
    })?;
    println!("cargo:rustc-env=ER_ESS_SOURCE_IDENTITY={digest}");
    Ok(())
}
