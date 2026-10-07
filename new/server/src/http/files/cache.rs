use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

use bytes::Bytes;
use http::header::HeaderValue;

use crate::core::cache::{Weigh, Weighted};

impl Key {

    pub fn path ( &self ) -> &Path {

        match self { Self::File(path) | Self::Dir(path) => path }

    }

    pub fn into_path ( self ) -> PathBuf {

        match self { Self::File(path) | Self::Dir(path) => path }

    }

}
use super::arch::{FileCache, Files, Key, Loaded, PACKED, Packed};

impl Weigh for Arc<Loaded> {

    fn weight ( &self ) -> u64 {

        self.bytes.len() as u64 + self.packed.iter().map(|packed| packed.bytes.len() as u64).sum::<u64>() + 160

    }

}

impl FileCache {

    pub fn new ( items: usize, bytes: u64, max_file_bytes: u64, valid_ms: u64, precompressed: bool ) -> Self {

        Self { store: Weighted::new(items, bytes), max_file_bytes: max_file_bytes.min(bytes), valid_ms, precompressed }

    }

    async fn packed ( &self, path: &Path, modified: Option<u64> ) -> Vec<Packed> {

        let mut packed = Vec::new();

        for ( suffix, coding ) in PACKED {

            let mut sibling = path.as_os_str().to_owned();

            sibling.push(".");
            sibling.push(suffix);

            let Ok(meta) = tokio::fs::metadata(&sibling).await else { continue; };

            if !meta.is_file() || meta.len() > self.max_file_bytes || Self::seconds(&meta) < modified { continue; }

            let Ok(bytes) = tokio::fs::read(&sibling).await else { continue; };

            packed.push(Packed { coding: HeaderValue::from_static(coding), etag: Files::etag(Self::seconds(&meta), meta.len()), bytes: Bytes::from(bytes) });

        }

        packed

    }

    pub fn fresh ( &self, key: &Key, now_ms: u64 ) -> Option<Arc<Loaded>> {

        let loaded = self.store.get(key)?;

        (now_ms.saturating_sub(loaded.checked.load(Ordering::Relaxed)) < self.valid_ms).then_some(loaded)

    }

    pub async fn load ( &self, path: &Path, meta: &Metadata, now_ms: u64 ) -> std::io::Result<Option<Arc<Loaded>>> {

        if meta.len() > self.max_file_bytes { return Ok(None); }

        let modified = Self::seconds(meta);
        let key = Key::File(path.to_path_buf());

        if let Some(loaded) = self.store.get(&key) && loaded.length == meta.len() && loaded.modified == modified {

            loaded.checked.store(now_ms, Ordering::Relaxed);

            return Ok(Some(loaded));

        }

        let bytes = tokio::fs::read(path).await?;

        if bytes.len() as u64 != meta.len() { return Ok(None); }

        let packed = if self.precompressed { self.packed(path, modified).await } else { Vec::new() };
        let loaded = Arc::new(Loaded { bytes: Bytes::from(bytes), length: meta.len(), modified, etag: Files::etag(modified, meta.len()), kind: Files::kind(path), checked: AtomicU64::new(now_ms), packed });

        self.store.insert(key, loaded.clone());

        Ok(Some(loaded))

    }

    pub fn alias ( &self, key: Key, loaded: &Arc<Loaded> ) {

        self.store.insert(key, loaded.clone());

    }

    pub fn forget ( &self, key: &Key ) {

        self.store.remove(key);

    }

    pub fn clear ( &self ) {

        self.store.clear();

    }

    pub fn entries ( &self ) -> usize {

        self.store.len()

    }

    pub fn bytes ( &self ) -> u64 {

        self.store.weight()

    }

    pub fn seconds ( meta: &Metadata ) -> Option<u64> {

        meta.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map(|since| since.as_secs())

    }

}
