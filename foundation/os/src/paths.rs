//! Path rules (P1.M1.OS.3).
//!
//! - In memory: `PathBuf`, never strings.
//! - In project files: paths relative to the project folder, `/`-separated, NFC-free (kept
//!   exactly as the OS gave them), never `..`.
//! - Canonical paths without the `\\?\` prefix ([`dunce`]) whenever the plain form works, so
//!   they can be shown and passed to other programs; paths over 260 characters keep it.
//! - Two names that differ only in letter case are a collision (Windows and macOS file
//!   systems treat them as one file).

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// A path inside a project, written with `/`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelPath(String);

/// Why a path cannot be stored relative to the project.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelPathError {
    /// Outside the project folder.
    #[error("{0} is outside the project folder")]
    Outside(PathBuf),
    /// Contains `..`, a drive or a root.
    #[error("{0:?} is not a plain relative path")]
    NotPlain(String),
    /// Not valid Unicode (cannot be written to a project file portably).
    #[error("{0} is not valid Unicode")]
    NotUnicode(PathBuf),
}

impl RelPath {
    /// Parse a stored relative path, rejecting `..`, roots and drive letters.
    ///
    /// # Errors
    /// [`RelPathError::NotPlain`].
    pub fn parse(s: &str) -> Result<Self, RelPathError> {
        let bad = s.is_empty()
            || s.starts_with('/')
            || s.contains('\\')
            || s.contains(':')
            || s.split('/').any(|p| p.is_empty() || p == "." || p == "..");
        if bad {
            Err(RelPathError::NotPlain(s.into()))
        } else {
            Ok(Self(s.into()))
        }
    }

    /// Make `path` relative to `root`.
    ///
    /// # Errors
    /// Outside `root`, or not Unicode.
    pub fn from_path(root: &Path, path: &Path) -> Result<Self, RelPathError> {
        // One side may carry the `\\?\` prefix (long paths) and the other not.
        let (root_p, path_p) = (plain(root), plain(path));
        let rel = path_p
            .strip_prefix(&root_p)
            .map_err(|_| RelPathError::Outside(path.into()))?;
        let mut parts = Vec::new();
        for c in rel.components() {
            match c {
                Component::Normal(p) => parts.push(
                    p.to_str()
                        .ok_or_else(|| RelPathError::NotUnicode(path.into()))?
                        .to_string(),
                ),
                Component::CurDir => {}
                _ => return Err(RelPathError::NotPlain(rel.display().to_string())),
            }
        }
        Self::parse(&parts.join("/"))
    }

    /// The path under `root`.
    #[must_use]
    pub fn to_path(&self, root: &Path) -> PathBuf {
        self.0
            .split('/')
            .fold(root.to_path_buf(), |p, part| p.join(part))
    }

    /// The `/` form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The path with a Windows verbatim prefix (`\\?\C:\…`, `\\?\UNC\…`) rewritten to the plain
/// form, for comparing paths only (the result may be too long to open).
fn plain(p: &Path) -> PathBuf {
    match p.to_str() {
        Some(s) if s.starts_with(r"\\?\UNC\") => PathBuf::from(format!(r"\\{}", &s[8..])),
        Some(s) if s.starts_with(r"\\?\") => PathBuf::from(&s[4..]),
        _ => p.to_path_buf(),
    }
}

/// Canonical form without the verbatim prefix.
///
/// # Errors
/// The path does not exist.
pub fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    dunce::canonicalize(path)
}

/// Groups of names that collide when letter case is ignored.
#[must_use]
pub fn case_collisions<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<Vec<&'a str>> {
    let mut by: HashMap<String, Vec<&str>> = HashMap::new();
    for n in names {
        by.entry(n.to_lowercase()).or_default().push(n);
    }
    let mut out: Vec<Vec<&str>> = by.into_values().filter(|v| v.len() > 1).collect();
    for v in &mut out {
        v.sort_unstable();
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rejects_escapes() {
        for bad in ["", "/a", "a/../b", "..", "a//b", "C:/x", r"a\b", "./a"] {
            assert!(RelPath::parse(bad).is_err(), "{bad}");
        }
        let root = Path::new("projects").join("p");
        assert!(RelPath::from_path(&root, Path::new("other/x.pdf")).is_err());
    }

    #[test]
    fn collisions() {
        assert_eq!(
            case_collisions(["Paper.pdf", "paper.PDF", "other.pdf"]),
            vec![vec!["Paper.pdf", "paper.PDF"]]
        );
        assert_eq!(
            case_collisions(["مقاله.pdf", "a.pdf"]),
            Vec::<Vec<&str>>::new()
        );
    }

    fn name() -> impl Strategy<Value = String> {
        // Persian, Latin, spaces, dots and dashes; no separators.
        proptest::string::string_regex("[a-zA-Z0-9 آ-ی۰-۹._-]{1,40}")
            .expect("regex")
            .prop_filter("plain", |s| {
                s != "." && s != ".." && !s.ends_with(' ') && !s.ends_with('.')
            })
    }

    proptest! {
        #[test]
        fn round_trip_odd_names(parts in proptest::collection::vec(name(), 1..6)) {
            let root = std::env::temp_dir().join("ep project");
            let path = parts.iter().fold(root.clone(), |p, s| p.join(s));
            let rel = RelPath::from_path(&root, &path).expect("relative");
            prop_assert_eq!(rel.as_str(), parts.join("/"));
            prop_assert_eq!(RelPath::parse(rel.as_str()).expect("parse"), rel.clone());
            prop_assert_eq!(rel.to_path(&root), path);
        }
    }

    #[test]
    fn long_and_persian_paths_exist_on_disk() {
        let d = tempfile::tempdir().expect("tmp");
        let deep = (0..12).fold(d.path().to_path_buf(), |p, i| {
            p.join(format!("پوشه با فاصله {i} - a fairly long folder name"))
        });
        std::fs::create_dir_all(&deep).expect("mkdir long");
        let f = deep.join("مقاله نهایی.pdf");
        std::fs::write(&f, b"%PDF").expect("write");
        // Over 260 characters the verbatim prefix is kept on purpose: the plain form would fail.
        let c = canonical(&f).expect("canonical");
        let short = d.path().join("پوشه کوتاه");
        std::fs::create_dir_all(&short).expect("mkdir short");
        assert!(
            !canonical(&short)
                .expect("canonical")
                .to_string_lossy()
                .starts_with(r"\\?\")
        );
        let root = canonical(d.path()).expect("root");
        let rel = RelPath::from_path(&root, &c).expect("rel");
        assert_eq!(std::fs::read(rel.to_path(&root)).expect("read"), b"%PDF");
    }
}
