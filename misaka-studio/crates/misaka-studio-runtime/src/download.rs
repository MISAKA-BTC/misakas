//! Downloads: resumable, verifiable, cancellable.
//!
//! A model download is 2 to 40 GB over a link that will be interrupted. Three properties follow,
//! and all three are the difference between a feature and an annoyance:
//!
//! * **Resume.** Bytes land in `<name>.gguf.part`, and a restart continues with a `Range`
//!   request. Losing 30 GB to a closed laptop lid is not acceptable behaviour.
//! * **Verify.** Hugging Face publishes each LFS object's SHA-256, so a finished file is checked
//!   against a digest from the repository — not against itself. A truncated or corrupted model
//!   is caught here rather than by llama.cpp failing to load with a message about tensors.
//! * **Never appear as a model until it is one.** The `.part` extension is why the scanner never
//!   lists a half-downloaded file: an incomplete model that shows up in the list, loads, and
//!   fails is a bug report about the runtime.
//!
//! Cancelling keeps the partial file. The next attempt resumes it, which is what someone
//! cancelling a download on a metered connection actually wants.

use crate::catalog::Catalog;
use crate::store::{ModelStore, Sidecar};
use crate::{Error, Result};
use misaka_studio_core::model::ModelSource;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::{RwLock, broadcast};

/// Progress is published at most this often, however fast the bytes arrive.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Downloading,
    /// Reading the finished file back to check its digest. Visible because on a 40 GB model it
    /// takes long enough that a silent app looks stuck.
    Verifying,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub id: String,
    pub repo: String,
    pub file: String,
    /// The model id this becomes once complete.
    pub model_id: String,
    pub destination: PathBuf,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub bytes_per_second: f64,
    pub status: DownloadStatus,
    pub error: Option<String>,
}

impl DownloadProgress {
    pub fn percent(&self) -> Option<f64> {
        self.total.filter(|t| *t > 0).map(|t| (self.downloaded as f64 / t as f64) * 100.0)
    }

    /// Seconds left at the current rate. `None` when the size is unknown or nothing is moving —
    /// an ETA computed from a zero rate is `inf`, which renders as garbage.
    pub fn eta_seconds(&self) -> Option<u64> {
        let total = self.total?;
        let remaining = total.saturating_sub(self.downloaded);
        if self.bytes_per_second <= 1.0 {
            return None;
        }
        Some((remaining as f64 / self.bytes_per_second) as u64)
    }
}

struct Job {
    progress: DownloadProgress,
    cancel: Arc<AtomicBool>,
}

/// Every download, running or finished.
pub struct DownloadManager {
    jobs: RwLock<HashMap<String, Job>>,
    events: broadcast::Sender<DownloadProgress>,
    http: reqwest::Client,
}

impl DownloadManager {
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(256);
        DownloadManager {
            jobs: RwLock::new(HashMap::new()),
            events,
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                // No request timeout: this request IS the download, and a 40 GB file over a slow
                // link legitimately takes hours.
                .user_agent(concat!("misaka-studio/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("http client builds"),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DownloadProgress> {
        self.events.subscribe()
    }

    pub async fn list(&self) -> Vec<DownloadProgress> {
        let mut all: Vec<DownloadProgress> = self.jobs.read().await.values().map(|j| j.progress.clone()).collect();
        all.sort_by(|a, b| a.file.cmp(&b.file));
        all
    }

    pub async fn get(&self, id: &str) -> Option<DownloadProgress> {
        self.jobs.read().await.get(id).map(|j| j.progress.clone())
    }

    /// Ask a running download to stop. The partial file is kept for resume.
    pub async fn cancel(&self, id: &str) -> Result<()> {
        let jobs = self.jobs.read().await;
        let job = jobs.get(id).ok_or_else(|| Error::bad_request(format!("no download with id '{id}'")))?;
        job.cancel.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Forget a finished download. Refuses while it is still running — the alternative is a task
    /// writing to a file nothing is tracking.
    pub async fn forget(&self, id: &str) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        match jobs.get(id).map(|j| j.progress.status) {
            Some(DownloadStatus::Downloading) | Some(DownloadStatus::Verifying) => {
                Err(Error::bad_request("that download is still running; cancel it first"))
            }
            Some(_) => {
                jobs.remove(id);
                Ok(())
            }
            None => Err(Error::bad_request(format!("no download with id '{id}'"))),
        }
    }

    /// Begin a download. Returns immediately with the initial progress record.
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        self: &Arc<Self>,
        catalog: &Catalog,
        store: Arc<ModelStore>,
        dest_dir: PathBuf,
        repo: String,
        revision: String,
        file: String,
        expected_sha256: Option<String>,
        expected_size: Option<u64>,
        base_model: Option<String>,
    ) -> Result<DownloadProgress> {
        let file_name = file.rsplit('/').next().unwrap_or(&file).to_string();
        let model_id = file_name.strip_suffix(".gguf").unwrap_or(&file_name).to_string();
        let destination = dest_dir.join(&file_name);
        let id = format!("{repo}/{file}");

        if destination.exists() {
            return Err(Error::bad_request(format!("{} already exists — delete it first to download it again", destination.display())));
        }
        {
            let jobs = self.jobs.read().await;
            if let Some(job) = jobs.get(&id)
                && matches!(job.progress.status, DownloadStatus::Downloading | DownloadStatus::Verifying)
            {
                return Ok(job.progress.clone());
            }
        }

        tokio::fs::create_dir_all(&dest_dir).await.map_err(|e| Error::io(dest_dir.display(), e))?;

        let progress = DownloadProgress {
            id: id.clone(),
            repo: repo.clone(),
            file: file.clone(),
            model_id: model_id.clone(),
            destination: destination.clone(),
            downloaded: 0,
            total: expected_size,
            bytes_per_second: 0.0,
            status: DownloadStatus::Downloading,
            error: None,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.jobs.write().await.insert(id.clone(), Job { progress: progress.clone(), cancel: cancel.clone() });
        let _ = self.events.send(progress.clone());

        let url = catalog.download_url(&repo, &revision, &file);
        let token = catalog.token().map(str::to_string);
        let manager = self.clone();
        let source = ModelSource {
            repo: Some(repo),
            revision: Some(revision),
            filename: Some(file),
            base_repo: base_model.clone(),
            // A card's `base_model` names a repository, never a commit. Recording a revision we
            // do not have would be inventing one, and `h_M` would then be derived from a lie.
            base_revision: None,
            origin: Some("huggingface".into()),
        };

        tokio::spawn(async move {
            let outcome = manager.run(&id, url, token, destination.clone(), expected_sha256, cancel).await;
            match outcome {
                Ok(digest) => {
                    let mut sidecar = Sidecar::load(&destination);
                    sidecar.source = source;
                    sidecar.sha256 = digest;
                    sidecar.hashed_size = tokio::fs::metadata(&destination).await.ok().map(|m| m.len());
                    if let Err(e) = sidecar.save(&destination) {
                        tracing::warn!("could not write the sidecar for {}: {e}", destination.display());
                    }
                    manager.finish(&id, DownloadStatus::Completed, None).await;
                    if let Err(e) = store.refresh().await {
                        tracing::warn!("model rescan after download failed: {e}");
                    }
                }
                Err(Error::Cancelled) => manager.finish(&id, DownloadStatus::Cancelled, None).await,
                Err(e) => manager.finish(&id, DownloadStatus::Failed, Some(e.to_string())).await,
            }
        });

        Ok(progress)
    }

    /// The transfer itself. Returns the verified digest when one could be established.
    async fn run(
        &self,
        id: &str,
        url: String,
        token: Option<String>,
        destination: PathBuf,
        expected_sha256: Option<String>,
        cancel: Arc<AtomicBool>,
    ) -> Result<Option<String>> {
        let part = part_path(&destination);
        let mut resume_from = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);

        let mut request = self.http.get(&url);
        if let Some(token) = &token {
            request = request.bearer_auth(token);
        }
        if resume_from > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
        }

        let response = request.send().await.map_err(|e| Error::Download { message: format!("{url}: {e}") })?;
        if !response.status().is_success() {
            let status = response.status();
            let hint = match status.as_u16() {
                401 | 403 => " — gated or private; accept the licence and add an access token in Settings",
                404 => " — the file is not at that revision",
                416 => " — the server rejected the resume range; delete the .part file and retry",
                _ => "",
            };
            return Err(Error::Download { message: format!("{url} returned {status}{hint}") });
        }

        // A server that ignores `Range` answers 200 with the whole file. Appending it to what we
        // already have would produce a file that is the right size and complete garbage, so the
        // partial data is discarded instead.
        let resumed = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        if resume_from > 0 && !resumed {
            tracing::info!("server ignored the resume range; restarting {}", destination.display());
            resume_from = 0;
        }

        let total = response.content_length().map(|len| len + resume_from);
        self.update(id, |p| {
            p.downloaded = resume_from;
            if p.total.is_none() {
                p.total = total;
            }
        })
        .await;

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(resume_from == 0)
            .append(resume_from > 0)
            .open(&part)
            .await
            .map_err(|e| Error::io(part.display(), e))?;

        let mut downloaded = resume_from;
        let mut last_emit = Instant::now();
        let mut window_start = Instant::now();
        let mut window_bytes = 0u64;
        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;

        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::SeqCst) {
                file.flush().await.ok();
                return Err(Error::Cancelled);
            }
            let chunk = chunk.map_err(|e| Error::Download { message: format!("transfer interrupted: {e}") })?;
            file.write_all(&chunk).await.map_err(|e| Error::io(part.display(), e))?;
            downloaded += chunk.len() as u64;
            window_bytes += chunk.len() as u64;

            if last_emit.elapsed() >= PROGRESS_INTERVAL {
                let rate = window_bytes as f64 / window_start.elapsed().as_secs_f64().max(0.001);
                self.update(id, |p| {
                    p.downloaded = downloaded;
                    p.bytes_per_second = rate;
                })
                .await;
                last_emit = Instant::now();
                window_start = Instant::now();
                window_bytes = 0;
            }
        }
        file.flush().await.map_err(|e| Error::io(part.display(), e))?;
        drop(file);

        let digest = match expected_sha256 {
            Some(expected) => {
                self.update(id, |p| {
                    p.downloaded = downloaded;
                    p.status = DownloadStatus::Verifying;
                })
                .await;
                let path = part.clone();
                let actual = tokio::task::spawn_blocking(move || sha256_file_sync(&path))
                    .await
                    .map_err(|e| Error::Download { message: format!("verification did not run: {e}") })??;
                if !actual.eq_ignore_ascii_case(&expected) {
                    // A file that fails its digest is not kept: resuming it would append to
                    // corrupt bytes forever, and keeping it invites a manual rename into the
                    // model directory.
                    let _ = tokio::fs::remove_file(&part).await;
                    return Err(Error::Download {
                        message: format!("the downloaded file does not match the digest the repository published (expected {expected}, got {actual}); it has been discarded"),
                    });
                }
                Some(actual)
            }
            None => None,
        };

        tokio::fs::rename(&part, &destination).await.map_err(|e| Error::io(destination.display(), e))?;
        self.update(id, |p| p.downloaded = downloaded).await;
        Ok(digest)
    }

    async fn update(&self, id: &str, f: impl FnOnce(&mut DownloadProgress)) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(id) {
            f(&mut job.progress);
            let _ = self.events.send(job.progress.clone());
        }
    }

    async fn finish(&self, id: &str, status: DownloadStatus, error: Option<String>) {
        self.update(id, |p| {
            p.status = status;
            p.error = error;
            p.bytes_per_second = 0.0;
            if status == DownloadStatus::Completed && let Some(total) = p.total {
                p.downloaded = total;
            }
        })
        .await;
    }
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Where partial bytes live. Not a `.gguf`, so the model scanner never sees it.
pub fn part_path(destination: &Path) -> PathBuf {
    let mut name = destination.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    name.push_str(".part");
    destination.with_file_name(name)
}

fn sha256_file_sync(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| Error::io(path.display(), e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 8 << 20];
    loop {
        let read = file.read(&mut buffer).map_err(|e| Error::io(path.display(), e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;

    const BODY: &[u8] = b"a small stand-in for a very large model file";

    /// Serves `BODY`, honouring `Range` — enough to exercise resume without a 4 GB fixture.
    async fn file_server() -> String {
        let app = axum::Router::new().route(
            "/{*path}",
            get(|headers: axum::http::HeaderMap| async move {
                match headers.get(axum::http::header::RANGE).and_then(|v| v.to_str().ok()).and_then(parse_range) {
                    Some(from) if (from as usize) < BODY.len() => (
                        axum::http::StatusCode::PARTIAL_CONTENT,
                        [(axum::http::header::CONTENT_RANGE, format!("bytes {from}-{}/{}", BODY.len() - 1, BODY.len()))],
                        BODY[from as usize..].to_vec(),
                    )
                        .into_response(),
                    _ => BODY.to_vec().into_response(),
                }
            }),
        );
        use axum::response::IntoResponse;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("binds");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{addr}")
    }

    fn parse_range(value: &str) -> Option<u64> {
        value.strip_prefix("bytes=")?.split('-').next()?.parse().ok()
    }

    fn sha256_of(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    async fn wait_for(manager: &DownloadManager, id: &str, want: DownloadStatus) -> DownloadProgress {
        for _ in 0..200 {
            if let Some(p) = manager.get(id).await
                && p.status == want
            {
                return p;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("download never reached {want:?}: {:?}", manager.get(id).await);
    }

    async fn setup() -> (Arc<DownloadManager>, Catalog, Arc<ModelStore>, tempfile::TempDir) {
        let endpoint = file_server().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(ModelStore::new(vec![dir.path().to_path_buf()]));
        (Arc::new(DownloadManager::new()), Catalog::new(endpoint, None), store, dir)
    }

    #[tokio::test]
    async fn a_download_lands_verified_with_its_provenance_sidecar() {
        let (manager, catalog, store, dir) = setup().await;
        let progress = manager
            .start(
                &catalog,
                store.clone(),
                dir.path().to_path_buf(),
                "org/repo".into(),
                "abc123".into(),
                "tiny-Q4_K_M.gguf".into(),
                Some(sha256_of(BODY)),
                Some(BODY.len() as u64),
                Some("org/base".into()),
            )
            .await
            .expect("starts");

        let done = wait_for(&manager, &progress.id, DownloadStatus::Completed).await;
        assert_eq!(done.downloaded, BODY.len() as u64);

        let dest = dir.path().join("tiny-Q4_K_M.gguf");
        assert_eq!(std::fs::read(&dest).expect("read"), BODY);
        assert!(!part_path(&dest).exists(), "the .part file is gone once it is a model");

        let sidecar = Sidecar::load(&dest);
        assert_eq!(sidecar.source.repo.as_deref(), Some("org/repo"));
        assert_eq!(sidecar.source.revision.as_deref(), Some("abc123"));
        assert_eq!(sidecar.source.base_repo.as_deref(), Some("org/base"));
        assert_eq!(sidecar.sha256.as_deref(), Some(sha256_of(BODY).as_str()));
    }

    /// The check that matters: a repository-published digest that does not match must discard
    /// the file rather than leaving a corrupt model to fail at load time.
    #[tokio::test]
    async fn a_digest_mismatch_discards_the_file() {
        let (manager, catalog, store, dir) = setup().await;
        let progress = manager
            .start(
                &catalog,
                store,
                dir.path().to_path_buf(),
                "org/repo".into(),
                "abc123".into(),
                "bad-Q4_K_M.gguf".into(),
                Some("00".repeat(32)),
                Some(BODY.len() as u64),
                None,
            )
            .await
            .expect("starts");

        let failed = wait_for(&manager, &progress.id, DownloadStatus::Failed).await;
        assert!(failed.error.expect("an error").contains("does not match"));
        assert!(!dir.path().join("bad-Q4_K_M.gguf").exists());
        assert!(!part_path(&dir.path().join("bad-Q4_K_M.gguf")).exists());
    }

    /// Resume: a partial file is continued with a Range request, not restarted.
    #[tokio::test]
    async fn an_interrupted_download_resumes_from_the_part_file() {
        let (manager, catalog, store, dir) = setup().await;
        let dest = dir.path().join("resumed-Q4_K_M.gguf");
        std::fs::write(part_path(&dest), &BODY[..10]).expect("write partial");

        let progress = manager
            .start(
                &catalog,
                store,
                dir.path().to_path_buf(),
                "org/repo".into(),
                "abc123".into(),
                "resumed-Q4_K_M.gguf".into(),
                Some(sha256_of(BODY)),
                Some(BODY.len() as u64),
                None,
            )
            .await
            .expect("starts");

        wait_for(&manager, &progress.id, DownloadStatus::Completed).await;
        // The verified digest is the proof: had the range been appended wrongly, or the first
        // ten bytes been fetched twice, the file would not hash to BODY.
        assert_eq!(std::fs::read(&dest).expect("read"), BODY);
    }

    #[tokio::test]
    async fn downloading_over_an_existing_model_is_refused() {
        let (manager, catalog, store, dir) = setup().await;
        std::fs::write(dir.path().join("there-Q4_K_M.gguf"), b"already here").expect("write");
        let err = manager
            .start(
                &catalog,
                store,
                dir.path().to_path_buf(),
                "org/repo".into(),
                "abc".into(),
                "there-Q4_K_M.gguf".into(),
                None,
                None,
                None,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("already exists"), "got {err}");
    }

    #[test]
    fn progress_arithmetic_does_not_produce_infinities() {
        let mut p = DownloadProgress {
            id: "x".into(),
            repo: "r".into(),
            file: "f".into(),
            model_id: "m".into(),
            destination: PathBuf::from("/tmp/f"),
            downloaded: 50,
            total: Some(200),
            bytes_per_second: 0.0,
            status: DownloadStatus::Downloading,
            error: None,
        };
        assert_eq!(p.percent(), Some(25.0));
        assert_eq!(p.eta_seconds(), None, "a stalled transfer has no ETA, not an infinite one");
        p.bytes_per_second = 50.0;
        assert_eq!(p.eta_seconds(), Some(3));
        p.total = None;
        assert_eq!(p.percent(), None);
    }
}
