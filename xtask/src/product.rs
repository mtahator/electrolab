//! The product name lives in `product.toml`; every place that shows it is derived from it.
//!
//! `cargo xtask set-name "<Name>"` rewrites all of them; `check` (run by `arch-check`) fails when
//! one disagrees. A place is a file plus the one line that starts with a given prefix.

use std::path::Path;

/// (file, line prefix, line template). `{name}` is the name, `{slug}` its lowercase letters and
/// digits only (used in identifiers).
const PLACES: &[(&str, &str, &str)] = &[
    ("product.toml", "name =", "name = \"{name}\""),
    (
        "foundation/config/src/lib.rs",
        "pub const PRODUCT: &str =",
        "pub const PRODUCT: &str = \"{name}\";",
    ),
    (
        "hosts/reader/src-tauri/tauri.conf.json",
        "\"productName\":",
        "\"productName\": \"{name} Reader\",",
    ),
    (
        "hosts/reader/src-tauri/tauri.conf.json",
        "\"identifier\":",
        "\"identifier\": \"app.{slug}.reader\",",
    ),
    (
        "hosts/reader/src-tauri/tauri.conf.json",
        "\"title\":",
        "\"title\": \"{name} Reader\",",
    ),
    (
        "ui/kit/src/i18n.ts",
        "\"app.title\":",
        "\"app.title\": \"{name} Reader\",",
    ),
    (
        "ui/reader/index.html",
        "<title>",
        "<title>{name} Reader</title>",
    ),
];

fn current(root: &Path) -> anyhow::Result<String> {
    let text = std::fs::read_to_string(root.join("product.toml"))?;
    let v: toml::Table = text.parse()?;
    v.get("name")
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("product.toml has no name"))
}

fn render(template: &str, name: &str) -> String {
    let slug: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    template.replace("{name}", name).replace("{slug}", &slug)
}

/// Lines of `text` (with their indentation) whose trimmed form starts with `prefix`.
fn find<'a>(text: &'a str, prefix: &str) -> Vec<(usize, &'a str)> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with(prefix))
        .collect()
}

/// Every place shows the name from `product.toml`.
pub fn check(root: &Path) -> anyhow::Result<Vec<String>> {
    let name = current(root)?;
    let mut problems = vec![];
    for (file, prefix, template) in PLACES {
        let text = std::fs::read_to_string(root.join(file))?;
        let hits = find(&text, prefix);
        let want = render(template, &name);
        match hits.as_slice() {
            [(_, line)] if line.trim() == want => {}
            [(n, line)] => problems.push(format!("{file}:{}: `{}` should be `{want}` (product.toml says {name:?}; run `cargo xtask set-name`)", n + 1, line.trim())),
            _ => problems.push(format!("{file}: expected exactly one line starting with `{prefix}`, found {}", hits.len())),
        }
    }
    Ok(problems)
}

/// Rename the product everywhere.
pub fn set_name(root: &Path, name: &str) -> anyhow::Result<()> {
    let name = name.trim();
    if name.is_empty() || name.contains('"') || !name.chars().any(|c| c.is_ascii_alphanumeric()) {
        anyhow::bail!("give a name with letters or digits and no quotes");
    }
    let old = current(root)?;
    for (file, prefix, template) in PLACES {
        let path = root.join(file);
        let text = std::fs::read_to_string(&path)?;
        let hits = find(&text, prefix);
        let [(n, line)] = hits.as_slice() else {
            anyhow::bail!("{file}: expected exactly one line starting with `{prefix}`");
        };
        let indent = &line[..line.len() - line.trim_start().len()];
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        lines[*n] = format!("{indent}{}", render(template, name));
        let mut out = lines.join("\n");
        if text.ends_with('\n') {
            out.push('\n');
        }
        std::fs::write(&path, out)?;
    }
    println!("renamed {old:?} -> {name:?} in {} places", PLACES.len());
    println!(
        "note: the per-user data folder follows the name; an existing library under the old name is not moved"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_and_templates() {
        assert_eq!(
            render("app.{slug}.reader", "Paper Lab 2"),
            "app.paperlab2.reader"
        );
        assert_eq!(
            render("<title>{name} Reader</title>", "X"),
            "<title>X Reader</title>"
        );
        assert_eq!(find("  \"title\": \"a\",\n\"x\": 1", "\"title\":").len(), 1);
    }
}
