use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::hash::CacheKey;
use crate::params::PipelineParams;

const MANIFEST_NAME: &str = "manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub entries: BTreeMap<String, CacheRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheRecord {
    pub source_path: String,
    pub fingerprint: String,
    pub pipeline_id: String,
    pub output: PathBuf,
    pub created_at_unix: u64,
    pub params: PipelineParams,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            version: 1,
            entries: BTreeMap::new(),
        }
    }
}

pub struct CacheIndex {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

impl CacheIndex {
    pub fn open(dir: PathBuf) -> Result<Self, std::io::Error> {
        fs::create_dir_all(&dir)?;
        let path = dir.join(MANIFEST_NAME);
        let manifest = if path.exists() {
            let text = fs::read_to_string(&path)?;
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            Manifest::default()
        };
        Ok(Self { dir, manifest })
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.dir.join(MANIFEST_NAME)
    }

    pub fn output_path(&self, key: &CacheKey) -> PathBuf {
        self.dir.join(format!("{}.mkv", &key.combined[..16.min(key.combined.len())]))
    }

    pub fn sidecar_path(&self, key: &CacheKey) -> PathBuf {
        self.dir.join(format!("{}.json", &key.combined[..16.min(key.combined.len())]))
    }

    pub fn lookup(&self, key: &CacheKey) -> Option<&CacheRecord> {
        let rec = self.manifest.entries.get(&key.combined)?;
        if rec.output.exists() {
            Some(rec)
        } else {
            None
        }
    }

    pub fn insert(&mut self, key: &CacheKey, record: CacheRecord) -> Result<(), std::io::Error> {
        let sidecar = self.sidecar_path(key);
        fs::write(&sidecar, serde_json::to_vec_pretty(&record).unwrap())?;
        self.manifest.entries.insert(key.combined.clone(), record);
        self.flush()
    }

    pub fn flush(&self) -> Result<(), std::io::Error> {
        let tmp = self.dir.join("manifest.json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&self.manifest).unwrap())?;
        fs::rename(tmp, self.manifest_path())?;
        Ok(())
    }
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn record_for(source: &Path, key: &CacheKey, output: PathBuf, params: PipelineParams) -> CacheRecord {
    CacheRecord {
        source_path: source.to_string_lossy().into_owned(),
        fingerprint: key.fingerprint.clone(),
        pipeline_id: key.pipeline_id.clone(),
        output,
        created_at_unix: now_unix(),
        params,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::cache_key;
    use crate::params::PipelineParams;

    #[test]
    fn hit_requires_output_file() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.mkv");
        std::fs::write(&src, b"abcd").unwrap();
        let params = PipelineParams::default();
        let key = cache_key(&src, &params).unwrap();
        let mut idx = CacheIndex::open(dir.path().join("cache")).unwrap();
        let out = idx.output_path(&key);
        idx.insert(&key, record_for(&src, &key, out.clone(), params.clone()))
            .unwrap();
        assert!(idx.lookup(&key).is_none(), "missing mkv is not a hit");
        std::fs::write(&out, b"mkv").unwrap();
        assert!(idx.lookup(&key).is_some());
    }
}
