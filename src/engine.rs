use std::collections::{HashMap, HashSet};
use std::io::SeekFrom;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context};
use librqbit::{AddTorrent, AddTorrentOptions, ManagedTorrent, Session};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

type Handle = Arc<ManagedTorrent>;

pub struct Engine {
    session: Arc<Session>,
    torrents: Mutex<HashMap<String, Handle>>,
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

impl Engine {
    pub async fn new() -> anyhow::Result<Arc<Self>> {
        let dir = dirs::download_dir().unwrap_or_else(std::env::temp_dir).join("ss-bridge");
        std::fs::create_dir_all(&dir).ok();
        let session = Session::new(dir).await?;
        Ok(Arc::new(Self { session, torrents: Mutex::new(HashMap::new()) }))
    }

    pub async fn add(&self, magnet: &str) -> anyhow::Result<AddResult> {
        let response = self
            .session
            .add_torrent(
                AddTorrent::from_url(magnet),
                Some(AddTorrentOptions { overwrite: true, ..Default::default() }),
            )
            .await?;
        let handle = response.into_handle().ok_or_else(|| anyhow!("no handle"))?;
        handle.wait_until_initialized().await?;

        let id = handle.info_hash().as_string();
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
        drop(guard);

        self.torrents.lock().unwrap().insert(id.clone(), handle);
        Ok(AddResult { id, name, files })
    }

    pub async fn select(&self, id: &str, file_index: usize) -> anyhow::Result<()> {
        let handle = self.handle(id)?;
        let mut only = HashSet::new();
        only.insert(file_index);
        self.session.update_only_files(&handle, &only).await?;
        Ok(())
    }

    pub fn stats(&self, id: &str) -> anyhow::Result<Stats> {
        let handle = self.handle(id)?;
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
        let handle = self.handle(id)?;
        let mut stream = handle.clone().stream(index).context("stream")?;
        stream.seek(SeekFrom::Start(start)).await?;
        let mut buf = vec![0u8; len as usize];
        stream.read_exact(&mut buf).await?;
        Ok(buf)
    }

    pub async fn close(&self, id: &str) {
        let handle = self.torrents.lock().unwrap().remove(id);
        if let Some(handle) = handle {
            let _ = self.session.delete(handle.id().into(), false).await;
        }
    }

    fn handle(&self, id: &str) -> anyhow::Result<Handle> {
        self.torrents.lock().unwrap().get(id).cloned().ok_or_else(|| anyhow!("unknown torrent"))
    }
}
