use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use filetime::FileTime;
use fpv_store::{LibraryStore, ProxyEntry, StoreError};

pub const DEFAULT_MAX_BYTES: i64 = 10 << 30; // 10 GB

/// The playback proxies on disk, held under a byte budget.
///
/// A proxy costs minutes of transcoding and tens of megabytes a flight, and the
/// user rewatches a few flights far more than the rest — hence a cache with
/// least-recently-used eviction rather than one proxy per flight forever.
///
/// **The files are the truth; the index is a cache of them.** The index lives in
/// the library database because LRU needs an access time and the filesystem's is
/// unreliable (atime is commonly disabled). Whenever the two disagree,
/// `reconcile` sides with the disk, and deleting too much costs nothing but a
/// re-transcode.
pub struct ProxyCache<'a> {
    pub directory: PathBuf,
    pub max_bytes: i64,
    store: &'a LibraryStore,
    now: Box<dyn Fn() -> DateTime<Utc> + Send + Sync>,
}

type Freed = (usize, i64);

impl<'a> ProxyCache<'a> {
    /// `directory`: `libraryRoot/proxies`, the sibling of `thumbnails`.
    pub fn new(directory: impl Into<PathBuf>, max_bytes: i64, store: &'a LibraryStore) -> Self {
        Self::with_clock(directory, max_bytes, store, Utc::now)
    }

    /// With an injected clock, so tests can order accesses.
    pub fn with_clock(directory: impl Into<PathBuf>, max_bytes: i64, store: &'a LibraryStore,
                      now: impl Fn() -> DateTime<Utc> + Send + Sync + 'static) -> Self {
        Self { directory: directory.into(), max_bytes, store, now: Box::new(now) }
    }

    /// Where the proxy for `key` lives once registered.
    pub fn path(&self, key: &str) -> PathBuf {
        self.directory.join(format!("{key}.mp4"))
    }

    /// Where to generate a proxy before `register` moves it in: inside the
    /// cache, so the move is a rename, and in a hidden folder, so neither
    /// `reconcile` nor a crash mid-transcode ever adopts a half-written file.
    pub fn incoming_path(&self, key: &str) -> PathBuf {
        self.directory.join(".incoming").join(format!("{key}-{}.mp4", uuid::Uuid::new_v4()))
    }

    /// The proxy for `key` if it is on disk. A hit counts as a use.
    pub fn cached(&self, key: &str) -> Result<Option<PathBuf>, StoreError> {
        let path = self.path(key);
        let Some(bytes) = size(&path) else {
            self.store.delete_proxy(key)?;
            return Ok(None);
        };
        let time = (self.now)();
        let entry = self.store.proxy(key)?.unwrap_or(ProxyEntry {
            key: key.into(), bytes, created_at: time, last_used_at: time,
        });
        self.store.save_proxy(&ProxyEntry { bytes, last_used_at: time, ..entry })?;
        Ok(Some(path))
    }

    /// Adopts a freshly generated proxy, moving it to `path(key)` if it was
    /// written elsewhere. Does not evict: the caller knows what is playing.
    pub fn register(&self, key: &str, source: &Path) -> std::io::Result<PathBuf> {
        let target = self.path(key);
        std::fs::create_dir_all(&self.directory)?;
        // Across file systems a rename cannot work; copy instead.
        if source != target && std::fs::rename(source, &target).is_err() {
            std::fs::copy(source, &target)?;
            std::fs::remove_file(source)?;
        }
        let bytes = size(&target).ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::NotFound, format!("no proxy at {}", target.display())))?;
        let time = (self.now)();
        self.store.save_proxy(&ProxyEntry {
            key: key.into(), bytes, created_at: time, last_used_at: time,
        }).map_err(std::io::Error::other)?;
        Ok(target)
    }

    /// Evicts least-recently-used entries until the budget holds. Never touches
    /// a pinned key — the caller passes whatever is open in the player, or a
    /// second transcode would pull the file out from under it. The budget can
    /// therefore still be exceeded afterwards, by the pinned proxies alone.
    pub fn evict(&self, pinned: &HashSet<String>) -> Result<Freed, StoreError> {
        self.reconcile()?;
        let entries = self.store.proxies()?;
        let mut total: i64 = entries.iter().map(|e| e.bytes).sum();
        let (mut removed, mut freed) = (0, 0);
        for entry in entries {
            if total <= self.max_bytes {
                break;
            }
            if pinned.contains(&entry.key) {
                continue;
            }
            self.remove(&entry)?;
            total -= entry.bytes;
            removed += 1;
            freed += entry.bytes;
        }
        Ok((removed, freed))
    }

    /// Deletes every proxy whose key is not in `keep` — the keys of the flights
    /// the library holds now. Re-segmenting or editing a boundary re-keys a
    /// flight, and without this its old proxy waits for LRU to reach it.
    /// Transcodes still in `.incoming` are untouched, but a key registered after
    /// `keep` was computed is not in it: sweep at open, not while generating.
    pub fn remove_keys_not_in(&self, keep: &HashSet<String>) -> Result<Freed, StoreError> {
        self.reconcile()?;
        let (mut removed, mut freed) = (0, 0);
        for entry in self.store.proxies()? {
            if !keep.contains(&entry.key) {
                self.remove(&entry)?;
                removed += 1;
                freed += entry.bytes;
            }
        }
        Ok((removed, freed))
    }

    /// Deletes every proxy, and anything left half-written.
    pub fn purge_all(&self) -> Result<Freed, StoreError> {
        self.reconcile()?;
        let entries = self.store.proxies()?;
        for entry in &entries {
            self.remove(entry)?;
        }
        let _ = std::fs::remove_dir_all(self.directory.join(".incoming"));
        self.store.delete_all_proxies()?;
        Ok((entries.len(), entries.iter().map(|e| e.bytes).sum()))
    }

    /// Brings the index in line with the disk: rows whose file is gone are
    /// dropped, sizes are refreshed, and proxies with no row are adopted with
    /// their modification time as the last use.
    pub fn reconcile(&self) -> Result<(), StoreError> {
        let mut known = HashSet::new();
        for entry in self.store.proxies()? {
            let Some(bytes) = size(&self.path(&entry.key)) else {
                self.store.delete_proxy(&entry.key)?;
                continue;
            };
            known.insert(entry.key.clone());
            if bytes != entry.bytes {
                self.store.save_proxy(&ProxyEntry { bytes, ..entry })?;
            }
        }
        let Ok(listing) = std::fs::read_dir(&self.directory) else { return Ok(()) };
        for file in listing.flatten() {
            let path = file.path();
            let name = file.file_name().to_string_lossy().into_owned();
            let Some(key) = name.strip_suffix(".mp4") else { continue };
            if name.starts_with('.') || known.contains(key) {
                continue;
            }
            let Some(bytes) = size(&path) else { continue };
            let modified = std::fs::metadata(&path).ok()
                .map(|m| FileTime::from_last_modification_time(&m))
                .and_then(|t| DateTime::from_timestamp(t.unix_seconds(), t.nanoseconds()))
                .unwrap_or_else(|| (self.now)());
            self.store.save_proxy(&ProxyEntry {
                key: key.into(), bytes, created_at: modified, last_used_at: modified,
            })?;
        }
        Ok(())
    }

    pub fn total_bytes(&self) -> Result<i64, StoreError> {
        Ok(self.store.proxies()?.iter().map(|e| e.bytes).sum())
    }

    /// The index, least recently used first.
    pub fn entries(&self) -> Result<Vec<ProxyEntry>, StoreError> {
        self.store.proxies()
    }

    fn remove(&self, entry: &ProxyEntry) -> Result<(), StoreError> {
        let _ = std::fs::remove_file(self.path(&entry.key));
        self.store.delete_proxy(&entry.key)
    }
}

fn size(path: &Path) -> Option<i64> {
    std::fs::metadata(path).ok().filter(|m| m.is_file()).map(|m| m.len() as i64)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI64, Ordering};

    use super::*;

    /// A clock the test advances by hand, so "least recently used" is not a race.
    struct Fixture {
        directory: tempfile::TempDir,
        scratch: tempfile::TempDir,
        store: LibraryStore,
        seconds: Arc<AtomicI64>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                directory: tempfile::tempdir().unwrap(), scratch: tempfile::tempdir().unwrap(),
                store: LibraryStore::in_memory().unwrap(),
                seconds: Arc::new(AtomicI64::new(1_787_443_200)),
            }
        }

        fn cache(&self, max_bytes: i64) -> ProxyCache<'_> {
            let seconds = self.seconds.clone();
            ProxyCache::with_clock(self.directory.path(), max_bytes, &self.store, move || {
                DateTime::from_timestamp(seconds.load(Ordering::SeqCst), 0).unwrap()
            })
        }

        fn tick(&self) {
            self.seconds.fetch_add(60, Ordering::SeqCst);
        }

        /// A dummy proxy of `bytes` bytes, generated outside the cache.
        fn generated(&self, bytes: usize) -> PathBuf {
            let path = self.scratch.path().join(format!("{}.mp4", uuid::Uuid::new_v4()));
            std::fs::write(&path, vec![0u8; bytes]).unwrap();
            path
        }

        fn on_disk(&self, key: &str) -> bool {
            self.directory.path().join(format!("{key}.mp4")).exists()
        }

        fn write_incoming(&self, cache: &ProxyCache, key: &str, bytes: usize) -> PathBuf {
            let incoming = cache.incoming_path(key);
            std::fs::create_dir_all(incoming.parent().unwrap()).unwrap();
            std::fs::write(&incoming, vec![0u8; bytes]).unwrap();
            incoming
        }
    }

    fn keys(cache: &ProxyCache) -> Vec<String> {
        cache.entries().unwrap().into_iter().map(|e| e.key).collect()
    }

    fn set(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn registers_a_proxy_and_finds_it_again() {
        let f = Fixture::new();
        let cache = f.cache(1000);
        assert_eq!(cache.cached("a").unwrap(), None);
        let path = cache.register("a", &f.generated(300)).unwrap();
        assert_eq!(path, cache.path("a"));
        assert_eq!(cache.cached("a").unwrap(), Some(path));
        assert_eq!(cache.total_bytes().unwrap(), 300);
    }

    #[test]
    fn evicts_least_recently_used_first_until_the_budget_holds() {
        let f = Fixture::new();
        let cache = f.cache(1000);
        for key in ["a", "b", "c", "d"] {
            cache.register(key, &f.generated(400)).unwrap();
            f.tick();
        }
        // Watching "a" again makes "b" the oldest.
        cache.cached("a").unwrap();
        assert_eq!(cache.evict(&set(&[])).unwrap(), (2, 800));
        assert!(!f.on_disk("b") && !f.on_disk("c") && f.on_disk("a") && f.on_disk("d"));
        assert_eq!(keys(&cache), ["d", "a"]);
    }

    #[test]
    fn never_evicts_the_pinned_proxy_even_when_it_is_the_oldest() {
        let f = Fixture::new();
        let cache = f.cache(1000);
        for key in ["playing", "b", "c"] {
            cache.register(key, &f.generated(600)).unwrap();
            f.tick();
        }
        assert_eq!(cache.evict(&set(&["playing"])).unwrap().0, 2);
        assert!(f.on_disk("playing") && !f.on_disk("b"));
        assert_eq!(keys(&cache), ["playing"]);
    }

    #[test]
    fn leaves_the_budget_exceeded_rather_than_evict_a_pinned_proxy() {
        let f = Fixture::new();
        let cache = f.cache(100);
        cache.register("playing", &f.generated(500)).unwrap();
        assert_eq!(cache.evict(&set(&["playing"])).unwrap().0, 0);
        assert!(f.on_disk("playing"));
    }

    #[test]
    fn purges_everything_including_half_written_files() {
        let f = Fixture::new();
        let cache = f.cache(10_000);
        cache.register("a", &f.generated(100)).unwrap();
        cache.register("b", &f.generated(200)).unwrap();
        f.write_incoming(&cache, "c", 50);
        assert_eq!(cache.purge_all().unwrap(), (2, 300));
        assert_eq!(cache.total_bytes().unwrap(), 0);
        assert_eq!(std::fs::read_dir(f.directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn reconciles_the_index_with_the_disk() {
        let f = Fixture::new();
        let cache = f.cache(10_000);
        cache.register("gone", &f.generated(100)).unwrap();
        cache.register("kept", &f.generated(100)).unwrap();
        // Deleted behind the cache's back, grown in place, and one it never saw.
        std::fs::remove_file(cache.path("gone")).unwrap();
        std::fs::write(cache.path("kept"), vec![0u8; 250]).unwrap();
        std::fs::write(cache.path("stray"), vec![0u8; 70]).unwrap();
        // A transcode in progress is not a proxy yet.
        f.write_incoming(&cache, "busy", 999);
        cache.reconcile().unwrap();
        let mut found = keys(&cache);
        found.sort();
        assert_eq!(found, ["kept", "stray"]);
        assert_eq!(cache.total_bytes().unwrap(), 320);
    }

    #[test]
    fn forgets_a_hit_whose_file_has_vanished() {
        let f = Fixture::new();
        let cache = f.cache(10_000);
        cache.register("a", &f.generated(100)).unwrap();
        std::fs::remove_file(cache.path("a")).unwrap();
        assert_eq!(cache.cached("a").unwrap(), None);
        assert!(cache.entries().unwrap().is_empty());
    }

    #[test]
    fn removes_proxies_of_flights_the_library_no_longer_has() {
        let f = Fixture::new();
        let cache = f.cache(10_000);
        cache.register("current", &f.generated(100)).unwrap();
        cache.register("old", &f.generated(200)).unwrap();
        std::fs::write(cache.path("stray"), vec![0u8; 50]).unwrap();
        let incoming = f.write_incoming(&cache, "busy", 10);
        assert_eq!(cache.remove_keys_not_in(&set(&["current", "not-generated-yet"])).unwrap(),
                   (2, 250));
        assert!(f.on_disk("current") && !f.on_disk("old") && !f.on_disk("stray"));
        assert!(incoming.exists());
        assert_eq!(keys(&cache), ["current"]);
    }
}
