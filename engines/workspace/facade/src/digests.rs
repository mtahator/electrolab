//! What other engines keep with the papers (digests, what was seen in figures): one JSON file per paper
//! in `digests/` and `figures/` of the library folder, so they travel with it and can be read by other
//! programs.

use std::path::PathBuf;
use std::sync::Arc;

use ep_bus::{CommandError, Cx};
use ep_store::rusqlite::{OptionalExtension, params};
use ep_types::PaperId;
use ep_workspace_contract::{GetDigests, MAX_DIGEST, PaperDigest, errors};

use super::{Workspace, sql_err};

/// Where they are kept.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Kept {
    Digests,
    Figures,
}

impl Kept {
    /// Both, for what is done to all of them.
    pub(crate) const ALL: [Self; 2] = [Self::Digests, Self::Figures];

    /// Folder in the library.
    pub(crate) fn folder(self) -> &'static str {
        match self {
            Self::Digests => "digests",
            Self::Figures => "figures",
        }
    }
}

impl Workspace {
    fn kept_dir(&self, kept: Kept) -> Result<PathBuf, CommandError> {
        self.project_folder()
            .map(|(_, dir)| dir.join(kept.folder()))
            .ok_or_else(|| CommandError::failed(errors::NO_PROJECT, "open a library first"))
    }

    pub(crate) async fn save_digest(
        self: Arc<Self>,
        _cx: Cx,
        i: PaperDigest,
    ) -> Result<(), CommandError> {
        self.keep(Kept::Digests, i).await
    }

    pub(crate) async fn save_figures(
        self: Arc<Self>,
        _cx: Cx,
        i: PaperDigest,
    ) -> Result<(), CommandError> {
        self.keep(Kept::Figures, i).await
    }

    pub(crate) async fn get_digests(
        self: Arc<Self>,
        _cx: Cx,
        i: GetDigests,
    ) -> Result<Vec<PaperDigest>, CommandError> {
        self.kept(Kept::Digests, i).await
    }

    pub(crate) async fn get_figures(
        self: Arc<Self>,
        _cx: Cx,
        i: GetDigests,
    ) -> Result<Vec<PaperDigest>, CommandError> {
        self.kept(Kept::Figures, i).await
    }

    async fn keep(&self, kept: Kept, i: PaperDigest) -> Result<(), CommandError> {
        let o = self.require()?;
        let known =
            o.db.read(|c| {
                c.query_row(
                    "SELECT 1 FROM paper WHERE id = ?1",
                    params![i.paper.to_string()],
                    |_| Ok(()),
                )
                .optional()
            })
            .map_err(sql_err)?;
        if known.is_none() {
            return Err(CommandError::failed(
                errors::NOT_FOUND,
                format!("paper {}", i.paper),
            ));
        }
        let text = serde_json::to_vec_pretty(&i.data).map_err(|e| CommandError::InvalidInput {
            message: e.to_string(),
        })?;
        if text.len() > MAX_DIGEST {
            return Err(CommandError::InvalidInput {
                message: format!("a digest has at most {MAX_DIGEST} bytes"),
            });
        }
        let dir = self.kept_dir(kept)?;
        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&dir)?;
            let (part, file) = (
                dir.join(format!("{}.part", i.paper)),
                dir.join(format!("{}.json", i.paper)),
            );
            std::fs::write(&part, text)?;
            std::fs::rename(part, file)
        })
        .await
        .map_err(|e| CommandError::failed(errors::IO, e))?
        .map_err(|e| CommandError::failed(errors::IO, e))
    }

    async fn kept(&self, kept: Kept, i: GetDigests) -> Result<Vec<PaperDigest>, CommandError> {
        self.require()?;
        let dir = self.kept_dir(kept)?;
        tokio::task::spawn_blocking(move || {
            let read = |paper: PaperId| -> Option<PaperDigest> {
                let text = std::fs::read(dir.join(format!("{paper}.json"))).ok()?;
                // A broken file is a missing one: it is made again.
                Some(PaperDigest {
                    paper,
                    data: serde_json::from_slice(&text).ok()?,
                })
            };
            match i.papers {
                Some(p) => p.into_iter().filter_map(read).collect(),
                None => std::fs::read_dir(&dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter_map(|e| e.path().file_stem()?.to_str()?.parse().ok())
                    .collect::<std::collections::BTreeSet<PaperId>>()
                    .into_iter()
                    .filter_map(read)
                    .collect(),
            }
        })
        .await
        .map_err(|e| CommandError::failed(errors::IO, e))
    }
}
