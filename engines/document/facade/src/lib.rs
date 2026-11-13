//! Document engine facade: `document.*` commands on the bus.
//!
//! The engine reads stored files through the [`BlobAccess`] port, which the host implements
//! with the open library's blob store (the Document engine does not depend on Workspace).
#![deny(missing_docs)]

use std::path::PathBuf;
use std::sync::Arc;

use ep_bus::{Bus, CommandError, Cx, Permission, RegisterError, Spec, Stability};
use ep_document_contract::{MetadataRequest, PdfMetadata, commands, errors};
use ep_types::Hash;

/// Port: where a stored file is on disk.
pub trait BlobAccess: Send + Sync {
    /// Path of the blob, if it is available.
    fn path_of(&self, hash: &Hash) -> Option<PathBuf>;
}

/// The Document engine.
pub struct Document {
    blobs: Arc<dyn BlobAccess>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document").finish_non_exhaustive()
    }
}

impl Document {
    /// An engine reading files through `blobs`.
    #[must_use]
    pub fn new(blobs: Arc<dyn BlobAccess>) -> Arc<Self> {
        Arc::new(Self { blobs })
    }

    /// Register the `document.*` commands.
    ///
    /// # Errors
    /// A name is already registered.
    pub fn register(self: &Arc<Self>, bus: &Bus) -> Result<(), RegisterError> {
        let d = self.clone();
        bus.register(
            Spec {
                name: commands::METADATA,
                summary: "Title, authors, DOI and page count from a stored PDF",
                stability: Stability::Experimental,
                permission: Permission::read(),
            },
            move |_cx: Cx, i: MetadataRequest| {
                let d = d.clone();
                async move { d.metadata(i).await }
            },
        )
    }

    async fn metadata(&self, i: MetadataRequest) -> Result<PdfMetadata, CommandError> {
        let path = self.blobs.path_of(&i.file.hash).ok_or_else(|| {
            CommandError::failed(
                errors::NOT_FOUND,
                format!("file {} is not available", i.file.hash),
            )
        })?;
        tokio::task::spawn_blocking(move || {
            let raw = ep_document_adapter_lopdf::read_metadata(&path)
                .map_err(|e| CommandError::failed(errors::UNREADABLE, e))?;
            Ok(ep_document_core::metadata::clean(&raw))
        })
        .await
        .map_err(|e| CommandError::failed(errors::UNREADABLE, e))?
    }
}
