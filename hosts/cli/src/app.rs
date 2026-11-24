//! Builder and wiring for the CLI host: the engines this host offers, composed on one bus.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ep_bus::{Bus, CommandError, Envelope};
use ep_document::{BlobAccess, Document};
use ep_document_contract::PdfMetadata;
use ep_types::Hash;
use ep_workspace::Workspace;
use ep_workspace_contract::{Added, Paper, commands as ws};
use serde_json::{Value, json};

/// The Document engine reads managed copies from the open library.
struct WorkspaceBlobs(Arc<Workspace>);

impl BlobAccess for WorkspaceBlobs {
    fn path_of(&self, hash: &Hash) -> Option<PathBuf> {
        self.0.blob_path(hash)
    }
    fn put(
        &self,
        bytes: &[u8],
        media_type: ep_types::MediaType,
    ) -> std::io::Result<ep_types::BlobRef> {
        self.0.store_blob(bytes, media_type)
    }
    fn project(&self) -> Option<(String, PathBuf)> {
        self.0
            .project_folder()
            .map(|(id, dir)| (id.to_string(), dir))
    }
}

pub struct App {
    pub bus: Arc<Bus>,
}

/// App folders: `EP_DATA_DIR` (portable install, tests) or the user's standard folders.
fn dirs() -> anyhow::Result<Arc<dyn ep_os::AppDirs>> {
    match std::env::var_os("EP_DATA_DIR") {
        Some(root) => Ok(Arc::new(ep_os::RootDirs(PathBuf::from(root)))),
        None => os_dirs(),
    }
}

#[cfg(windows)]
fn os_dirs() -> anyhow::Result<Arc<dyn ep_os::AppDirs>> {
    Ok(Arc::new(ep_os_windows::WinDirs::new(ep_config::PRODUCT)?))
}

#[cfg(not(windows))]
fn os_dirs() -> anyhow::Result<Arc<dyn ep_os::AppDirs>> {
    // macOS and Linux adapters arrive in Phase 3; until then use a folder in the home directory.
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    Ok(Arc::new(ep_os::RootDirs(home.join(".electropaper"))))
}

impl App {
    pub fn build() -> anyhow::Result<Self> {
        Self::with_dirs(dirs()?)
    }

    pub fn with_dirs(dirs: Arc<dyn ep_os::AppDirs>) -> anyhow::Result<Self> {
        let bus = Arc::new(Bus::default());
        let renders = dirs.cache().join("renders");
        let workspace = Workspace::new(dirs);
        workspace.register(&bus)?;
        Document::new(
            Arc::new(WorkspaceBlobs(workspace)),
            ep_document::find_pdfium().and_then(|d| ep_document::start_pdfium(&d).ok()),
            Some(&renders),
        )?
        .register(&bus)?;
        // Language models: `EP_LLM_DIR` only; the builds of `.natives` above the program.
        let llm = std::env::var_os("EP_LLM_DIR").map(std::path::PathBuf::from);
        ep_inference::Inference::new(
            llm.into_iter().collect(),
            ep_inference::runtime_dirs(),
            spawner()?,
            None,
            None,
        )
        .register(&bus)?;
        ep_understanding::Understanding::new().register(&bus)?;
        Ok(Self { bus })
    }

    /// Open the project (or the default library) unless the command itself opens one.
    pub async fn open(&self, project: Option<&Path>, command: &str) -> Result<(), CommandError> {
        let opens_itself = [
            ws::CREATE_PROJECT,
            ws::OPEN_PROJECT,
            ws::OPEN_DEFAULT_LIBRARY,
            ws::CLASSIFY,
        ]
        .contains(&command);
        if opens_itself {
            return Ok(());
        }
        let env = match project {
            Some(p) => Envelope::new(ws::OPEN_PROJECT, json!({ "folder": p })),
            None => Envelope::new(ws::OPEN_DEFAULT_LIBRARY, Value::Null),
        };
        self.bus.call(env).await.map(|_| ())
    }

    /// Add a PDF, then read and record its metadata (the host chains the two engines).
    pub async fn import(&self, file: &Path) -> Result<Paper, CommandError> {
        let added: Added = typed(
            self.bus
                .call(Envelope::new(ws::ADD_FILE, json!({ "path": file })))
                .await?,
        )?;
        if added.already_present && added.paper.metadata_source.is_some() {
            return Ok(added.paper);
        }
        let meta: PdfMetadata = typed(
            self.bus
                .call(Envelope::new(
                    ep_document_contract::commands::METADATA,
                    json!({ "file": added.paper.file }),
                ))
                .await?,
        )?;
        let input = json!({
            "id": added.paper.id,
            "title": meta.title,
            "authors": if meta.authors.is_empty() { Value::Null } else { json!(meta.authors) },
            "doi": meta.doi,
            "pages": meta.pages,
            "source": "pdf-info",
        });
        typed(
            self.bus
                .call(Envelope::new(ws::SET_PAPER_METADATA, input))
                .await?,
        )
    }
}

fn typed<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, CommandError> {
    serde_json::from_value(v).map_err(|e| CommandError::failed("output", e))
}

/// One file per command: `<dir>/<command>.json` with input and output schemas.
pub fn write_schemas(bus: &Bus, dir: &Path) -> anyhow::Result<usize> {
    std::fs::create_dir_all(dir)?;
    let commands = bus.commands();
    for c in &commands {
        let doc = json!({
            "command": c.name,
            "summary": c.summary,
            "stability": c.stability,
            "permission": c.permission,
            "streaming": c.streaming,
            "input": c.input_schema,
            "output": c.output_schema,
        });
        std::fs::write(
            dir.join(format!("{}.json", c.name)),
            serde_json::to_string_pretty(&doc)? + "\n",
        )?;
    }
    // Types every caller needs: the envelope, errors and command descriptions.
    let bus = json!({ "types": {
        "Envelope": schemars::schema_for!(ep_bus::Envelope),
        "CommandError": schemars::schema_for!(ep_bus::CommandError),
        "CommandInfo": schemars::schema_for!(ep_bus::CommandInfo),
    }});
    std::fs::write(
        dir.join("_bus.json"),
        serde_json::to_string_pretty(&bus)? + "\n",
    )?;
    Ok(commands.len())
}

/// Starts helper processes tied to this program's lifetime (ADR-009).
#[cfg(windows)]
fn spawner() -> anyhow::Result<Arc<dyn ep_os::ProcessSpawner>> {
    Ok(Arc::new(ep_os_windows::JobSpawner::new()?))
}

#[cfg(not(windows))]
#[allow(clippy::unnecessary_wraps)] // same signature on every system
fn spawner() -> anyhow::Result<Arc<dyn ep_os::ProcessSpawner>> {
    Ok(Arc::new(ep_os::fake::FakeSpawner::default()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[tokio::test]
    async fn import_chains_add_and_metadata() {
        let d = tempfile::tempdir().expect("tmp");
        let app =
            App::with_dirs(Arc::new(ep_os::fake::FakeDirs(d.path().to_path_buf()))).expect("app");
        app.open(None, ws::ADD_FILE).await.expect("default library");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/papers/fawiki-artificial-neural-network.pdf");
        let p = app.import(&fixture).await.expect("import");
        assert_eq!(p.metadata_source.as_deref(), Some("pdf-info"));
        assert!(p.pages.is_some_and(|n| n > 0));
        let again = app.import(&fixture).await.expect("again");
        assert_eq!(again.id, p.id);
        let n = write_schemas(&app.bus, &d.path().join("schemas")).expect("schemas");
        assert!(n >= 10);
    }
}
