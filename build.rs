use std::process::Command;

fn main() {
    // Get git SHA
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();

    let git_sha = if output.status.success() {
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    } else {
        "unknown".to_string()
    };

    // Get git short SHA (first 7 chars)
    let git_sha_short = if git_sha.len() >= 7 {
        git_sha[..7].to_string()
    } else {
        git_sha.clone()
    };

    // Set environment variables for the build
    println!("cargo:rustc-env=GIT_SHA={git_sha}");
    println!("cargo:rustc-env=GIT_SHA_SHORT={git_sha_short}");

    // Re-run if HEAD changes
    println!("cargo:rerun-if-changed=.git/HEAD");
}
