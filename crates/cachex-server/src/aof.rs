

use cachex_core::config::AofConfig;
use cachex_core::protocol::{encode_command, parse_command, Command};
use crate::storage::CacheStore;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

struct AofInner {
    file: Option<tokio::fs::File>,
}

pub struct Aof {
    inner: Mutex<AofInner>,
    path: PathBuf,
    always_fsync: bool,
    bytes_written: AtomicU64,
    writes: AtomicU64,
    fsyncs: AtomicU64,
    rewrites: AtomicU64,
}

#[derive(Debug, Default)]
pub struct ReplayReport {
    pub commands: u64,
    pub applied: u64,
    pub skipped: u64,
    pub elapsed: Duration,
}

impl Aof {
    pub async fn new(config: &AofConfig) -> std::io::Result<Option<Arc<Aof>>> {
        if !config.enabled {
            return Ok(None);
        }
        let path = PathBuf::from(&config.path);
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        let always_fsync = config.fsync == "always";
        Ok(Some(Arc::new(Aof {
            inner: Mutex::new(AofInner { file: Some(file) }),
            path,
            always_fsync,
            bytes_written: AtomicU64::new(0),
            writes: AtomicU64::new(0),
            fsyncs: AtomicU64::new(0),
            rewrites: AtomicU64::new(0),
        })))
    }

    
    
    pub async fn append(&self, command: &Command) -> std::io::Result<()> {
        match command {
            Command::Set { .. } | Command::Delete { .. } => {}
            _ => return Ok(()),
        }
        let mut line = encode_command(command);
        line.push('\n');
        let len = line.len() as u64;

        let mut inner = self.inner.lock().await;
        if let Some(file) = inner.file.as_mut() {
            file.write_all(line.as_bytes()).await?;
            if self.always_fsync {
                file.sync_all().await?;
                self.fsyncs.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.bytes_written.fetch_add(len, Ordering::Relaxed);
        self.writes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    
    pub async fn sync(&self) -> std::io::Result<()> {
        if self.always_fsync {
            return Ok(());
        }
        let mut inner = self.inner.lock().await;
        if let Some(file) = inner.file.as_mut() {
            file.sync_all().await?;
            self.fsyncs.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    
    
    pub async fn rewrite(
        &self,
        entries: Vec<(String, Vec<u8>, Option<u64>)>,
    ) -> std::io::Result<()> {
        let mut inner = self.inner.lock().await;
        drop(inner.file.take());

        let tmp = self.path.with_extension("aof.tmp");
        let snapshot_bytes = {
            let mut file = tokio::fs::File::create(&tmp).await?;
            let mut total = 0u64;
            for (key, value, ttl) in &entries {
                let cmd = Command::Set {
                    key: key.clone(),
                    value: value.clone(),
                    ttl: *ttl,
                };
                let mut line = encode_command(&cmd);
                line.push('\n');
                total += line.len() as u64;
                file.write_all(line.as_bytes()).await?;
            }
            file.sync_all().await?;
            total
        };

        tokio::fs::rename(&tmp, &self.path).await?;
        inner.file = Some(
            tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .await?,
        );
        self.bytes_written.store(snapshot_bytes, Ordering::Relaxed);
        self.rewrites.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn bytes_written(&self) -> u64 {
        self.bytes_written.load(Ordering::Relaxed)
    }

    pub fn write_count(&self) -> u64 {
        self.writes.load(Ordering::Relaxed)
    }

    pub fn fsync_count(&self) -> u64 {
        self.fsyncs.load(Ordering::Relaxed)
    }

    pub fn rewrite_count(&self) -> u64 {
        self.rewrites.load(Ordering::Relaxed)
    }
}

pub fn replay(path: impl AsRef<Path>, store: &CacheStore) -> std::io::Result<ReplayReport> {
    let start = Instant::now();
    let mut report = ReplayReport::default();

    let path = path.as_ref();
    if !path.exists() {
        report.elapsed = start.elapsed();
        return Ok(report);
    }

    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        report.commands += 1;
        match parse_command(&line) {
            Ok(Command::Set { key, value, ttl }) => {
                store.set(&key, value, ttl);
                report.applied += 1;
            }
            Ok(Command::Delete { key }) => {
                store.delete(&key);
                report.applied += 1;
            }
            _ => report.skipped += 1,
        }
    }

    report.elapsed = start.elapsed();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cachex_core::config::AofConfig;

    fn temp_path(tag: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("cachex-aof-{tag}-{}.aof", std::process::id()));
        path
    }

    fn config(path: &Path, fsync: &str) -> AofConfig {
        AofConfig {
            enabled: true,
            path: path.to_string_lossy().into_owned(),
            fsync: fsync.to_string(),
            fsync_interval_secs: 1,
            rewrite_threshold_bytes: 0,
        }
    }

    #[tokio::test]
    async fn disabled_returns_none() {
        let cfg = AofConfig { enabled: false, ..AofConfig::default() };
        assert!(Aof::new(&cfg).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn append_then_replay_restores_state() {
        let path = temp_path("roundtrip");
        let _ = std::fs::remove_file(&path);

        let aof = Aof::new(&config(&path, "interval")).await.unwrap().unwrap();
        aof.append(&Command::Set { key: "a".into(), value: b"1".to_vec(), ttl: None }).await.unwrap();
        aof.append(&Command::Set { key: "b".into(), value: b"2".to_vec(), ttl: Some(100) }).await.unwrap();
        aof.append(&Command::Delete { key: "a".into() }).await.unwrap();
        aof.sync().await.unwrap();
        drop(aof);

        let store = CacheStore::new(1_000_000);
        let report = replay(&path, &store).unwrap();
        assert_eq!(report.commands, 3);
        assert_eq!(report.applied, 3);
        assert_eq!(store.get("a").0, None);
        assert_eq!(store.get("b").0, Some(b"2".to_vec()));

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn replay_preserves_ttl() {
        let path = temp_path("ttl");
        let _ = std::fs::remove_file(&path);

        let aof = Aof::new(&config(&path, "interval")).await.unwrap().unwrap();
        aof.append(&Command::Set { key: "otp".into(), value: b"1234".to_vec(), ttl: Some(0) }).await.unwrap();
        aof.sync().await.unwrap();
        drop(aof);

        let store = CacheStore::new(1_000_000);
        replay(&path, &store).unwrap();
        assert_eq!(store.get("otp").0, None, "replayed zero-TTL key must be expired");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn replay_skips_unparseable_lines() {
        let path = temp_path("truncated");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, "SET a 1\nBOGUS junk\nSET b 2\nSET d").unwrap();

        let store = CacheStore::new(1_000_000);
        let report = replay(&path, &store).unwrap();
        assert_eq!(report.commands, 4);
        assert_eq!(report.applied, 2);
        assert_eq!(report.skipped, 2, "BOGUS + truncated trailing SET d (missing value)");
        assert_eq!(store.get("a").0, Some(b"1".to_vec()));
        assert_eq!(store.get("b").0, Some(b"2".to_vec()));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn rewrite_produces_replayable_snapshot() {
        let path = temp_path("rewrite");
        let _ = std::fs::remove_file(&path);

        let store = CacheStore::new(1_000_000);
        store.set("a", b"1".to_vec(), None);
        store.set("b", b"2".to_vec(), Some(500));
        let entries = store.snapshot();

        let aof = Aof::new(&config(&path, "always")).await.unwrap().unwrap();
        aof.rewrite(entries).await.unwrap();
        assert_eq!(aof.rewrite_count(), 1);
        drop(aof);

        let fresh = CacheStore::new(1_000_000);
        let report = replay(&path, &fresh).unwrap();
        assert_eq!(report.applied, 2);
        assert_eq!(fresh.get("a").0, Some(b"1".to_vec()));
        assert_eq!(fresh.get("b").0, Some(b"2".to_vec()));
        assert!(fresh.get("b").1 || fresh.ttl_expiration_count() == 0);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn fsync_always_appends_and_flushes() {
        let path = temp_path("always");
        let _ = std::fs::remove_file(&path);

        let aof = Aof::new(&config(&path, "always")).await.unwrap().unwrap();
        aof.append(&Command::Set { key: "k".into(), value: b"v".to_vec(), ttl: None }).await.unwrap();
        assert_eq!(aof.fsync_count(), 1);
        drop(aof);

        let store = CacheStore::new(1_000_000);
        replay(&path, &store).unwrap();
        assert_eq!(store.get("k").0, Some(b"v".to_vec()));
        let _ = std::fs::remove_file(&path);
    }
}