use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

// AI-FUNC-SUMMARY:
// Purpose: Run a git command in the crate directory and return its trimmed stdout.
// Inputs: git arguments.
// Returns: Some(stdout) when git exists and exited zero, None otherwise.
// Side effects: Spawns a git process.
// Notes: Never panics; a checkout without git, without .git, or without any commit yields None.
fn git_output(args: &[&str]) -> Option<String> {
    let dir = env::var("CARGO_MANIFEST_DIR").ok()?;
    let out = Command::new("git")
        .args(["-C", &dir])
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}

// AI-FUNC-SUMMARY: Emit a cargo:rerun-if-changed line only when the path exists; returns nothing; side effects: prints to stdout.
fn rerun_if_exists(path: &Path) {
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Emit every rerun trigger that can change the recorded build identity.
// Inputs: None (reads CARGO_MANIFEST_DIR).
// Returns: None.
// Side effects: Prints cargo directives.
// Notes: The .git entries are guarded on existence because a worktree or submodule checkout has .git as a file.
fn emit_rerun_triggers() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    for rel in ["build.rs", "Cargo.toml", "Cargo.lock", "src"] {
        rerun_if_exists(&root.join(rel));
    }
    let git = root.join(".git");
    if git.is_dir() {
        for rel in ["HEAD", "index", "packed-refs"] {
            rerun_if_exists(&git.join(rel));
        }
        if let Ok(head) = std::fs::read_to_string(git.join("HEAD")) {
            if let Some(reference) = head.trim().strip_prefix("ref: ") {
                rerun_if_exists(&git.join(reference));
            }
        }
    }
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
}

// AI-FUNC-SUMMARY:
// Purpose: List the cargo features enabled for this build, sorted.
// Inputs: None (reads CARGO_FEATURE_* environment variables).
// Returns: Sorted feature names in cargo spelling (lowercase, hyphenated).
// Side effects: None.
// Notes: Must read the environment, not cfg!(feature = ...): a build script compiles without the crate's features.
fn enabled_features() -> Vec<String> {
    let mut names: Vec<String> = env::vars()
        .filter_map(|(key, _)| {
            key.strip_prefix("CARGO_FEATURE_")
                .map(|name| name.to_lowercase().replace('_', "-"))
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

// AI-FUNC-SUMMARY:
// Purpose: Record the git commit, worktree cleanliness, enabled features and build platform as compile-time environment variables.
// Inputs: None.
// Returns: None.
// Side effects: Prints cargo:rustc-env and cargo:rerun-if-changed directives.
// Notes: Every value is emitted as a possibly-empty string; an empty value means "not determined" and maps to None in src/version.rs.
fn main() {
    emit_rerun_triggers();

    let commit = git_output(&["rev-parse", "HEAD"]).unwrap_or_default();
    let dirty = if commit.is_empty() {
        String::new()
    } else {
        match git_output(&["status", "--porcelain"]) {
            Some(status) if status.is_empty() => "false".to_string(),
            Some(_) => "true".to_string(),
            None => String::new(),
        }
    };

    println!("cargo:rustc-env=RUSTMSPT_GIT_COMMIT={commit}");
    println!("cargo:rustc-env=RUSTMSPT_GIT_DIRTY={dirty}");
    println!("cargo:rustc-env=RUSTMSPT_FEATURES={}", enabled_features().join(","));
    println!(
        "cargo:rustc-env=RUSTMSPT_BUILD_TARGET={}",
        env::var("TARGET").unwrap_or_default()
    );
    println!(
        "cargo:rustc-env=RUSTMSPT_BUILD_HOST={}",
        env::var("HOST").unwrap_or_default()
    );
    println!(
        "cargo:rustc-env=RUSTMSPT_BUILD_PROFILE={}",
        env::var("PROFILE").unwrap_or_default()
    );
    println!(
        "cargo:rustc-env=RUSTMSPT_SOURCE_DATE_EPOCH={}",
        env::var("SOURCE_DATE_EPOCH").unwrap_or_default()
    );
}
