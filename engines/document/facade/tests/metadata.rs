//! `document.metadata` on the fixture papers (P1.M1.WSP.3 snapshot).
//!
//! The committed fixture always runs; the downloaded ones (`cargo xtask fetch-fixtures`) run
//! when present. `EP_PRINT_METADATA=1` prints what was read; `EP_UPDATE_SNAPSHOT=1` rewrites
//! the snapshot after a reviewed change.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // tests and their helpers stop loudly

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ep_bus::{Bus, Envelope};
use ep_document::{BlobAccess, Document};
use ep_document_contract::PdfMetadata;
use ep_types::{BlobRef, Hash, MediaType};
use serde_json::json;

struct ByName(PathBuf);
impl BlobAccess for ByName {
    fn path_of(&self, _hash: &Hash) -> Option<PathBuf> {
        self.0.exists().then(|| self.0.clone())
    }
}

async fn read(path: &Path) -> PdfMetadata {
    let bus = Bus::default();
    Document::new(Arc::new(ByName(path.to_path_buf())))
        .register(&bus)
        .expect("register");
    let file = BlobRef {
        hash: Hash::of(b"any"),
        size: 0,
        media_type: MediaType::pdf(),
    };
    let v = bus
        .call(Envelope::new("document.metadata", json!({ "file": file })))
        .await
        .expect("metadata");
    serde_json::from_value(v).expect("contract")
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[tokio::test]
async fn committed_fixture() {
    let m = read(&root().join("fixtures/papers/fawiki-artificial-neural-network.pdf")).await;
    assert!(m.pages > 0);
    assert!(!m.encrypted);
}

#[tokio::test]
async fn downloaded_fixtures_snapshot() {
    let dir = root().join(".fixtures/papers");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
        .collect();
    files.sort();
    let mut snapshot = serde_json::Map::new();
    for f in &files {
        let m = read(f).await;
        assert!(m.pages > 0, "{}", f.display());
        snapshot.insert(
            f.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            serde_json::to_value(&m).expect("json"),
        );
    }
    if std::env::var_os("EP_PRINT_METADATA").is_some() {
        eprintln!("{}", serde_json::to_string_pretty(&snapshot).expect("json"));
    }
    let expected_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/metadata-snapshot.json");
    if std::env::var_os("EP_UPDATE_SNAPSHOT").is_some() {
        std::fs::write(
            &expected_path,
            serde_json::to_string_pretty(&snapshot).expect("json")
                + "
",
        )
        .expect("write snapshot");
        return;
    }
    if let Ok(text) = std::fs::read_to_string(&expected_path) {
        let expected: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&text).expect("snapshot json");
        for (name, value) in &snapshot {
            if let Some(want) = expected.get(name) {
                assert_eq!(
                    value, want,
                    "metadata of {name} changed; review and update the snapshot"
                );
            }
        }
    }
}
