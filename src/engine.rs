use std::collections::{HashMap, HashSet};
use std::io::SeekFrom;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use librqbit::{AddTorrent, AddTorrentOptions, Magnet, ManagedTorrent, Session, SessionOptions};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::task::AbortHandle;

type Handle = Arc<ManagedTorrent>;

const TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.tracker.cl:1337/announce",
    "udp://open.demonii.com:1337/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://exodus.desync.com:6969/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.tiny-vps.com:6969/announce",
    "udp://explodie.org:6969/announce",
    "udp://tracker.dler.org:6969/announce",
    "udp://tracker.openbittorrent.com:6969/announce",
    "udp://opentracker.i2p.rocks:6969/announce",
    "udp://tracker.moeking.me:6969/announce",
    "https://tracker.tamersunion.org:443/announce",
    "udp://tracker1.bt.moack.co.kr:80/announce",
    "udp://tracker.bittor.pw:1337/announce",
];

const DEFAULT_DELETE_DELAY: Duration = Duration::from_secs(180);
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const SWEEP_INTERVAL: Duration = Duration::from_secs(5);

struct Entry {
    handle: Handle,
    refs: usize,
    last_active: Instant,
    pending_delete: Option<AbortHandle>,
}

pub struct Engine {
    session: Arc<Session>,
    torrents: Arc<Mutex<HashMap<String, Entry>>>,
    delete_delay: Duration,
    idle_timeout: Duration,
}

#[derive(Serialize)]
pub struct FileEntry {
    pub index: usize,
    pub name: String,
    pub path: String,
    pub size: u64,
}

#[derive(Serialize)]
pub struct AddResult {
    pub id: String,
    pub name: String,
    pub files: Vec<FileEntry>,
}

#[derive(Serialize)]
pub struct Stats {
    pub peers: u64,
    #[serde(rename = "downloadSpeed")]
    pub download_speed: u64,
    pub downloaded: u64,
    pub progress: f64,
}

fn env_duration(name: &str, default: Duration) -> Duration {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(default)
}

fn magnet_hash(magnet: &str) -> Option<String> {
    Magnet::parse(magnet).ok()?.as_id20().map(|hash| hash.as_string())
}

fn add_result(handle: &Handle) -> anyhow::Result<AddResult> {
    let guard = handle.metadata.load();
    let meta = guard.as_ref().ok_or_else(|| anyhow!("no metadata"))?;
    let name = meta.name.clone().unwrap_or_else(|| "torrent".to_string());
    let files = meta
        .file_infos
        .iter()
        .enumerate()
        .map(|(index, info)| {
            let path = info.relative_filename.to_string_lossy().to_string();
            let name = info
                .relative_filename
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            FileEntry { index, name, path, size: info.len }
        })
        .collect();
    Ok(AddResult { id: handle.info_hash().as_string(), name, files })
}

impl Engine {
    pub async fn new() -> anyhow::Result<Arc<Self>> {
        let dir = dirs::download_dir().unwrap_or_else(std::env::temp_dir).join("ss-bridge");
        std::fs::create_dir_all(&dir).ok();
        // Maximise peers: DHT on by default, plus a listen port and UPnP for
        // incoming connections, fast-resume, and a big pack of public trackers
        // added to every torrent so peer discovery is fast and wide.
        let mut opts = SessionOptions::default();
        opts.enable_upnp_port_forwarding = true;
        opts.listen_port_range = Some(4240..4260);
        opts.fastresume = true;
        opts.trackers = TRACKERS.iter().filter_map(|t| url::Url::parse(t).ok()).collect();
        let session = Session::new_with_opts(dir, opts).await?;
        let engine = Arc::new(Self {
            session,
            torrents: Arc::new(Mutex::new(HashMap::new())),
            delete_delay: env_duration("SS_BRIDGE_DELETE_DELAY_SECS", DEFAULT_DELETE_DELAY),
            idle_timeout: env_duration("SS_BRIDGE_IDLE_SECS", DEFAULT_IDLE_TIMEOUT),
        });
        let weak = Arc::downgrade(&engine);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(SWEEP_INTERVAL).await;
                let Some(engine) = weak.upgrade() else { break };
                engine.sweep_idle().await;
            }
        });
        Ok(engine)
    }

    pub async fn add(&self, magnet: &str) -> anyhow::Result<AddResult> {
        if let Some(hash) = magnet_hash(magnet) {
            let existing = {
                let mut map = self.torrents.lock().unwrap();
                map.get_mut(&hash).map(|entry| {
                    if let Some(pending) = entry.pending_delete.take() {
                        pending.abort();
                    }
                    entry.refs += 1;
                    entry.last_active = Instant::now();
                    entry.handle.clone()
                })
            };
            if let Some(handle) = existing {
                let _ = self.session.unpause(&handle).await;
                return add_result(&handle);
            }
        }

        let response = self
            .session
            .add_torrent(
                AddTorrent::from_url(magnet),
                Some(AddTorrentOptions { overwrite: true, ..Default::default() }),
            )
            .await?;
        let handle = response.into_handle().ok_or_else(|| anyhow!("no handle"))?;
        handle.wait_until_initialized().await?;

        let result = add_result(&handle)?;
        let mut map = self.torrents.lock().unwrap();
        match map.get_mut(&result.id) {
            Some(entry) => {
                entry.refs += 1;
                entry.last_active = Instant::now();
            }
            None => {
                map.insert(
                    result.id.clone(),
                    Entry { handle, refs: 1, last_active: Instant::now(), pending_delete: None },
                );
            }
        }
        Ok(result)
    }

    pub async fn select(&self, id: &str, file_index: usize) -> anyhow::Result<()> {
        let handle = self.touch(id)?;
        let mut only = HashSet::new();
        only.insert(file_index);
        self.session.update_only_files(&handle, &only).await?;
        Ok(())
    }

    pub fn stats(&self, id: &str) -> anyhow::Result<Stats> {
        let handle = self.touch(id)?;
        let stats = handle.stats();
        let total = stats.total_bytes.max(1);
        let (peers, speed) = match &stats.live {
            Some(live) => (
                live.snapshot.peer_stats.live as u64,
                (live.download_speed.mbps * 1_048_576.0) as u64,
            ),
            None => (0, 0),
        };
        Ok(Stats {
            peers,
            download_speed: speed,
            downloaded: stats.progress_bytes,
            progress: stats.progress_bytes as f64 / total as f64,
        })
    }

    pub fn file_size(&self, id: &str, index: usize) -> anyhow::Result<u64> {
        let handle = self.handle(id)?;
        let guard = handle.metadata.load();
        let meta = guard.as_ref().ok_or_else(|| anyhow!("no metadata"))?;
        meta.file_infos.get(index).map(|f| f.len).ok_or_else(|| anyhow!("no such file"))
    }

    pub async fn read_range(&self, id: &str, index: usize, start: u64, len: u64) -> anyhow::Result<Vec<u8>> {
        let handle = self.touch(id)?;
        let mut stream = handle.clone().stream(index).context("stream")?;
        stream.seek(SeekFrom::Start(start)).await?;
        let mut buf = vec![0u8; len as usize];
        stream.read_exact(&mut buf).await?;
        Ok(buf)
    }

    pub async fn close(&self, id: &str) {
        let handle = {
            let mut map = self.torrents.lock().unwrap();
            match map.get_mut(id) {
                Some(entry) => {
                    entry.refs = entry.refs.saturating_sub(1);
                    if entry.refs > 0 {
                        return;
                    }
                    entry.handle.clone()
                }
                None => return,
            }
        };
        let _ = self.session.pause(&handle).await;
        self.schedule_delete(id, &handle);
    }

    fn schedule_delete(&self, id: &str, handle: &Handle) {
        let torrents = self.torrents.clone();
        let session = self.session.clone();
        let rqbit_id = handle.id();
        let key = id.to_string();
        let delay = self.delete_delay;
        let task = tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let gone = torrents.lock().unwrap().remove(&key).is_some();
            if gone {
                let _ = session.delete(rqbit_id.into(), true).await;
            }
        });
        let mut map = self.torrents.lock().unwrap();
        if let Some(entry) = map.get_mut(id) {
            entry.pending_delete = Some(task.abort_handle());
        } else {
            task.abort();
        }
    }

    async fn sweep_idle(&self) {
        let stale: Vec<(String, Handle)> = {
            let mut map = self.torrents.lock().unwrap();
            map.iter_mut()
                .filter(|(_, entry)| {
                    entry.pending_delete.is_none() && entry.last_active.elapsed() > self.idle_timeout
                })
                .map(|(id, entry)| {
                    entry.refs = 0;
                    (id.clone(), entry.handle.clone())
                })
                .collect()
        };
        for (id, handle) in stale {
            let _ = self.session.pause(&handle).await;
            self.schedule_delete(&id, &handle);
        }
    }

    fn touch(&self, id: &str) -> anyhow::Result<Handle> {
        let mut map = self.torrents.lock().unwrap();
        let entry = map.get_mut(id).ok_or_else(|| anyhow!("unknown torrent"))?;
        entry.last_active = Instant::now();
        Ok(entry.handle.clone())
    }

    fn handle(&self, id: &str) -> anyhow::Result<Handle> {
        self.torrents.lock().unwrap().get(id).map(|e| e.handle.clone()).ok_or_else(|| anyhow!("unknown torrent"))
    }
}
