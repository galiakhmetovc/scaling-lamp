use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../cmd");
    println!("cargo:rerun-if-changed=../../crates");
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");
    println!("cargo:rerun-if-changed=../../.git/index");

    let commit = git_commit_short().unwrap_or_else(|| "unknown".to_string());
    let tree_state = if git_tree_is_dirty() {
        "dirty"
    } else {
        "clean"
    };
    let build_id = build_id();
    println!("cargo:rustc-env=AGENTD_GIT_COMMIT={commit}");
    println!("cargo:rustc-env=AGENTD_GIT_TREE_STATE={tree_state}");
    println!("cargo:rustc-env=AGENTD_BUILD_ID={build_id}");
}

fn build_id() -> String {
    match std::env::var("SOURCE_DATE_EPOCH") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "unknown".to_string()),
    }
}

fn git_commit_short() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8(output.stdout).ok()?;
    let commit = commit.trim();
    if commit.is_empty() {
        None
    } else {
        Some(commit.to_string())
    }
}

fn git_tree_is_dirty() -> bool {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output();
    match output {
        Ok(output) if output.status.success() => !output.stdout.is_empty(),
        _ => false,
    }
}
