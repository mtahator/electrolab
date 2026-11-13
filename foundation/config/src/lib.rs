//! Layered settings (P1.M1.STO.5): defaults → app file → project file → environment.
//!
//! Each later layer overrides keys of the earlier ones (tables merge deeply). Environment
//! variables use a prefix and `__` between levels: `EP_VIEWER__THEME=dark` sets
//! `viewer.theme`. Values from the environment are parsed as TOML when possible (`true`, `12`,
//! `[1, 2]`), otherwise taken as strings.
#![deny(missing_docs)]

/// The product name (a placeholder until decided). Source of truth: `product.toml`;
/// change it with `cargo xtask set-name`. Used for per-user folders and the key store.
pub const PRODUCT: &str = "ElectroPaper";

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use toml::{Table, Value};

/// Settings errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// A settings file could not be read or written.
    #[error("settings file {path}: {source}")]
    Io {
        /// File.
        path: PathBuf,
        /// Cause.
        source: io::Error,
    },
    /// A settings file is not valid TOML.
    #[error("settings file {path}: {source}")]
    Parse {
        /// File.
        path: PathBuf,
        /// Cause.
        source: Box<toml::de::Error>,
    },
    /// The merged settings do not fit the settings type.
    #[error("settings: {0}")]
    Shape(Box<toml::de::Error>),
    /// Settings could not be written as TOML.
    #[error("settings: {0}")]
    Write(Box<toml::ser::Error>),
}

/// Result alias.
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Where each layer comes from.
#[derive(Clone, Debug, Default)]
pub struct Layers {
    /// The app's settings file (per user).
    pub app_file: Option<PathBuf>,
    /// The open project's settings file.
    pub project_file: Option<PathBuf>,
    /// Environment variable prefix, e.g. `EP_`; `None` ignores the environment.
    pub env_prefix: Option<String>,
}

/// Deep-merge `over` into `base`.
fn merge(base: &mut Table, over: Table) {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(Value::Table(b)), Value::Table(o)) => merge(b, o),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

fn read_table(path: &Path) -> Result<Table> {
    match fs::read_to_string(path) {
        Ok(text) => text.parse::<Table>().map_err(|source| ConfigError::Parse {
            path: path.into(),
            source: Box::new(source),
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Table::new()),
        Err(source) => Err(ConfigError::Io {
            path: path.into(),
            source,
        }),
    }
}

/// Turn `PREFIX_A__B=v` pairs into a nested table.
fn env_table(prefix: &str, vars: impl Iterator<Item = (String, String)>) -> Table {
    let mut out = Table::new();
    for (k, v) in vars {
        let Some(rest) = k.strip_prefix(prefix) else {
            continue;
        };
        let path: Vec<String> = rest.split("__").map(str::to_ascii_lowercase).collect();
        if path.iter().any(String::is_empty) {
            continue;
        }
        let value = format!("x = {v}")
            .parse::<Table>()
            .ok()
            .and_then(|mut t| t.remove("x"))
            .unwrap_or(Value::String(v));
        let mut leaf = Table::new();
        let mut parts = path.into_iter().rev();
        if let Some(last) = parts.next() {
            leaf.insert(last, value);
        }
        for p in parts {
            let mut t = Table::new();
            t.insert(p, Value::Table(leaf));
            leaf = t;
        }
        merge(&mut out, leaf);
    }
    out
}

/// Load settings of type `T` from its defaults and the given layers.
///
/// # Errors
/// A file is unreadable or invalid, or the merged result does not fit `T`.
pub fn load<T: Serialize + DeserializeOwned + Default>(layers: &Layers) -> Result<T> {
    load_with_env(layers, std::env::vars())
}

/// [`load`] with an explicit environment (for tests and hosts that filter variables).
///
/// # Errors
/// See [`load`].
pub fn load_with_env<T: Serialize + DeserializeOwned + Default>(
    layers: &Layers,
    env: impl Iterator<Item = (String, String)>,
) -> Result<T> {
    let mut merged = Table::try_from(T::default()).map_err(|e| ConfigError::Write(Box::new(e)))?;
    for f in [&layers.app_file, &layers.project_file]
        .into_iter()
        .flatten()
    {
        merge(&mut merged, read_table(f)?);
    }
    if let Some(prefix) = &layers.env_prefix {
        merge(&mut merged, env_table(prefix, env));
    }
    merged
        .try_into()
        .map_err(|e| ConfigError::Shape(Box::new(e)))
}

/// Write settings to a file atomically (temporary file, then rename).
///
/// # Errors
/// Serialisation or disk error.
pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = toml::to_string_pretty(value).map_err(|e| ConfigError::Write(Box::new(e)))?;
    let io_err = |source| ConfigError::Io {
        path: path.into(),
        source,
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io_err)?;
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(io_err)?;
    fs::rename(&tmp, path).map_err(io_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Viewer {
        theme: String,
        zoom: f64,
        smooth: bool,
    }

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct App {
        language: String,
        viewer: Viewer,
    }

    #[test]
    fn layers_override_in_order() {
        let d = tempfile::tempdir().expect("tmp");
        let app = d.path().join("app.toml");
        let project = d.path().join("project.toml");
        fs::write(
            &app,
            "language = \"fa\"\n[viewer]\ntheme = \"sepia\"\nzoom = 1.25\n",
        )
        .expect("w");
        fs::write(&project, "[viewer]\ntheme = \"dark\"\n").expect("w");
        let layers = Layers {
            app_file: Some(app),
            project_file: Some(project),
            env_prefix: Some("EP_".into()),
        };
        let env = vec![
            ("EP_VIEWER__SMOOTH".to_string(), "true".to_string()),
            ("OTHER".into(), "x".into()),
        ];
        let s: App = load_with_env(&layers, env.into_iter()).expect("load");
        assert_eq!(
            s,
            App {
                language: "fa".into(),
                viewer: Viewer {
                    theme: "dark".into(),
                    zoom: 1.25,
                    smooth: true
                }
            }
        );
    }

    #[test]
    fn missing_files_give_defaults_and_save_round_trips() {
        let d = tempfile::tempdir().expect("tmp");
        let p = d.path().join("nested").join("app.toml");
        let layers = Layers {
            app_file: Some(p.clone()),
            ..Layers::default()
        };
        let s: App = load_with_env(&layers, std::iter::empty()).expect("load");
        assert_eq!(s, App::default());
        let changed = App {
            language: "en".into(),
            viewer: Viewer {
                theme: "light".into(),
                zoom: 2.0,
                smooth: false,
            },
        };
        save(&p, &changed).expect("save");
        let back: App = load_with_env(&layers, std::iter::empty()).expect("load");
        assert_eq!(back, changed);
    }

    #[test]
    fn bad_file_is_reported_with_its_path() {
        let d = tempfile::tempdir().expect("tmp");
        let p = d.path().join("app.toml");
        fs::write(&p, "language = ").expect("w");
        let layers = Layers {
            app_file: Some(p.clone()),
            ..Layers::default()
        };
        let e = load_with_env::<App>(&layers, std::iter::empty()).expect_err("must fail");
        assert!(e.to_string().contains("app.toml"));
    }

    #[test]
    fn env_values_fall_back_to_strings() {
        let t = env_table(
            "EP_",
            vec![("EP_LANGUAGE".to_string(), "fa".to_string())].into_iter(),
        );
        assert_eq!(t.get("language"), Some(&Value::String("fa".into())));
    }
}
