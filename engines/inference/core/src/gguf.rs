//! What a GGUF file is, read from its header only (contract: automatic model search). The file is
//! untrusted: every length is checked, the header is limited, and nothing is loaded.

use std::io::{self, Read, Seek, SeekFrom};

/// Most bytes of header read or skipped (tokenizers are a few MB).
const MAX_HEADER: u64 = 128 << 20;
/// Longest text value kept.
const MAX_TEXT: u64 = 4096;

/// The header fields the engine uses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Header {
    /// `general.architecture` (`qwen3`, `clip`, …).
    pub architecture: String,
    /// `general.type` (`model`, `mmproj`, …).
    pub kind: Option<String>,
    /// `general.name`.
    pub name: Option<String>,
    /// `general.license`.
    pub licence: Option<String>,
    /// `general.file_type`.
    pub file_type: Option<u32>,
    /// `<architecture>.context_length`.
    pub context: Option<u32>,
    /// `<architecture>.block_count`.
    pub layers: Option<u32>,
    /// `<architecture>.embedding_length`.
    pub width: Option<u32>,
    /// `<architecture>.attention.head_count`.
    pub heads: Option<u32>,
    /// `<architecture>.attention.head_count_kv` (when it is the same in every layer).
    pub cache_heads: Option<u32>,
    /// `<architecture>.attention.key_length`.
    pub key_length: Option<u32>,
    /// `<architecture>.pooling_type` is present: an embedding or reranking model.
    pub pooling: bool,
    /// A chat template is present.
    pub chat_template: bool,
}

/// What a model file can be used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Writes answers.
    Chat,
    /// Makes vectors for search.
    Embedding,
    /// The picture part of another model (`mmproj`).
    VisionPart,
    /// Something else (a base model, a page reader, …).
    Other,
}

struct Reader<'a, R> {
    r: &'a mut R,
    left: u64,
}

fn bad(m: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.to_string())
}

impl<R: Read + Seek> Reader<'_, R> {
    fn take(&mut self, n: u64) -> io::Result<()> {
        self.left = self
            .left
            .checked_sub(n)
            .ok_or_else(|| bad("header too long"))?;
        Ok(())
    }
    fn bytes<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        self.take(N as u64)?;
        let mut b = [0; N];
        self.r.read_exact(&mut b)?;
        Ok(b)
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.bytes()?))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.bytes()?))
    }
    fn skip(&mut self, n: u64) -> io::Result<()> {
        self.take(n)?;
        // Short skips are read: a seek throws the reader's buffer away (a tokenizer has 150 000
        // short texts).
        if n <= 4096 {
            let done = io::copy(&mut self.r.by_ref().take(n), &mut io::sink())?;
            if done < n {
                return Err(bad("cut short"));
            }
        } else {
            let n = i64::try_from(n).map_err(|_| bad("length"))?;
            self.r.seek(SeekFrom::Current(n))?;
        }
        Ok(())
    }
    fn text(&mut self) -> io::Result<String> {
        let n = self.u64()?;
        let keep = n.min(MAX_TEXT);
        self.take(keep)?;
        let mut b = vec![0; usize::try_from(keep).map_err(|_| bad("length"))?];
        self.r.read_exact(&mut b)?;
        self.skip(n - keep)?;
        Ok(String::from_utf8_lossy(&b).into_owned())
    }
    fn skip_value(&mut self, ty: u32, depth: u8) -> io::Result<()> {
        match ty {
            0 | 1 | 7 => self.skip(1),
            2 | 3 => self.skip(2),
            4..=6 => self.skip(4),
            10..=12 => self.skip(8),
            8 => {
                let n = self.u64()?;
                self.skip(n)
            }
            9 if depth < 2 => {
                let (of, n) = (self.u32()?, self.u64()?);
                match of {
                    0 | 1 | 7 => self.skip(n),
                    2 | 3 => self.skip(n.checked_mul(2).ok_or_else(|| bad("length"))?),
                    4..=6 => self.skip(n.checked_mul(4).ok_or_else(|| bad("length"))?),
                    10..=12 => self.skip(n.checked_mul(8).ok_or_else(|| bad("length"))?),
                    _ => {
                        for _ in 0..n {
                            self.skip_value(of, depth + 1)?;
                        }
                        Ok(())
                    }
                }
            }
            _ => Err(bad("unknown value type")),
        }
    }
}

/// Read the header.
///
/// # Errors
/// Not a GGUF file, or a broken one.
pub fn read<R: Read + Seek>(r: &mut R) -> io::Result<Header> {
    let mut r = Reader {
        r,
        left: MAX_HEADER,
    };
    if &r.bytes::<4>()? != b"GGUF" {
        return Err(bad("not GGUF"));
    }
    if !matches!(r.u32()?, 2 | 3) {
        return Err(bad("unknown GGUF version"));
    }
    let _tensors = r.u64()?;
    let pairs = r.u64()?;
    let mut h = Header::default();
    for _ in 0..pairs.min(10_000) {
        let key = r.text()?;
        let ty = r.u32()?;
        match (key.as_str(), ty) {
            ("general.architecture", 8) => h.architecture = r.text()?,
            ("general.type", 8) => h.kind = Some(r.text()?),
            ("general.name", 8) => h.name = Some(r.text()?),
            ("general.license", 8) => h.licence = Some(r.text()?),
            ("general.file_type", 4) => h.file_type = Some(r.u32()?),
            (k, 4) if k.ends_with(".context_length") => h.context = Some(r.u32()?),
            (k, 4) if k.ends_with(".block_count") => h.layers = Some(r.u32()?),
            (k, 4) if k.ends_with(".embedding_length") => h.width = Some(r.u32()?),
            (k, 4) if k.ends_with(".attention.head_count") => h.heads = Some(r.u32()?),
            (k, 4) if k.ends_with(".attention.head_count_kv") => h.cache_heads = Some(r.u32()?),
            (k, 4) if k.ends_with(".attention.key_length") => h.key_length = Some(r.u32()?),
            (k, _) if k.ends_with(".pooling_type") => {
                h.pooling = true;
                r.skip_value(ty, 0)?;
            }
            (k, _) if k.starts_with("tokenizer.chat_template") => {
                h.chat_template = true;
                r.skip_value(ty, 0)?;
            }
            _ => r.skip_value(ty, 0)?,
        }
    }
    if h.architecture.is_empty() {
        return Err(bad("no architecture"));
    }
    Ok(h)
}

impl Header {
    /// What the file can be used for.
    #[must_use]
    pub fn use_for(&self) -> Kind {
        let arch = self.architecture.to_lowercase();
        if arch == "clip" || self.kind.as_deref() == Some("mmproj") {
            Kind::VisionPart
        } else if arch.contains("ocr") {
            Kind::Other
        } else if self.pooling || arch.contains("bert") || arch.contains("embed") {
            Kind::Embedding
        } else if self.chat_template {
            Kind::Chat
        } else {
            Kind::Other
        }
    }

    /// Bytes of 8-bit cache per token of context, when the file says enough. Untrusted numbers are
    /// multiplied without overflow.
    #[must_use]
    pub fn cache_per_token(&self) -> Option<u64> {
        let (layers, heads) = (u64::from(self.layers?), u64::from(self.heads?).max(1));
        let cache_heads = u64::from(self.cache_heads?);
        let key = match self.key_length {
            Some(k) => u64::from(k),
            None => u64::from(self.width?) / heads,
        };
        // Keys and values; 34 bytes hold 32 numbers.
        let numbers = layers
            .checked_mul(2)?
            .checked_mul(cache_heads)?
            .checked_mul(key)?;
        let bytes = numbers.checked_mul(17)? / 16;
        (bytes > 0 && bytes < 64 << 20).then_some(bytes)
    }

    /// Name of the quantisation (`Q4_K_M`), when known.
    #[must_use]
    pub fn quantisation(&self) -> Option<&'static str> {
        Some(match self.file_type? {
            0 => "F32",
            1 => "F16",
            2 => "Q4_0",
            7 => "Q8_0",
            8 => "Q5_0",
            10 => "Q2_K",
            11 => "Q3_K_S",
            12 => "Q3_K_M",
            13 => "Q3_K_L",
            14 => "Q4_K_S",
            15 => "Q4_K_M",
            16 => "Q5_K_S",
            17 => "Q5_K_M",
            18 => "Q6_K",
            32 => "BF16",
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn text(s: &str) -> Vec<u8> {
        let mut b = (s.len() as u64).to_le_bytes().to_vec();
        b.extend(s.as_bytes());
        b
    }
    /// A header with these text values, a token list before the chat template, as real files.
    fn file(texts: &[(&str, &str)], template: bool) -> Vec<u8> {
        let mut b = b"GGUF".to_vec();
        b.extend(3u32.to_le_bytes());
        b.extend(0u64.to_le_bytes());
        b.extend((texts.len() as u64 + 2 + u64::from(template)).to_le_bytes());
        for (k, v) in texts {
            b.extend(text(k));
            b.extend(8u32.to_le_bytes());
            b.extend(text(v));
        }
        b.extend(text("general.file_type"));
        b.extend(4u32.to_le_bytes());
        b.extend(15u32.to_le_bytes());
        b.extend(text("tokenizer.ggml.tokens"));
        b.extend(9u32.to_le_bytes());
        b.extend(8u32.to_le_bytes());
        b.extend(3u64.to_le_bytes());
        for t in ["a", "bc", ""] {
            b.extend(text(t));
        }
        if template {
            b.extend(text("tokenizer.chat_template"));
            b.extend(8u32.to_le_bytes());
            b.extend(text(&"{{ x }}".repeat(2000)));
        }
        b
    }
    fn header(b: &[u8]) -> io::Result<Header> {
        read(&mut Cursor::new(b))
    }

    #[test]
    fn a_header_says_what_the_model_is_for() {
        let h = header(&file(
            &[("general.architecture", "qwen3"), ("general.name", "Q")],
            true,
        ))
        .unwrap();
        assert_eq!(
            (h.use_for(), h.quantisation(), h.name.as_deref()),
            (Kind::Chat, Some("Q4_K_M"), Some("Q"))
        );
        let kind = |a: &str, t| {
            header(&file(&[("general.architecture", a)], t))
                .unwrap()
                .use_for()
        };
        assert_eq!(kind("clip", false), Kind::VisionPart);
        assert_eq!(kind("nomic-bert", false), Kind::Embedding);
        assert_eq!(kind("deepseek2-ocr", true), Kind::Other);
        assert_eq!(kind("llama", false), Kind::Other);
    }

    #[test]
    fn the_cache_is_counted_from_the_header() {
        let h = Header {
            layers: Some(36),
            width: Some(2560),
            heads: Some(32),
            cache_heads: Some(8),
            key_length: Some(128),
            ..Header::default()
        };
        // Qwen3 4B: about 640 MiB at 8192 tokens.
        let mib = (h.cache_per_token().unwrap() * 8192) >> 20;
        assert!((600..680).contains(&mib), "{mib}");
        let huge = Header {
            layers: Some(u32::MAX),
            ..h.clone()
        };
        assert_eq!(huge.cache_per_token(), None, "a wrong number is no number");
        assert_eq!(Header::default().cache_per_token(), None);
    }

    #[test]
    fn broken_headers_are_errors_never_crashes() {
        let good = file(&[("general.architecture", "qwen3")], true);
        for n in 0..good.len().min(400) {
            let _ = header(&good[..n]);
        }
        for i in 0..good.len().min(400) {
            let mut b = good.clone();
            b[i] = 0xff;
            let _ = header(&b);
        }
        assert!(header(b"PK\x03\x04 not a model").is_err());
        assert!(header(&file(&[], true)).is_err(), "no architecture");
        let mut huge = b"GGUF\x03\0\0\0\0\0\0\0\0\0\0\0\x01\0\0\0\0\0\0\0".to_vec();
        huge.extend(u64::MAX.to_le_bytes());
        assert!(header(&huge).is_err(), "a length of 2^64 is refused");
    }
}
