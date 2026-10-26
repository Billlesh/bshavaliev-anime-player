use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::params::PipelineParams;

const HEAD_TAIL: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheKey {
    pub fingerprint: String,
    pub pipeline_id: String,
    pub combined: String,
}

pub fn pipeline_id(params: &PipelineParams) -> String {
    let mut hasher = Sha256::new();
    let canonical = serde_json::to_vec(params).expect("params serialize");
    hasher.update(&canonical);
    format!("p{:x}", Hex8(hasher.finalize()))
}

/// Size + mtime + first/last 1 MiB. Whole-file SHA-256 is too slow for episode opens.
pub fn fingerprint_file(path: &Path) -> Result<String, std::io::Error> {
    let meta = std::fs::metadata(path)?;
    let mut file = File::open(path)?;
    let len = meta.len();
    let mtime = meta
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let mut hasher = Sha256::new();
    hasher.update(len.to_le_bytes());
    hasher.update(mtime.to_le_bytes());
    hasher.update(path.to_string_lossy().as_bytes());

    let mut head = vec![0u8; HEAD_TAIL.min(len as usize)];
    file.read_exact(&mut head)?;
    hasher.update(&head);

    if len > HEAD_TAIL as u64 * 2 {
        file.seek(SeekFrom::End(-(HEAD_TAIL as i64)))?;
        let mut tail = vec![0u8; HEAD_TAIL];
        file.read_exact(&mut tail)?;
        hasher.update(&tail);
    }

    Ok(hex::encode(hasher.finalize()))
}

pub fn cache_key(path: &Path, params: &PipelineParams) -> Result<CacheKey, std::io::Error> {
    let fingerprint = fingerprint_file(path)?;
    let pid = pipeline_id(params);
    let combined = {
        let mut hasher = Sha256::new();
        hasher.update(fingerprint.as_bytes());
        hasher.update(pid.as_bytes());
        hex::encode(hasher.finalize())
    };
    Ok(CacheKey {
        fingerprint,
        pipeline_id: pid,
        combined,
    })
}

struct Hex8(sha2::digest::Output<Sha256>);

impl std::fmt::LowerHex for Hex8 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in self.0.iter().take(8) {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn key_changes_when_params_change() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ep.mkv");
        std::fs::write(&file, vec![7u8; 4096]).unwrap();
        let a = cache_key(&file, &PipelineParams::default()).unwrap();
        let mut p = PipelineParams::default();
        p.nvenc_cq = 18;
        let b = cache_key(&file, &p).unwrap();
        assert_ne!(a.combined, b.combined);
        assert_eq!(a.fingerprint, b.fingerprint);
    }

    #[test]
    fn key_stable_for_same_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ep.mkv");
        let mut f = File::create(&file).unwrap();
        f.write_all(&[1, 2, 3, 4]).unwrap();
        drop(f);
        let params = PipelineParams::default();
        let a = cache_key(&file, &params).unwrap();
        let b = cache_key(&file, &params).unwrap();
        assert_eq!(a, b);
    }
}
