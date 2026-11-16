//! Makes a tied-embedding model share one table, which saves about 127 MB of RAM and disk on
//! S1-mini / Qwen3-0.6B: the GGUF stores a Q6_K `output.weight` next to the Q4_K `token_embd.weight`,
//! but the model is tied, so llama.cpp uses `token_embd.weight` for the output projection when
//! `output.weight` is absent. Same output, measured on Android: 761 -> 391 MB with no-repack mmap.
//!
//! The file is rewritten without that tensor, streamed (never the whole model in memory):
//! the header is parsed and rewritten and the tensor data is copied in large pieces.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

const MAGIC: u32 = 0x4655_4747; // "GGUF" little endian
const DEFAULT_ALIGNMENT: u64 = 32;
const HEADER: u64 = 24; // magic, version, tensor count, key/value count

struct Info {
    name: String,
    raw: Vec<u8>,
    offset: u64,
}

fn bad(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

fn align(v: u64, a: u64) -> u64 {
    v.div_ceil(a) * a
}

/// Sequential little-endian reader that remembers its position in the file.
struct Reader<'a, R: Read + Seek> {
    inner: &'a mut R,
    pos: u64,
}

impl<R: Read + Seek> Reader<'_, R> {
    fn read(&mut self, n: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; n];
        self.inner.read_exact(&mut buf).map_err(|_| bad("truncated GGUF header"))?;
        self.pos += n as u64;
        Ok(buf)
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.read(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.read(8)?.try_into().unwrap()))
    }
    fn string(&mut self) -> io::Result<String> {
        let len = self.u64()? as usize;
        if len > 1 << 24 {
            return Err(bad("implausible GGUF string length"));
        }
        Ok(String::from_utf8_lossy(&self.read(len)?).into_owned())
    }
    fn skip(&mut self, n: u64) -> io::Result<()> {
        self.inner.seek(SeekFrom::Current(n as i64))?;
        self.pos += n;
        Ok(())
    }
    fn skip_value(&mut self, ty: u32) -> io::Result<()> {
        match ty {
            0 | 1 | 7 => self.skip(1),
            2 | 3 => self.skip(2),
            4..=6 => self.skip(4),
            10..=12 => self.skip(8),
            8 => {
                let len = self.u64()?;
                self.skip(len)
            }
            9 => {
                let elem = self.u32()?;
                let n = self.u64()?;
                match elem {
                    0 | 1 | 7 => self.skip(n),
                    2 | 3 => self.skip(2 * n),
                    4..=6 => self.skip(4 * n),
                    10..=12 => self.skip(8 * n),
                    _ => (0..n).try_for_each(|_| self.skip_value(elem)),
                }
            }
            _ => Err(bad(format!("unknown GGUF value type {ty}"))),
        }
    }
}

fn copy_range(src: &mut File, pos: u64, count: u64, dst: &mut impl Write) -> io::Result<()> {
    src.seek(SeekFrom::Start(pos))?;
    let copied = io::copy(&mut Read::take(&mut *src, count), dst)?;
    if copied != count {
        return Err(bad("short copy"));
    }
    Ok(())
}

/// Writes `dst` without the tensor called `name`. Returns `Ok(false)` (and writes nothing) when
/// `src` has no such tensor.
pub fn drop_tensor(src: &Path, dst: &Path, name: &str) -> io::Result<bool> {
    let mut input = File::open(src)?;
    let file_len = input.metadata()?.len();
    let mut head = [0u8; HEADER as usize];
    input.read_exact(&mut head).map_err(|_| bad("not a GGUF file"))?;
    if u32::from_le_bytes(head[0..4].try_into().unwrap()) != MAGIC {
        return Err(bad("not a GGUF file"));
    }
    let version = u32::from_le_bytes(head[4..8].try_into().unwrap());
    if version < 2 {
        return Err(bad(format!("GGUF version {version} is not supported")));
    }
    let tensor_count = u64::from_le_bytes(head[8..16].try_into().unwrap());
    let kv_count = u64::from_le_bytes(head[16..24].try_into().unwrap());
    if tensor_count > 1 << 20 || kv_count > 1 << 20 {
        return Err(bad("implausible GGUF header"));
    }

    let mut r = Reader { inner: &mut input, pos: HEADER };
    let mut alignment = DEFAULT_ALIGNMENT;
    for _ in 0..kv_count {
        let key = r.string()?;
        let ty = r.u32()?;
        if key == "general.alignment" && ty == 4 {
            alignment = u64::from(r.u32()?).max(1);
        } else {
            r.skip_value(ty)?;
        }
    }
    let kv_end = r.pos;
    let mut infos = Vec::with_capacity(tensor_count as usize);
    for _ in 0..tensor_count {
        let start = r.pos;
        let tensor = r.string()?;
        let dims = r.u32()?;
        for _ in 0..dims {
            r.u64()?;
        }
        r.u32()?; // type
        let offset = r.u64()?;
        let end = r.pos;
        r.inner.seek(SeekFrom::Start(start))?;
        let raw = {
            let mut buf = vec![0u8; (end - start) as usize];
            r.inner.read_exact(&mut buf)?;
            buf
        };
        r.inner.seek(SeekFrom::Start(end))?;
        infos.push(Info { name: tensor, raw, offset });
    }
    let info_end = r.pos;
    let data_start = align(info_end, alignment);
    let Some(drop_index) = infos.iter().position(|i| i.name == name) else { return Ok(false) };

    // Each tensor's stored size is the distance to the next one (they are alignment padded already).
    let mut order: Vec<usize> = (0..infos.len()).collect();
    order.sort_by_key(|&i| infos[i].offset);
    let mut size = vec![0u64; infos.len()];
    for (k, &idx) in order.iter().enumerate() {
        let end = if k + 1 < order.len() { infos[order[k + 1]].offset } else { file_len - data_start };
        size[idx] = end - infos[idx].offset;
    }
    let kept: Vec<usize> = order.iter().copied().filter(|&i| i != drop_index).collect();
    let mut new_offset = vec![0u64; infos.len()];
    let mut cursor = 0u64;
    for &i in &kept {
        new_offset[i] = cursor;
        cursor += size[i];
    }

    let tmp = dst.with_extension("part");
    let result = (|| -> io::Result<()> {
        let mut out = io::BufWriter::new(File::create(&tmp)?);
        out.write_all(&MAGIC.to_le_bytes())?;
        out.write_all(&version.to_le_bytes())?;
        out.write_all(&(tensor_count - 1).to_le_bytes())?;
        out.write_all(&kv_count.to_le_bytes())?;
        copy_range(&mut input, HEADER, kv_end - HEADER, &mut out)?;
        let mut written = kv_end;
        for (i, info) in infos.iter().enumerate() {
            if i == drop_index {
                continue;
            }
            let mut raw = info.raw.clone();
            let n = raw.len();
            raw[n - 8..].copy_from_slice(&new_offset[i].to_le_bytes()); // the offset is the last 8 bytes
            out.write_all(&raw)?;
            written += raw.len() as u64;
        }
        out.write_all(&vec![0u8; (align(written, alignment) - written) as usize])?;
        for &i in &kept {
            copy_range(&mut input, data_start + infos[i].offset, size[i], &mut out)?;
        }
        out.flush()
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, dst)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(s: &str) -> Vec<u8> {
        let mut v = (s.len() as u64).to_le_bytes().to_vec();
        v.extend(s.as_bytes());
        v
    }

    /// A tiny valid GGUF v3 file: a string, an int, a string array and the given tensors.
    fn build(tensors: &[(&str, Vec<u8>)], alignment: u32) -> Vec<u8> {
        let a = u64::from(alignment);
        let mut kv = Vec::new();
        kv.extend(string("general.architecture")); kv.extend(8u32.to_le_bytes()); kv.extend(string("qwen3"));
        kv.extend(string("general.alignment")); kv.extend(4u32.to_le_bytes()); kv.extend(alignment.to_le_bytes());
        kv.extend(string("tokenizer.tokens")); kv.extend(9u32.to_le_bytes()); kv.extend(8u32.to_le_bytes()); kv.extend(3u64.to_le_bytes());
        for t in ["a", "bb", "ccc"] { kv.extend(string(t)); }
        let (mut infos, mut data, mut offset) = (Vec::new(), Vec::new(), 0u64);
        for (name, bytes) in tensors {
            infos.extend(string(name)); infos.extend(1u32.to_le_bytes()); infos.extend((bytes.len() as u64).to_le_bytes());
            infos.extend(0u32.to_le_bytes()); infos.extend(offset.to_le_bytes());
            data.extend(bytes);
            let pad = (a - bytes.len() as u64 % a) % a;
            data.extend(vec![0u8; pad as usize]);
            offset += bytes.len() as u64 + pad;
        }
        let mut out = Vec::new();
        out.extend(MAGIC.to_le_bytes()); out.extend(3u32.to_le_bytes());
        out.extend((tensors.len() as u64).to_le_bytes()); out.extend(3u64.to_le_bytes());
        out.extend(kv); out.extend(infos);
        let pad = (a - out.len() as u64 % a) % a;
        out.extend(vec![0u8; pad as usize]);
        out.extend(data);
        out
    }

    /// Reads every tensor's bytes back through the file's own table.
    fn read_tensors(f: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut c = io::Cursor::new(f);
        let mut r = Reader { inner: &mut c, pos: 0 };
        r.u32().unwrap(); r.u32().unwrap();
        let n = r.u64().unwrap();
        let kvs = r.u64().unwrap();
        let mut a = 32u64;
        for _ in 0..kvs {
            let k = r.string().unwrap();
            let t = r.u32().unwrap();
            if k == "general.alignment" { a = u64::from(r.u32().unwrap()); } else { r.skip_value(t).unwrap(); }
        }
        let mut table = Vec::new();
        for _ in 0..n {
            let name = r.string().unwrap();
            r.u32().unwrap();
            let len = r.u64().unwrap();
            r.u32().unwrap();
            table.push((name, len, r.u64().unwrap()));
        }
        let start = align(r.pos, a) as usize;
        table.into_iter().map(|(name, len, off)| (name, f[start + off as usize..start + (off + len) as usize].to_vec())).collect()
    }

    fn tensors() -> Vec<(&'static str, Vec<u8>)> {
        vec![
            ("token_embd.weight", (0..100u32).map(|i| (i + 1) as u8).collect()),
            ("output.weight", (0..70u32).map(|i| (200 - i) as u8).collect()),
            ("blk.0.attn_q.weight", (0..33u32).map(|i| (i * 3) as u8).collect()),
        ]
    }

    fn run(alignment: u32, name: &str) -> (Option<Vec<(String, Vec<u8>)>>, u64, u64) {
        let dir = std::env::temp_dir().join(format!("gguf-test-{}-{alignment}-{}", std::process::id(), name.len()));
        std::fs::create_dir_all(&dir).unwrap();
        let (src, dst) = (dir.join("m.gguf"), dir.join("tied.gguf"));
        std::fs::write(&src, build(&tensors(), alignment)).unwrap();
        let changed = drop_tensor(&src, &dst, name).unwrap();
        let out = changed.then(|| read_tensors(&std::fs::read(&dst).unwrap()));
        let sizes = (std::fs::metadata(&src).unwrap().len(), dst.metadata().map(|m| m.len()).unwrap_or(0));
        let _ = std::fs::remove_dir_all(&dir);
        (out, sizes.0, sizes.1)
    }

    #[test]
    fn drops_the_tensor_and_keeps_the_rest_byte_for_byte() {
        for alignment in [32, 64] {
            let (out, before, after) = run(alignment, "output.weight");
            let out = out.expect("tensor present");
            let t = tensors();
            assert_eq!(out.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["token_embd.weight", "blk.0.attn_q.weight"]);
            assert_eq!(out[0].1, t[0].1);
            assert_eq!(out[1].1, t[2].1);
            assert!(after < before);
        }
    }

    #[test]
    fn a_missing_tensor_writes_nothing() {
        assert!(run(32, "nope.weight").0.is_none());
    }

    #[test]
    fn rejects_a_file_that_is_not_gguf() {
        let dir = std::env::temp_dir().join(format!("gguf-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("x.gguf");
        std::fs::write(&src, b"hello world, definitely not a model").unwrap();
        assert!(drop_tensor(&src, &dir.join("y.gguf"), "output.weight").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The real S1-mini, if present: must match the 356,593,504-byte file the Python gguf package made.
    #[test]
    fn real_model_matches_the_reference_size() {
        let real = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache/voicio-bakeoff/s1-mini-q4km.gguf");
        if !real.is_file() { return; }
        let dst = std::env::temp_dir().join(format!("real-tied-{}.gguf", std::process::id()));
        assert!(drop_tensor(&real, &dst, "output.weight").unwrap());
        assert_eq!(dst.metadata().unwrap().len(), 356_593_504);
        let _ = std::fs::remove_file(&dst);
    }
}
