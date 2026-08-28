//! The performance monitor: hardware counters and generation throughput, in one stream.
//!
//! Two numbers decide whether a local model is usable — how fast it generates, and how close the
//! machine is to its memory limit — and they are only meaningful together. 12 tokens/sec at 40 %
//! of VRAM means try a bigger model; 12 tokens/sec at 99 % means the KV cache is spilling and
//! the fix is a shorter context. So one sampler produces both and one stream carries them.
//!
//! # Nobody watching, nobody sampling
//!
//! The sampler wakes only while something is subscribed. A desktop app that polls `nvidia-smi`
//! twice a second forever is a measurable battery cost for a window nobody is looking at.

use misaka_studio_core::hardware::{HardwareMonitor, HardwareSample, HardwareSnapshot};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, broadcast};

/// How often the monitor samples while someone is watching.
pub const SAMPLE_INTERVAL: Duration = Duration::from_millis(1000);

/// Throughput, as last measured.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct GenerationStats {
    /// Generations in flight.
    pub active: u64,
    /// Tokens per second of the most recent completed generation.
    pub last_tokens_per_second: f64,
    /// Time to first token of the most recent generation, in milliseconds.
    pub last_time_to_first_token_ms: u64,
    /// Completion tokens produced since the process started.
    pub total_tokens: u64,
    /// Completed generations since the process started.
    pub total_generations: u64,
}

/// One tick of the monitor.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeSample {
    pub hardware: HardwareSample,
    pub generation: GenerationStats,
}

/// Collects and publishes samples.
pub struct MetricsHub {
    monitor: Mutex<HardwareMonitor>,
    events: broadcast::Sender<RuntimeSample>,

    // Counters, atomic because generation tasks touch them from anywhere.
    active: AtomicU64,
    total_tokens: AtomicU64,
    total_generations: AtomicU64,
    // f64 has no atomic; the bits do, which is the standard trick and avoids a lock on a value
    // written once per generation and read once per second.
    last_tps_bits: AtomicU64,
    last_ttft_ms: AtomicU64,
}

impl MetricsHub {
    pub fn new(snapshot: &HardwareSnapshot) -> Arc<Self> {
        let (events, _) = broadcast::channel(64);
        Arc::new(MetricsHub {
            monitor: Mutex::new(HardwareMonitor::new(snapshot)),
            events,
            active: AtomicU64::new(0),
            total_tokens: AtomicU64::new(0),
            total_generations: AtomicU64::new(0),
            last_tps_bits: AtomicU64::new(0),
            last_ttft_ms: AtomicU64::new(0),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RuntimeSample> {
        self.events.subscribe()
    }

    pub fn stats(&self) -> GenerationStats {
        GenerationStats {
            active: self.active.load(Ordering::Relaxed),
            last_tokens_per_second: f64::from_bits(self.last_tps_bits.load(Ordering::Relaxed)),
            last_time_to_first_token_ms: self.last_ttft_ms.load(Ordering::Relaxed),
            total_tokens: self.total_tokens.load(Ordering::Relaxed),
            total_generations: self.total_generations.load(Ordering::Relaxed),
        }
    }

    /// Take one sample now.
    pub async fn sample(&self) -> RuntimeSample {
        let hardware = self.monitor.lock().await.sample();
        RuntimeSample { hardware, generation: self.stats() }
    }

    pub fn generation_started(&self) {
        self.active.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a finished generation. Called even when it failed part-way, so `active` cannot
    /// leak — a counter that only decrements on success reads "3 active" forever after three
    /// cancelled requests.
    pub fn generation_finished(&self, tokens: u64, tokens_per_second: f64, time_to_first_token_ms: u64) {
        // `fetch_update` rather than `fetch_sub`: an unsigned counter that underflows reads as
        // 18 quintillion active generations.
        let _ = self.active.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_sub(1)));
        if tokens > 0 {
            self.total_tokens.fetch_add(tokens, Ordering::Relaxed);
            self.total_generations.fetch_add(1, Ordering::Relaxed);
            self.last_tps_bits.store(tokens_per_second.to_bits(), Ordering::Relaxed);
            self.last_ttft_ms.store(time_to_first_token_ms, Ordering::Relaxed);
        }
    }

    /// Run the sampling loop until the process ends.
    pub async fn run(self: Arc<Self>) {
        let mut ticker = tokio::time::interval(SAMPLE_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if self.events.receiver_count() == 0 {
                continue;
            }
            let sample = self.sample().await;
            let _ = self.events.send(sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn counters_survive_more_finishes_than_starts() {
        let hub = MetricsHub::new(&HardwareSnapshot::probe());
        hub.generation_started();
        hub.generation_finished(10, 12.5, 100);
        // A double-finish (a cancelled request whose cleanup ran twice) must not underflow.
        hub.generation_finished(0, 0.0, 0);
        let stats = hub.stats();
        assert_eq!(stats.active, 0);
        assert_eq!(stats.total_tokens, 10);
        assert_eq!(stats.total_generations, 1, "a zero-token generation is not counted");
        assert_eq!(stats.last_tokens_per_second, 12.5);
    }

    #[tokio::test]
    async fn a_sample_carries_both_halves() {
        let hub = MetricsHub::new(&HardwareSnapshot::probe());
        hub.generation_started();
        let sample = hub.sample().await;
        assert!(sample.hardware.memory_total > 0);
        assert_eq!(sample.generation.active, 1);
    }

    #[tokio::test]
    async fn subscribers_receive_ticks() {
        let hub = MetricsHub::new(&HardwareSnapshot::probe());
        let mut rx = hub.subscribe();
        tokio::spawn(hub.clone().run());
        let sample = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.expect("a tick arrives").expect("no lag");
        assert!(sample.hardware.memory_total > 0);
    }
}
