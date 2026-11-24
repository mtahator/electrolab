//! The project map as a tool: which topics a change touches, what to read with them, and which
//! checks to run — only those (docs/project-map.md).
//!
//! - `cargo xtask topics` — every topic with its tags.
//! - `cargo xtask topics <tag>` — one topic, and the topics it connects to.
//! - `cargo xtask topics --changed` — topics whose files differ from the last commit, with their
//!   checks.

use std::path::Path;
use std::process::Command;

/// One topic of the map.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Topic {
    pub tag: String,
    pub title: String,
    pub tags: String,
    pub connects: Vec<String>,
    pub checks: Vec<String>,
    /// Recipes of docs/best-practices.md.
    pub practice: Vec<String>,
    /// Best practices and the tests that prove them.
    pub best: Vec<(String, Vec<String>)>,
    /// Repository paths named in backticks.
    pub paths: Vec<String>,
    pub text: String,
}

const TOP: [&str; 8] = [
    "docs/",
    "engines/",
    "foundation/",
    "hosts/",
    "ui/",
    "xtask/",
    "e2e/",
    "natives.toml",
];

fn backticked(line: &str) -> impl Iterator<Item = &str> {
    line.split('`').skip(1).step_by(2)
}

/// Read the map.
pub fn parse(map: &str) -> Vec<Topic> {
    let mut out: Vec<Topic> = vec![];
    for line in map.lines() {
        if let Some(h) = line.strip_prefix("## #") {
            let (tag, title) = h.split_once(' ').unwrap_or((h, ""));
            out.push(Topic {
                tag: tag.into(),
                title: title.into(),
                ..Topic::default()
            });
        }
        let Some(t) = out.last_mut() else { continue };
        t.text.push_str(line);
        t.text.push('\n');
        if let Some(v) = line.strip_prefix("tags: ") {
            t.tags = v.into();
        } else if let Some(v) = line.strip_prefix("connects: ") {
            t.connects = v
                .split_whitespace()
                .map(|s| s.trim_start_matches('#').into())
                .collect();
        } else if let Some(v) = line.strip_prefix("practice: ") {
            t.practice = v.split_whitespace().map(str::to_string).collect();
        } else if line.starts_with("check: ") {
            t.checks.extend(backticked(line).map(str::to_string));
        } else {
            if let Some(v) = line.strip_prefix("- best: ") {
                let tests = backticked(v)
                    .filter_map(|s| s.strip_prefix("test:"))
                    .map(str::to_string)
                    .collect();
                t.best.push((v.into(), tests));
            }
            t.paths.extend(
                backticked(line)
                    .filter(|s| {
                        TOP.iter().any(|p| s.starts_with(p)) && !s.contains(['<', ' ', '{', '*'])
                    })
                    .map(str::to_string),
            );
        }
    }
    out
}

fn gather(dir: &Path, wanted: &dyn Fn(&Path) -> bool, out: &mut String) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if !matches!(
                name.as_str(),
                "target" | "node_modules" | "dist" | ".git" | "gen"
            ) {
                gather(&p, wanted, out);
            }
        } else if wanted(&p) {
            out.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
        }
    }
}

/// All code in which a test can be defined.
pub fn all_code(root: &Path) -> String {
    let mut code = String::new();
    for d in ["engines", "foundation", "hosts", "xtask", "e2e"] {
        gather(
            &root.join(d),
            &|p| p.extension().is_some_and(|x| x == "rs" || x == "mjs"),
            &mut code,
        );
    }
    code
}

/// All contracts.
pub fn all_contracts(root: &Path) -> String {
    let mut text = String::new();
    gather(
        &root.join("engines"),
        &|p| p.ends_with("CONTRACT.md"),
        &mut text,
    );
    text
}

/// Whether a test of this name is defined (`fn name` in Rust, `check("name"` end to end).
pub fn has_test(code: &str, name: &str) -> bool {
    code.contains(&format!("fn {name}(")) || code.contains(&format!("check(\"{name}\""))
}

/// Topics one of whose paths holds a changed file.
pub fn touched<'a>(topics: &'a [Topic], changed: &[String]) -> Vec<&'a Topic> {
    topics
        .iter()
        .filter(|t| {
            t.paths.iter().any(|p| {
                !p.starts_with("docs/adr")
                    && changed
                        .iter()
                        .any(|c| c == p || c.starts_with(&format!("{}/", p.trim_end_matches('/'))))
            })
        })
        .collect()
}

fn changed_files(root: &Path) -> anyhow::Result<Vec<String>> {
    let git = |args: &[&str]| -> anyhow::Result<String> {
        let out = Command::new("git").args(args).current_dir(root).output()?;
        anyhow::ensure!(out.status.success(), "git {args:?} failed");
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let mut all = git(&["diff", "--name-only", "HEAD"])?;
    all.push_str(&git(&["ls-files", "--others", "--exclude-standard"])?);
    Ok(all.lines().map(str::to_string).collect())
}

/// The `topics` command.
pub fn run(root: &Path, args: &[String]) -> anyhow::Result<()> {
    let map = std::fs::read_to_string(root.join("docs/project-map.md"))?;
    let topics = parse(&map);
    match args.first().map(String::as_str) {
        None => {
            for t in &topics {
                println!("#{:<12} {} — {}", t.tag, t.title, t.tags);
            }
        }
        Some("--changed") => {
            let hit = touched(&topics, &changed_files(root)?);
            let mut checks: Vec<&String> = hit.iter().flat_map(|t| &t.checks).collect();
            checks.sort();
            checks.dedup();
            for t in &hit {
                println!(
                    "#{} — read with: {} — recipes: {}",
                    t.tag,
                    t.connects.join(" "),
                    t.practice.join(" ")
                );
            }
            println!("checks:");
            for c in checks {
                println!("  {c}");
            }
        }
        Some(tag) => {
            let tag = tag.trim_start_matches('#');
            let t = topics
                .iter()
                .find(|t| t.tag == tag)
                .ok_or_else(|| anyhow::anyhow!("no topic #{tag}"))?;
            print!("{}", t.text);
            for c in topics.iter().filter(|o| t.connects.contains(&o.tag)) {
                println!("connected: #{} {} — {}", c.tag, c.title, c.tags);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = "# Map\n## #a First\ntags: one\nconnects: #b\n- `engines/a/src/lib.rs` does it\n- best: keep it — `test:stays`\ncheck: `cargo test -p a`\n## #b Second\ntags: two\nconnects: #a\n- `ui/b`\ncheck: `npm run b`\n";

    #[test]
    fn topics_are_read_from_the_map() {
        let t = parse(MAP);
        assert_eq!(t.len(), 2);
        assert_eq!(
            (t[0].tag.as_str(), t[0].title.as_str(), t[0].tags.as_str()),
            ("a", "First", "one")
        );
        assert_eq!(t[0].connects, ["b"]);
        assert_eq!(t[0].checks, ["cargo test -p a"]);
        assert_eq!(t[0].best[0].1, ["stays"]);
        assert_eq!(t[0].paths, ["engines/a/src/lib.rs"]);
    }

    #[test]
    fn only_the_topics_of_changed_files_are_checked() {
        let t = parse(MAP);
        let tags = |files: &[&str]| -> Vec<String> {
            let files: Vec<String> = files.iter().map(|f| (*f).to_string()).collect();
            touched(&t, &files).iter().map(|t| t.tag.clone()).collect()
        };
        assert_eq!(tags(&["engines/a/src/lib.rs"]), ["a"]);
        assert_eq!(tags(&["ui/b/x.ts"]), ["b"]);
        assert_eq!(tags(&["ui/bc/x.ts", "README.md"]), [""; 0]);
    }

    /// The map is searched and trusted, so it must stay true: paths exist, connections exist, every
    /// topic says how to check it, every best practice names a test and that test exists.
    #[test]
    fn project_map_is_current() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let topics = parse(&std::fs::read_to_string(root.join("docs/project-map.md")).unwrap());
        let code = all_code(&root);
        let mut wrong = vec![];
        for t in &topics {
            if t.tags.is_empty()
                || t.connects.is_empty()
                || t.checks.is_empty()
                || t.best.is_empty()
            {
                wrong.push(format!("#{}: needs tags, connects, best and check", t.tag));
            }
            for c in t
                .connects
                .iter()
                .filter(|c| !topics.iter().any(|o| &&o.tag == c))
            {
                wrong.push(format!("#{}: connects to unknown #{c}", t.tag));
            }
            for p in t.paths.iter().filter(|p| !root.join(p).exists()) {
                wrong.push(format!("#{}: {p} is not there", t.tag));
            }
            for (text, tests) in &t.best {
                if tests.is_empty() {
                    wrong.push(format!("#{}: no test for: {text}", t.tag));
                }
                for n in tests.iter().filter(|n| !has_test(&code, n)) {
                    wrong.push(format!("#{}: test {n} does not exist", t.tag));
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
