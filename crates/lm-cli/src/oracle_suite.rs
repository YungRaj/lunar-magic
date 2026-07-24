use std::path::Path;

mod case;
mod fixtures;
mod release;

pub use release::{audit_coverage, release_gate};

use case::verify_case;
use fixtures::{discover, relative_name};

const MANIFEST: &str = "case.manifest";
const BEFORE_ROM: &str = "before.smc";
const AFTER_ROM: &str = "after.smc";
const BEFORE_OBSERVATION: &str = "before.obs";
const AFTER_OBSERVATION: &str = "after.obs";
const RENDER_PNG: &str = "render.png";
const EMULATOR_PNG: &str = "emulator.png";
const MAX_DIRECTORIES: usize = 100_000;
const MAX_CASES: usize = 10_000;

pub fn verify(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    verify_with_policy(root, false)
}

fn verify_with_policy(
    root: &Path,
    require_observations: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let cases = discover(root)?;
    if cases.is_empty() {
        return Err("oracle suite contains no case.manifest files".into());
    }
    let mut failures = Vec::new();
    for directory in &cases {
        match verify_case(directory, require_observations) {
            Ok(()) => println!("PASS {}", relative_name(root, directory)),
            Err(error) => {
                println!("FAIL {}: {error}", relative_name(root, directory));
                failures.push(relative_name(root, directory));
            }
        }
    }
    println!("oracle-cases: {}", cases.len());
    println!("oracle-failures: {}", failures.len());
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} oracle case(s) failed: {}",
            failures.len(),
            failures.join(", ")
        )
        .into())
    }
}

#[cfg(test)]
#[path = "oracle_suite_tests.rs"]
mod tests;
