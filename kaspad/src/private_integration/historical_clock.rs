//! Process-local historical time for real isolated native integration. This
//! never constructs a block, changes DAA, skips work, or advances a deadline.
//! The controller must wait for every peer before requesting another slot.
use super::{PrivateProtectedClaimProfileV3, validate_private_probe};
use crate::args::Args;
use kaspa_consensus_core::{config::Config, palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS};
use std::{
    fs,
    io::Read,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ClockCommand {
    version: u16,
    genesis: String,
    timestamp_ms: u64,
}

fn read_command(path: &Path, genesis: &str, genesis_ms: u64, last: Option<u64>, real_now: u64) -> Result<u64, String> {
    if !path.is_absolute() || !fs::symlink_metadata(path).map_err(|e| e.to_string())?.file_type().is_file() {
        return Err("historical clock requires an absolute regular file".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path).map_err(|e| e.to_string())?.take(1025).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > 1024 {
        return Err("historical clock command exceeds 1024 bytes".into());
    }
    let command: ClockCommand = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if command.version != 1 || command.genesis != genesis || !(genesis_ms..=real_now).contains(&command.timestamp_ms) {
        return Err("historical clock must name this genesis and a time between genesis and real wall time".into());
    }
    if last.is_some_and(|prior| command.timestamp_ms < prior || command.timestamp_ms - prior > HEARTBEAT_RECOVERY_INTERVAL_MS) {
        return Err("historical clock update moved backwards or skipped more than one heartbeat interval".into());
    }
    Ok(command.timestamp_ms)
}

fn real_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("clock after UNIX epoch").as_millis() as u64
}

/// Keeping this guard alive holds only this process's clock offset. Monotonic
/// timeouts and the host clock remain real. Invalid updates leave the last
/// offset in place and report refusal; they cannot advance chain observation.
pub struct PrivateHistoricalClockV1 {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    _clock: kaspa_core::time::test_clock::ClockGuard,
}

impl PrivateHistoricalClockV1 {
    pub fn start(args: &Args, config: &Config, path: &Path) -> Result<Self, String> {
        // Both checks precede clock activation: no caller can use this helper
        // on public genesis, an override, a PoW-skipping node or remote peers.
        validate_private_probe(args, config)?;
        PrivateProtectedClaimProfileV3::of_params(&config.params)?
            .ok_or("historical clock requires protected-v3")?
            .check_config_v3(config)?;
        if std::env::var("KASPA_TEST_CLOCK_ISOLATED").as_deref() != Ok("1") || kaspa_core::time::test_clock::active() {
            return Err("historical clock requires an exclusive isolated process scope".into());
        }
        let genesis = config.params.genesis.hash.to_string();
        let genesis_ms = config.params.genesis.timestamp;
        let initial = read_command(path, &genesis, genesis_ms, None, real_now())?;
        let clock = kaspa_core::time::test_clock::start_at(initial);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let path = path.to_owned();
        let worker = thread::Builder::new()
            .name("private-historical-clock".into())
            .spawn(move || {
                let mut last = initial;
                let mut last_error = None;
                while !thread_stop.load(Ordering::SeqCst) {
                    match read_command(&path, &genesis, genesis_ms, Some(last), real_now()) {
                        Ok(next) => {
                            if next != last {
                                kaspa_core::time::test_clock::advance_to(next);
                                eprintln!(
                                    "[private-historical-clock] requested_ms={next} observed_ms={}",
                                    kaspa_core::time::unix_now()
                                );
                                last = next;
                            }
                            last_error = None;
                        }
                        Err(error) => {
                            if last_error.as_ref() != Some(&error) {
                                eprintln!("[private-historical-clock] update refused: {error}");
                                last_error = Some(error);
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { stop, worker: Some(worker), _clock: clock })
    }
}

impl Drop for PrivateHistoricalClockV1 {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_keep_genesis_time_bounds_and_single_slot_progression() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clock.json");
        let genesis = "b9".repeat(64);
        let start = 1_000_000;
        let real = start + 10 * HEARTBEAT_RECOVERY_INTERVAL_MS;
        let write = |id: &str, at| {
            fs::write(&path, serde_json::to_vec(&serde_json::json!({"version": 1, "genesis": id, "timestampMs": at})).unwrap())
                .unwrap();
        };
        for (id, at, last, accepted) in [
            (genesis.as_str(), start, None, true),
            (genesis.as_str(), start + HEARTBEAT_RECOVERY_INTERVAL_MS, Some(start), true),
            (genesis.as_str(), start, Some(start), true),
            (genesis.as_str(), start, Some(start + 1), false),
            (genesis.as_str(), start + HEARTBEAT_RECOVERY_INTERVAL_MS + 1, Some(start), false),
            (genesis.as_str(), start - 1, None, false),
            (genesis.as_str(), real + 1, None, false),
            ("another-genesis", start, None, false),
        ] {
            write(id, at);
            assert_eq!(read_command(&path, &genesis, start, last, real).is_ok(), accepted, "{id} {at} {last:?}");
        }
        fs::write(&path, vec![b' '; 1025]).unwrap();
        assert!(read_command(&path, &genesis, start, None, real).is_err());
        fs::write(&path, b"{\"version\":1").unwrap();
        assert!(read_command(&path, &genesis, start, None, real).is_err());
    }
}
