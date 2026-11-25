//! Finding models: the chosen folders are searched, sub-folders too, and every GGUF file is
//! recognised from its header (contract: automatic model search). A suggestion is made for this
//! machine; what the user chose is kept in a small settings file.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use ep_inference_contract::{Backend, ModelKind};
use ep_inference_core::gguf;
use serde::{Deserialize, Serialize};

/// How deep, how many entries and how long one folder is searched.
const DEPTH: usize = 8;
const ENTRIES: usize = 60_000;
const TIME: Duration = Duration::from_secs(8);
/// Models up to this size run whole on a 4 GB GPU (ADR-005).
pub const FITS_GPU: u64 = 3_000_000_000;
/// Models up to this size are comfortable on the processor.
const FITS_CPU: u64 = 1_500_000_000;
/// Longest context used.
const CONTEXT: u32 = 8192;
/// Folders never searched.
const SKIP: [&str; 6] = [
    "node_modules",
    "target",
    "Windows",
    "$RECYCLE.BIN",
    "System Volume Information",
    "site-packages",
];

/// A model the project has measured (ADR-005).
struct Known {
    id: &'static str,
    name: &'static str,
    file: &'static str,
    licence: &'static str,
    gpu: bool,
}

const KNOWN: &[Known] = &[
    Known {
        id: "qwen3-4b-instruct-2507",
        name: "Qwen3 4B Instruct (2507), Q4_K_M",
        file: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        licence: "Apache-2.0",
        gpu: true,
    },
    Known {
        id: "lfm2.5-1.2b-instruct",
        name: "LFM2.5 1.2B Instruct, Q8_0",
        file: "LFM2.5-1.2B-Instruct-Q8_0.gguf",
        licence: "LFM Open License 1.0",
        gpu: false,
    },
];

/// A model file that was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub licence: String,
    pub kind: ModelKind,
    pub architecture: String,
    pub quantisation: Option<String>,
    pub context: u32,
    /// Bytes of cache per token of context, when the file says so.
    pub cache_per_token: Option<u64>,
}

/// What a search of one folder gave.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderFound {
    pub models: usize,
    /// Files of models in other formats (they cannot be used).
    pub other_formats: usize,
    /// The folder is not there.
    pub missing: bool,
    /// The search stopped at its limit.
    pub partial: bool,
}

/// Every model found.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub models: Vec<Found>,
    pub folders: HashMap<PathBuf, FolderFound>,
}

/// Headers already read, by path, size and time.
pub type Cache = HashMap<PathBuf, (u64, Option<SystemTime>, Option<gguf::Header>)>;

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The number of a later part of a split model (`-00002-of-00003`), which is not a model itself.
fn is_later_part(stem: &str) -> bool {
    let Some((head, total)) = stem.rsplit_once("-of-") else {
        return false;
    };
    let digits = |s: &str| s.len() == 5 && s.bytes().all(|b| b.is_ascii_digit());
    digits(total) && head.len() > 5 && digits(&head[head.len() - 5..]) && !head.ends_with("00001")
}

fn header(
    path: &Path,
    size: u64,
    time: Option<SystemTime>,
    cache: &mut Cache,
) -> Option<gguf::Header> {
    if let Some((s, t, h)) = cache.get(path)
        && (*s, *t) == (size, time)
    {
        return h.clone();
    }
    let h = fs::File::open(path)
        .ok()
        .and_then(|f| gguf::read(&mut std::io::BufReader::new(f)).ok());
    cache.insert(path.to_path_buf(), (size, time, h.clone()));
    h
}

/// Not empty, and not a code such as a commit hash (some converters write one).
fn is_a_name(s: &str) -> bool {
    let s = s.trim();
    let code = s.len() >= 16 && s.bytes().all(|b| b.is_ascii_hexdigit());
    !s.is_empty() && !code
}

fn recognise(path: &Path, cache: &mut Cache) -> Option<Found> {
    let meta = fs::metadata(path).ok()?;
    let h = header(path, meta.len(), meta.modified().ok(), cache)?;
    let file = file_name(path);
    let known = KNOWN.iter().find(|k| k.file.eq_ignore_ascii_case(&file));
    let quantisation = h.quantisation().map(str::to_string);
    let title = h
        .name
        .clone()
        .filter(|n| is_a_name(n))
        .unwrap_or_else(|| stem(path));
    let name = match &quantisation {
        Some(q) if !title.contains(q.as_str()) => format!("{title}, {q}"),
        _ => title,
    };
    Some(Found {
        id: known.map_or_else(|| stem(path).to_lowercase(), |k| k.id.to_string()),
        name: known.map_or(name, |k| k.name.to_string()),
        path: path.to_path_buf(),
        size: meta.len(),
        licence: known.map_or_else(
            || h.licence.clone().unwrap_or_default(),
            |k| k.licence.to_string(),
        ),
        kind: match h.use_for() {
            gguf::Kind::Chat => ModelKind::Chat,
            gguf::Kind::Embedding => ModelKind::Embedding,
            gguf::Kind::VisionPart => ModelKind::VisionPart,
            gguf::Kind::Other => ModelKind::Other,
        },
        cache_per_token: h.cache_per_token(),
        context: h.context.map_or(CONTEXT, |c| c.clamp(512, CONTEXT)),
        architecture: h.architecture,
        quantisation,
    })
}

/// Search `dir` and its sub-folders, nearest first.
fn search(
    dir: &Path,
    cache: &mut Cache,
    seen: &mut HashSet<PathBuf>,
    out: &mut Vec<Found>,
) -> FolderFound {
    let mut r = FolderFound::default();
    if !dir.is_dir() {
        r.missing = true;
        return r;
    }
    let started = Instant::now();
    let mut entries = 0usize;
    let mut queue = std::collections::VecDeque::from([(dir.to_path_buf(), 0usize)]);
    while let Some((d, depth)) = queue.pop_front() {
        let Ok(read) = fs::read_dir(&d) else { continue };
        let mut list: Vec<_> = read.flatten().collect();
        list.sort_by_key(fs::DirEntry::file_name);
        for e in list {
            entries += 1;
            if entries > ENTRIES || started.elapsed() > TIME {
                r.partial = true;
                return r;
            }
            let path = e.path();
            let name = file_name(&path);
            let Ok(ty) = e.file_type() else { continue };
            // Folder links are not followed (they can lead in circles); file links are.
            if ty.is_dir() {
                if depth < DEPTH
                    && !name.starts_with('.')
                    && !SKIP.iter().any(|s| s.eq_ignore_ascii_case(&name))
                {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }
            match path
                .extension()
                .and_then(|x| x.to_str())
                .map(str::to_lowercase)
                .as_deref()
            {
                Some("gguf") if !is_later_part(&stem(&path)) => {
                    let target = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                    if !seen.insert(target) {
                        continue;
                    }
                    if let Some(mut f) = recognise(&path, cache) {
                        // The same name twice: the second one is told apart by its folder.
                        if out.iter().any(|o| o.id == f.id) {
                            let parent = path.parent().map(file_name).unwrap_or_default();
                            f.id = format!("{}@{}", f.id, parent.to_lowercase());
                            f.name = format!("{} ({parent})", f.name);
                        }
                        if out.iter().all(|o| o.id != f.id) {
                            r.models += 1;
                            out.push(f);
                        }
                    }
                }
                Some("safetensors" | "pth" | "onnx") => r.other_formats += 1,
                _ => {}
            }
        }
    }
    r
}

/// Search every folder.
pub fn scan(dirs: &[PathBuf], cache: &mut Cache) -> Catalog {
    let mut c = Catalog::default();
    let mut seen = HashSet::new();
    for d in dirs {
        let r = search(d, cache, &mut seen, &mut c.models);
        c.folders.insert(d.clone(), r);
    }
    c
}

/// A model the project has measured.
pub fn is_known(id: &str) -> bool {
    KNOWN.iter().any(|k| k.id == id)
}

/// The model suggested for this machine: the measured one for its kind, else the largest chat
/// model that runs well here, else the smallest one.
pub fn suggested(models: &[Found], backend: Backend) -> Option<&Found> {
    let gpu = matches!(backend, Backend::Cuda | Backend::Vulkan);
    let chat = || models.iter().filter(|m| m.kind == ModelKind::Chat);
    let known = |g: bool| chat().find(|m| KNOWN.iter().any(|k| k.gpu == g && m.id == k.id));
    let limit = if gpu { FITS_GPU } else { FITS_CPU };
    known(gpu)
        .or_else(|| chat().filter(|m| m.size <= limit).max_by_key(|m| m.size))
        .or_else(|| known(!gpu))
        .or_else(|| chat().min_by_key(|m| m.size))
}

/// The model that looks at pictures: the smallest chat model with its picture part beside it.
pub fn vision(models: &[Found]) -> Option<(&Found, &Found)> {
    let beside = |a: &Found, b: &Found| a.path.parent() == b.path.parent();
    models
        .iter()
        .filter(|m| m.kind == ModelKind::Chat)
        .filter_map(|m| {
            let parts = models
                .iter()
                .filter(|p| p.kind == ModelKind::VisionPart && beside(m, p));
            let alone = models
                .iter()
                .filter(|o| o.kind == ModelKind::Chat && beside(m, o))
                .count()
                == 1;
            parts
                .filter(|p| alone || same_family(&stem(&m.path), &stem(&p.path)))
                .min_by_key(|p| p.size)
                .map(|p| (m, p))
        })
        .min_by_key(|(m, p)| m.size + p.size)
}

/// `Qwen2.5-VL-3B-Instruct-Q4_K_M` and `mmproj-Qwen2.5-VL-3B-Instruct-f16` are one model.
fn same_family(model: &str, part: &str) -> bool {
    let words = |s: &str| {
        s.to_lowercase()
            .split(['-', '_', '.', ' '])
            .filter(|w| !w.is_empty() && *w != "mmproj" && *w != "model")
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let (m, p) = (words(model), words(part));
    let same = m.iter().zip(&p).take_while(|(a, b)| a == b).count();
    same >= 2 && same + 2 >= p.len()
}

/// What the user chose, kept between starts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choices {
    /// Folders added by the user.
    #[serde(default)]
    pub folders: Vec<PathBuf>,
    /// Models in the order they are tried (empty: automatic).
    #[serde(default)]
    pub order: Vec<String>,
    /// No model is loaded before it is needed.
    #[serde(default)]
    pub on_demand: bool,
}

impl Choices {
    /// Read them; a missing or broken file means none.
    pub fn read(file: Option<&Path>) -> Self {
        file.and_then(|f| fs::read(f).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Keep them.
    pub fn write(&self, file: Option<&Path>) -> std::io::Result<()> {
        let Some(file) = file else { return Ok(()) };
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        let part = file.with_extension("part");
        fs::write(&part, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&part, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_is_not_a_name() {
        assert!(is_a_name("Qwen3 4B Instruct"));
        assert!(!is_a_name("4cd563d5a96af9e7c738b76cd89a0a200db7608f"));
        assert!(!is_a_name("  "));
    }

    #[test]
    fn later_parts_of_a_split_model_are_not_models() {
        assert!(is_later_part("big-00002-of-00003"));
        assert!(!is_later_part("big-00001-of-00003"));
        assert!(!is_later_part("made-of-steel"));
        assert!(!is_later_part("x-of-00003"));
    }

    fn model(id: &str, size: u64, kind: ModelKind) -> Found {
        Found {
            id: id.into(),
            name: id.into(),
            path: PathBuf::from(id),
            size,
            licence: String::new(),
            kind,
            architecture: "llama".into(),
            quantisation: None,
            context: CONTEXT,
            cache_per_token: None,
        }
    }

    #[test]
    fn the_model_that_looks_at_pictures_has_its_part_beside_it() {
        let at = |path: &str, size, kind| Found {
            path: PathBuf::from(path),
            ..model(path, size, kind)
        };
        let (chat, part) = (ModelKind::Chat, ModelKind::VisionPart);
        let ids = |v: &[Found]| vision(v).map(|(m, p)| (m.id.clone(), p.id.clone()));
        let alone = [
            at("a/plain.gguf", 9, chat),
            at("b/mmproj-f16.gguf", 1, part),
        ];
        assert_eq!(ids(&alone), None, "in another folder");
        let pair = [
            at("v/See-VL-3B-Q4_K_M.gguf", 9, chat),
            at("v/mmproj-See-VL-3B-Q8_0.gguf", 2, part),
            at("v/plain-4B-Q4.gguf", 3, chat),
        ];
        assert_eq!(
            ids(&pair),
            Some((pair[0].id.clone(), pair[1].id.clone())),
            "not the model without a part, though it is smaller"
        );
        let only = [at("v/x.gguf", 9, chat), at("v/mmproj.gguf", 2, part)];
        assert!(ids(&only).is_some(), "the only pair of its folder");
    }

    #[test]
    fn the_suggestion_fits_the_machine() {
        let chat = |id, gb: u64| model(id, gb * 100_000_000, ModelKind::Chat);
        let pick = |v: &[Found], b| suggested(v, b).map(|m| m.id.clone());
        let mut all = vec![
            chat("small", 8),
            chat("middle", 25),
            chat("huge", 400),
            model("vec", 1, ModelKind::Embedding),
        ];
        assert_eq!(
            pick(&all, Backend::Cuda).as_deref(),
            Some("middle"),
            "the largest that fits the GPU"
        );
        assert_eq!(pick(&all, Backend::Cpu).as_deref(), Some("small"));
        assert_eq!(
            pick(&all[2..], Backend::Cuda).as_deref(),
            Some("huge"),
            "better than none"
        );
        assert_eq!(
            pick(&all[3..], Backend::Cuda),
            None,
            "a search model never answers"
        );
        all.push(chat("qwen3-4b-instruct-2507", 25));
        assert_eq!(
            pick(&all, Backend::Vulkan).as_deref(),
            Some("qwen3-4b-instruct-2507"),
            "measured first"
        );
    }
}
