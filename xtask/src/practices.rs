//! Best practices that are enforced (docs/best-practices.md): every recipe is complete and names
//! tests that exist, every topic of the project map names its recipes, and what was made is under
//! its checks (interface parts in `e2e/layout.mjs`, suites in a release, commands in a contract).

use std::fmt::Write as _;
use std::path::Path;

use crate::topics;

/// One recipe.
#[derive(Debug, Default)]
pub struct Recipe {
    pub id: String,
    pub title: String,
    pub text: String,
    pub tests: Vec<String>,
    fields: Vec<String>,
}

const FIELDS: [&str; 5] = ["read", "do", "never", "proof", "done"];

pub fn parse(text: &str) -> Vec<Recipe> {
    let mut out: Vec<Recipe> = vec![];
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            let (id, title) = h.split_once(' ').unwrap_or((h, ""));
            out.push(Recipe {
                id: id.into(),
                title: title.into(),
                ..Recipe::default()
            });
        }
        let Some(r) = out.last_mut() else { continue };
        r.text.push_str(line);
        r.text.push('\n');
        if let Some((field, rest)) = line.strip_prefix("- ").and_then(|l| l.split_once(": ")) {
            r.fields.push(field.into());
            if field == "proof" {
                r.tests.extend(
                    rest.split('`')
                        .skip(1)
                        .step_by(2)
                        .filter_map(|s| s.strip_prefix("test:"))
                        .map(str::to_string),
                );
            }
        }
    }
    out
}

/// Source files of the interface that open something over or beside the page.
fn parts(dir: &Path, base: &Path, out: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for e in read.flatten() {
        let p = e.path();
        if p.is_dir() {
            parts(&p, base, out);
        } else if p.extension().is_some_and(|x| x == "tsx") {
            let s = std::fs::read_to_string(&p).unwrap_or_default();
            if [
                "<Dialog",
                "<Card",
                "openMenu(",
                "class=\"peek",
                "class=\"palette",
                "ep-card\" role",
            ]
            .iter()
            .any(|m| s.contains(m))
            {
                let rel = p.strip_prefix(base).unwrap_or(&p);
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

/// Everything that is wrong, in words.
pub fn problems(root: &Path) -> anyhow::Result<Vec<String>> {
    let read = |p: &str| -> anyhow::Result<String> {
        std::fs::read_to_string(root.join(p)).map_err(|e| anyhow::anyhow!("{p}: {e}"))
    };
    let recipes = parse(&read("docs/best-practices.md")?);
    let map = topics::parse(&read("docs/project-map.md")?);
    let code = topics::all_code(root);
    let mut wrong = vec![];
    for r in &recipes {
        for f in FIELDS {
            if !r.fields.iter().any(|x| x == f) {
                wrong.push(format!("recipe {}: no `{f}`", r.id));
            }
        }
        if !r.text.contains("\ntags: ") {
            wrong.push(format!("recipe {}: no tags", r.id));
        }
        for t in r.tests.iter().filter(|t| !topics::has_test(&code, t)) {
            wrong.push(format!("recipe {}: test {t} does not exist", r.id));
        }
    }
    for t in &map {
        if t.practice.is_empty() {
            wrong.push(format!("topic #{}: no `practice:` line", t.tag));
        }
        for p in t
            .practice
            .iter()
            .filter(|p| !recipes.iter().any(|r| &&r.id == p))
        {
            wrong.push(format!("topic #{}: no recipe {p}", t.tag));
        }
    }
    // Interface parts are under the layout check.
    let layout = read("e2e/layout.mjs")?;
    let covers: String = layout.lines().filter(|l| l.contains("covers:")).collect();
    let ui = root.join("ui/reader/src");
    let mut found = vec![];
    parts(&ui, &ui, &mut found);
    for f in found.iter().filter(|f| !covers.contains(f.as_str())) {
        wrong.push(format!(
            "ui/reader/src/{f} opens a part that e2e/layout.mjs does not cover"
        ));
    }
    // Commands are in a contract; what the window may call exists.
    let contracts = topics::all_contracts(root);
    let mut names = vec![];
    for e in std::fs::read_dir(root.join("spec/schemas"))?.flatten() {
        let n = e.file_name().to_string_lossy().replace(".json", "");
        if !n.starts_with('_') {
            names.push(n);
        }
    }
    for n in names.iter().filter(|n| {
        !contracts.contains(n.as_str()) && !contracts.contains(n.split('.').nth(1).unwrap_or("?"))
    }) {
        wrong.push(format!("command {n} is in no CONTRACT.md"));
    }
    let allowed: serde_json::Value =
        serde_json::from_str(&read("hosts/reader/src-tauri/allowed-commands.json")?)?;
    for c in allowed["main"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c.as_str())
    {
        if !names.iter().any(|n| n == c) {
            wrong.push(format!("allowed-commands.json: {c} does not exist"));
        }
    }
    // Every suite is known to the map.
    let checks: String = map
        .iter()
        .flat_map(|t| t.checks.clone())
        .collect::<Vec<_>>()
        .join(" ");
    for s in suites(root) {
        if !checks.contains(&format!("e2e/{s}.mjs")) {
            wrong.push(format!("e2e/{s}.mjs is a check of no topic"));
        }
    }
    Ok(wrong)
}

/// End-to-end suites, by name.
pub fn suites(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root.join("e2e"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_string_lossy()
                .strip_suffix(".mjs")
                .map(str::to_string)
        })
        .filter(|n| n != "lib")
        .collect();
    v.sort();
    v
}

/// The `practices` command: check everything, or print one recipe.
pub fn run(root: &Path, args: &[String]) -> anyhow::Result<()> {
    let recipes = parse(&std::fs::read_to_string(
        root.join("docs/best-practices.md"),
    )?);
    if let Some(id) = args.first() {
        let r = recipes
            .iter()
            .find(|r| &r.id == id)
            .ok_or_else(|| anyhow::anyhow!("no recipe {id}"))?;
        print!("{}", r.text);
        return Ok(());
    }
    for r in &recipes {
        println!("{:<9} {}", r.id, r.title);
    }
    let wrong = problems(root)?;
    let mut list = String::new();
    for w in &wrong {
        let _ = write!(list, "\n  - {w}");
    }
    anyhow::ensure!(
        wrong.is_empty(),
        "practices: {} problem(s){list}",
        wrong.len()
    );
    println!("practices: ok ({} recipes)", recipes.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn practices_are_complete_and_proven() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let wrong = problems(&root).unwrap();
        assert_eq!(wrong, [""; 0], "{}", wrong.join("\n"));
    }

    #[test]
    fn a_recipe_is_read_with_its_tests() {
        let r =
            parse("# x\n## popup A pop-up\ntags: a\n- read: b\n- proof: `test:one` `test:two`\n");
        assert_eq!(
            (r[0].id.as_str(), r[0].title.as_str()),
            ("popup", "A pop-up")
        );
        assert_eq!(r[0].tests, ["one", "two"]);
        assert_eq!(r[0].fields, ["read", "proof"]);
    }
}
