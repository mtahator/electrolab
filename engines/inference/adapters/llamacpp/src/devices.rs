//! The graphics cards llama.cpp can use and how much of their memory is free: `--list-devices` of the
//! same build that will run the model. It is asked, limited in time, and may not answer.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ep_os::{ProcessSpawner, ProcessSpec};

/// How long the answer may take.
const LIMIT: Duration = Duration::from_secs(8);

/// A graphics card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// Name for `-dev` (`Vulkan1`).
    pub id: String,
    /// Name for people.
    pub name: String,
    /// Bytes.
    pub total: u64,
    /// Bytes.
    pub free: u64,
}

impl Device {
    /// Part of the processor: it shares the computer's memory and is slow.
    fn integrated(&self) -> bool {
        let n = self.name.to_lowercase();
        [
            "radeon(tm) graphics",
            "radeon graphics",
            "uhd graphics",
            "hd graphics",
            "iris",
        ]
        .iter()
        .any(|x| n.contains(x))
    }
}

/// Read what `--list-devices` wrote.
#[must_use]
pub fn parse(text: &str) -> Vec<Device> {
    text.lines()
        .filter_map(|l| {
            let (id, rest) = l.trim().split_once(": ")?;
            let word = !id.is_empty() && id.chars().all(char::is_alphanumeric);
            let open = rest.rfind('(')?;
            let inside = rest[open + 1..].trim_end().strip_suffix(')')?;
            let (total, free) = inside.split_once(',')?;
            let mib = |s: &str| -> Option<u64> {
                let n: String = s.chars().filter(char::is_ascii_digit).collect();
                s.contains("MiB")
                    .then(|| n.parse::<u64>().ok())?
                    .map(|n| n << 20)
            };
            let (total, free) = (mib(total)?, mib(free)?);
            word.then(|| Device {
                id: id.into(),
                name: rest[..open].trim().into(),
                total,
                free,
            })
        })
        .collect()
}

/// The card to use: a real one before one that is part of the processor, then the one with more
/// free memory.
#[must_use]
pub fn best(devices: &[Device]) -> Option<&Device> {
    devices.iter().max_by_key(|d| (!d.integrated(), d.free))
}

/// Ask the build at `exe`; nothing when it does not answer in time.
pub async fn ask(exe: &Path, spawner: &Arc<dyn ProcessSpawner>, file: &Path) -> Vec<Device> {
    let _ = std::fs::remove_file(file);
    let spec = ProcessSpec {
        program: exe.to_path_buf(),
        args: vec!["--list-devices".into()],
        cwd: exe.parent().map(PathBuf::from),
        env: vec![],
        log_file: Some(file.to_path_buf()),
    };
    let Ok(mut child) = spawner.spawn(&spec) else {
        return vec![];
    };
    let started = Instant::now();
    while matches!(child.try_wait(), Ok(None)) {
        if started.elapsed() > LIMIT {
            let _ = child.kill();
            return vec![];
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    let text = std::fs::read(file).unwrap_or_default();
    parse(&String::from_utf8_lossy(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAID: &str = "load_backend: loaded Vulkan backend\nAvailable devices:\n  Vulkan0: AMD Radeon(TM) Graphics (8143 MiB, 7735 MiB free)\n  Vulkan1: NVIDIA GeForce RTX 3050 Ti Laptop GPU (3962 MiB, 3367 MiB free)\n";

    #[test]
    fn the_real_card_is_found_with_its_free_memory() {
        let d = parse(SAID);
        assert_eq!(d.len(), 2);
        let b = best(&d).unwrap();
        assert_eq!((b.id.as_str(), b.free >> 20), ("Vulkan1", 3367));
        assert_eq!(best(&d[..1]).unwrap().id, "Vulkan0", "better than none");
    }

    #[test]
    fn strange_lines_are_left_out() {
        assert_eq!(
            parse("error: x (y)\n a: b (1, 2)\n : (1 MiB, 2 MiB free)\nAvailable devices:\n"),
            []
        );
    }
}
