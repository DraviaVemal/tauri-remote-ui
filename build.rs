// AGPL-3.0-only License
// Copyright (c) 2025 DraviaVemal
// See LICENSE file in the root directory.

use std::process::Command;
use std::{env, fs};

const COMMANDS: &[&str] = &[];

/// Entry point for the build script.
fn main() {
    if env::var("DEVOPS_BUILD").unwrap_or_default() == "1" {
        Command::new("git")
            .args(["fetch", "--tags", "--force"])
            .output()
            .expect("Failed to pull latest tags");
        // Get the latest Git tag matching "v*"
        let output = Command::new("git")
            .args(["describe", "--tags", "--match", "v*", "--abbrev=0"])
            .output()
            .expect("Failed to get latest Git tag");

        if !output.status.success() {
            panic!("Error retrieving Git tag");
        }

        let version = String::from_utf8(output.stdout)
            .unwrap()
            .trim()
            .replace('v', "");

        // Set as environment variable for Rust
        println!("cargo:rustc-env=GIT_VERSION={}", version);

        // Overwrite version in Cargo.toml based on Git tag
        // This ensures the package version always matches the latest Git tag
        // when building in a CI/CD environment (DEVOPS_BUILD=1)
        let cargo_toml_path = "Cargo.toml";
        let cargo_toml = fs::read_to_string(cargo_toml_path).expect("Failed to read Cargo.toml");
        let mut updated_cargo_toml = cargo_toml
            .lines()
            .map(|line| {
                if line.starts_with("version =") {
                    // Replace the version line with the one from Git tag
                    format!("version = \"{}\"", version)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        updated_cargo_toml.push('\n'); // Ensure file ends with newline
        fs::write(cargo_toml_path, updated_cargo_toml).expect("Failed to update Cargo.toml");

        println!("Updated Cargo.toml version to {}", version);
    }

    tauri_plugin::Builder::new(COMMANDS).build();
}
