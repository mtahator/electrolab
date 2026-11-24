//! `cargo xtask release`: every check, in order; the installer only after the last one
//! (docs/best-practices.md, *release*). The report goes to `target/release-report.md`.
//!
//! `--from <step>` continues at a step after a fix of the machine (not of the code).

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use crate::practices;

/// A step: its name and what is run.
pub struct Step {
    pub name: String,
    pub program: &'static str,
    pub args: Vec<String>,
}

fn step(name: &str, program: &'static str, args: &str) -> Step {
    Step {
        name: name.into(),
        program,
        args: args.split_whitespace().map(str::to_string).collect(),
    }
}

const APP: &str = "target/release/electropaper-reader.exe";

/// Every step of a release, in order.
pub fn steps(root: &Path) -> Vec<Step> {
    let pnpm = if cfg!(windows) { "pnpm.cmd" } else { "pnpm" };
    let mut v = vec![
        step("format", "cargo", "fmt --all --check"),
        step("architecture", "cargo", "xtask arch-check"),
        step("practices", "cargo", "xtask practices"),
        step("schemas", "cargo", "xtask schema-check"),
        step("licences", "cargo", "deny check"),
        step("unused dependencies", "cargo", "machete"),
        step(
            "lint",
            "cargo",
            "clippy -q --workspace --all-targets -- -D warnings",
        ),
        step("tests", "cargo", "test -q --workspace"),
        step("ui types", pnpm, "typecheck"),
        step("ui style", pnpm, "lint"),
        step("ui tests", pnpm, "test"),
        step(
            "app",
            pnpm,
            "--filter @electropaper/reader-host tauri build --no-bundle",
        ),
    ];
    for s in practices::suites(root) {
        v.push(Step {
            name: format!("e2e {s}"),
            program: "node",
            args: vec![
                format!("e2e/{s}.mjs"),
                APP.into(),
                "target/release-check".into(),
                "target/release-check/out".into(),
            ],
        });
    }
    v.push(step(
        "installer",
        pnpm,
        "--filter @electropaper/reader-host tauri build",
    ));
    v
}

/// Run the steps until one fails; the report and whether all passed.
pub fn run_steps(steps: &[Step], mut run: impl FnMut(&Step) -> bool) -> (String, bool) {
    let mut report = String::from("| Step | Result | Time |\n|---|---|---|\n");
    for s in steps {
        println!("release: {} …", s.name);
        let t = Instant::now();
        let ok = run(s);
        let secs = t.elapsed().as_secs();
        let result = if ok { "passed" } else { "**failed**" };
        let _ = writeln!(report, "| {} | {result} | {secs} s |", s.name);
        if !ok {
            return (report, false);
        }
    }
    (report, true)
}

fn git(root: &Path, args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("git").args(args).current_dir(root).output()?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The `release` command.
pub fn run(root: &Path, args: &[String]) -> anyhow::Result<()> {
    let dirty = git(root, &["status", "--porcelain"])?;
    anyhow::ensure!(
        dirty.is_empty(),
        "commit first: an installer is only built from committed code\n{dirty}"
    );
    let commit = git(root, &["rev-parse", "--short", "HEAD"])?;
    let mut steps = steps(root);
    if let [flag, from] = args
        && flag == "--from"
    {
        let i = steps
            .iter()
            .position(|s| &s.name == from)
            .ok_or_else(|| anyhow::anyhow!("no step {from:?}"))?;
        steps.drain(..i);
    }
    let (table, ok) = run_steps(&steps, |s| {
        Command::new(s.program)
            .args(&s.args)
            .current_dir(root)
            .status()
            .is_ok_and(|x| x.success())
    });
    std::fs::write(
        root.join("target/release-report.md"),
        format!("# Release check of {commit}\n\n{table}"),
    )?;
    println!("{table}");
    anyhow::ensure!(
        ok,
        "release: stopped; no installer was built (target/release-report.md)"
    );
    println!("release: ok, commit {commit}; the installer is in target/release/bundle/nsis");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_stops_at_the_first_failure() {
        let steps = [
            step("a", "x", ""),
            step("b", "x", ""),
            step("installer", "x", ""),
        ];
        let mut ran = vec![];
        let (report, ok) = run_steps(&steps, |s| {
            ran.push(s.name.clone());
            s.name != "b"
        });
        assert!(!ok);
        assert_eq!(ran, ["a", "b"], "no installer after a failure");
        assert!(report.contains("| b | **failed** |"));
    }

    #[test]
    fn every_suite_is_part_of_a_release() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let names: Vec<String> = steps(&root).into_iter().map(|s| s.name).collect();
        for s in practices::suites(&root) {
            assert!(names.contains(&format!("e2e {s}")), "{s}");
        }
        for must in ["practices", "lint", "tests", "architecture", "licences"] {
            assert!(names.iter().any(|n| n == must), "{must}");
        }
        assert_eq!(names.last().map(String::as_str), Some("installer"));
    }
}
