//! A model on disk, and whether this machine can run it.
//!
//! The second half is the part people actually need. "Llama-3.3-70B-Q4_K_M — 42.5 GB" is a fact;
//! "you have 32 GB, this will not load" is the answer. Getting from one to the other means
//! adding up three things that a file size alone does not contain:
//!
//! * **Weights** — the file, essentially all of it.
//! * **KV cache** — grows linearly with context, and at 128 k context on a 70B model it is not a
//!   rounding error but tens of gigabytes. This is the term a naive "file size vs RAM" check
//!   misses, and it is why models that "should fit" die at the first long conversation.
//! * **Compute overhead** — activations, the graph, the runtime's own scratch buffers.
//!
//! The estimate is deliberately a little pessimistic. Telling someone a model fits and watching
//! their machine swap itself to death costs far more trust than telling them it is tight.

use crate::gguf::GgufMetadata;
use crate::hardware::{AcceleratorKind, HardwareSnapshot};
use crate::provenance::ModelIdentity;
use crate::quant::Quantization;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where a local file came from. Written beside the model as a sidecar when the Studio
/// downloaded it, because the identity the chain derives ([`ModelIdentity`]) binds the repository
/// and revision, and a bare `.gguf` in a folder cannot answer for those.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelSource {
    /// Hugging Face repository, e.g. `bartowski/Qwen3-4B-Instruct-GGUF`.
    pub repo: Option<String>,
    /// Commit sha of that repository, when the download recorded one. A branch name is not a
    /// revision: `main` moves, and two people downloading "the same model" a month apart can get
    /// different bytes.
    pub revision: Option<String>,
    /// Path within the repository.
    pub filename: Option<String>,
    /// The base (unquantized) repository the GGUF was converted from, when the card names one.
    /// Part of the model identity: a different tokenizer is a different function from text to
    /// tokens, whatever the weights say.
    pub base_repo: Option<String>,
    pub base_revision: Option<String>,
    /// `imported` for a file the user pointed at, `huggingface` for a download.
    pub origin: Option<String>,
}

/// One GGUF the Studio knows about.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalModel {
    /// Stable identifier, also the `model` field of the OpenAI API. Derived from the filename
    /// without its extension, so `/v1/chat/completions` takes `Qwen3-4B-Instruct-Q4_K_M`.
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub quantization: Option<Quantization>,
    pub architecture: Option<String>,
    /// Exact, from the tensor index — not inferred from the file size.
    pub parameter_count: Option<u64>,
    pub context_length: Option<u64>,
    pub block_count: Option<u64>,
    /// Experts, for a mixture-of-experts model. Present because MoE breaks the usual intuition:
    /// a 30B-A3B needs 30B of memory and runs at 3B's speed.
    pub expert_count: Option<u64>,
    /// KV bytes per token at f16 — the multiplier behind every context-size estimate.
    pub kv_cache_bytes_per_token: Option<u64>,
    /// Whether the file carries its own chat template.
    pub has_chat_template: bool,
    pub source: ModelSource,
    /// SHA-256 of the file, once computed. Hashing 20 GB is not something to do during a
    /// directory scan, so this is filled in on demand — and it is what unlocks [`Self::identity`].
    pub sha256: Option<String>,
    /// Unix seconds when the file was last modified.
    pub modified_at: Option<u64>,
}

impl LocalModel {
    /// Read a GGUF's header and build the record. Does not hash the file.
    pub fn inspect(path: impl AsRef<Path>, source: ModelSource) -> Result<Self> {
        let path = path.as_ref();
        let display = path.display().to_string();
        let meta = std::fs::metadata(path).map_err(|e| Error::io(&display, e))?;
        let filename = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| display.clone());
        let id = filename.strip_suffix(".gguf").unwrap_or(&filename).to_string();

        let gguf = GgufMetadata::from_path(path)?;
        let quantization = Quantization::detect(gguf.file_type(), &filename);

        Ok(LocalModel {
            name: gguf.name().unwrap_or(&id).to_string(),
            id,
            path: path.to_path_buf(),
            size_bytes: meta.len(),
            quantization,
            architecture: gguf.architecture().map(str::to_string),
            parameter_count: Some(gguf.parameter_count).filter(|&p| p > 0),
            context_length: gguf.context_length(),
            block_count: gguf.block_count(),
            expert_count: gguf.expert_count(),
            kv_cache_bytes_per_token: gguf.kv_cache_bytes_per_token(),
            has_chat_template: gguf.chat_template().is_some(),
            source,
            sha256: None,
            modified_at: meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()),
        })
    }

    /// The chain-compatible model identity, available once the file has been hashed.
    ///
    /// `None` before hashing rather than a placeholder: an identity derived from an unknown
    /// digest would be a number that looks authoritative and answers for nothing.
    pub fn identity(&self) -> Option<ModelIdentity> {
        let sha = self.sha256.as_ref()?;
        let filename = self.path.file_name()?.to_string_lossy().into_owned();
        Some(ModelIdentity::derive(
            sha,
            self.size_bytes,
            &filename,
            self.source.base_repo.as_deref().or(self.source.repo.as_deref()).unwrap_or(""),
            self.source.base_revision.as_deref().or(self.source.revision.as_deref()).unwrap_or(""),
        ))
    }

    /// Memory this model needs at `context_tokens` of context.
    pub fn requirements(&self, context_tokens: u64) -> ModelRequirements {
        ModelRequirements::estimate(self.size_bytes, self.kv_cache_bytes_per_token, context_tokens)
    }

    /// The context length to offer by default: what the model was trained for, capped so the
    /// default configuration is one that actually loads.
    ///
    /// Models now ship 256 k trained context. Defaulting to it on a 16 GB laptop reserves more
    /// KV cache than the machine has and the load fails — with an out-of-memory message that
    /// blames the model rather than the setting. So the default is the largest power-of-two
    /// context whose total still fits, floor 2048.
    pub fn recommended_context(&self, hardware: &HardwareSnapshot) -> u64 {
        let trained = self.context_length.unwrap_or(4096).min(131_072);
        let budget = hardware.best_usable_memory();
        let mut ctx = trained;
        while ctx > 2048 && self.requirements(ctx).total_bytes > budget {
            ctx /= 2;
        }
        ctx.max(2048)
    }

    /// Can this machine run it, and how comfortably.
    pub fn fit(&self, hardware: &HardwareSnapshot, context_tokens: u64) -> FitVerdict {
        FitVerdict::assess(&self.requirements(context_tokens), hardware)
    }
}

/// The memory bill, itemised. Itemised on purpose: "needs 38 GB" invites disbelief, "34 GB of
/// weights plus 4 GB of KV cache at 32 k context" invites lowering the context.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelRequirements {
    pub weights_bytes: u64,
    pub kv_cache_bytes: u64,
    pub overhead_bytes: u64,
    pub total_bytes: u64,
    pub context_tokens: u64,
}

impl ModelRequirements {
    /// Compute buffers and activations: 6 % of the weights, floor 256 MiB.
    ///
    /// A fraction rather than a constant because the scratch a graph needs scales with the model
    /// — measured across llama.cpp loads it lands in the 4-8 % band — and a constant that is
    /// right for a 3B model is wildly wrong for a 70B one in whichever direction it was tuned.
    fn overhead(weights: u64) -> u64 {
        ((weights as f64 * 0.06) as u64).max(256 << 20)
    }

    pub fn estimate(weights_bytes: u64, kv_bytes_per_token: Option<u64>, context_tokens: u64) -> Self {
        // No shape metadata (a header we could not read) still gets an estimate, because a
        // refusal to estimate reads as "unknown" and users hear "fine". 128 KiB/token is a
        // mid-size 7B-class model at f16 — wrong for any specific model, never wildly optimistic.
        let per_token = kv_bytes_per_token.unwrap_or(128 << 10);
        let kv_cache_bytes = per_token.saturating_mul(context_tokens);
        let overhead_bytes = Self::overhead(weights_bytes);
        ModelRequirements {
            weights_bytes,
            kv_cache_bytes,
            overhead_bytes,
            total_bytes: weights_bytes.saturating_add(kv_cache_bytes).saturating_add(overhead_bytes),
            context_tokens,
        }
    }
}

/// Whether it will run, in the terms a person decides with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum FitVerdict {
    /// Fits in accelerator memory with room to spare — full GPU offload, fast.
    Fits { device: String, headroom_bytes: u64 },
    /// Fits, but with little margin. Expect the loader to succeed and other applications to
    /// suffer.
    Tight { device: String, headroom_bytes: u64 },
    /// Too big for the accelerator, small enough for system RAM: it will run with partial
    /// offload, several times slower.
    PartialOffload { device: String, gpu_bytes: u64, needed_bytes: u64 },
    /// Does not fit anywhere. Carries the shortfall so the UI can suggest a smaller
    /// quantization or a shorter context instead of just saying no.
    DoesNotFit { needed_bytes: u64, available_bytes: u64 },
}

impl FitVerdict {
    pub fn assess(req: &ModelRequirements, hardware: &HardwareSnapshot) -> Self {
        let gpu = hardware
            .accelerators
            .iter()
            .filter(|a| a.kind != AcceleratorKind::Cpu)
            .filter_map(|a| a.usable_memory.map(|m| (a.name.clone(), m)))
            .max_by_key(|(_, m)| *m);
        let ram = crate::hardware::cpu_usable_memory(hardware.total_memory);

        if let Some((device, budget)) = gpu {
            if req.total_bytes <= budget {
                let headroom = budget - req.total_bytes;
                // Under 10 % of the budget left is where "it loaded" and "it is usable" start
                // disagreeing, so it gets a different word.
                return if headroom * 10 >= budget {
                    FitVerdict::Fits { device, headroom_bytes: headroom }
                } else {
                    FitVerdict::Tight { device, headroom_bytes: headroom }
                };
            }
            if req.total_bytes <= ram {
                return FitVerdict::PartialOffload { device, gpu_bytes: budget, needed_bytes: req.total_bytes };
            }
            return FitVerdict::DoesNotFit { needed_bytes: req.total_bytes, available_bytes: ram.max(budget) };
        }

        if req.total_bytes <= ram {
            let headroom = ram - req.total_bytes;
            if headroom * 10 >= ram {
                FitVerdict::Fits { device: "system memory".into(), headroom_bytes: headroom }
            } else {
                FitVerdict::Tight { device: "system memory".into(), headroom_bytes: headroom }
            }
        } else {
            FitVerdict::DoesNotFit { needed_bytes: req.total_bytes, available_bytes: ram }
        }
    }

    /// One line for the model card.
    pub fn summary(&self) -> String {
        fn gb(bytes: u64) -> String {
            format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
        }
        match self {
            FitVerdict::Fits { device, headroom_bytes } => format!("Runs on {device} — {} free", gb(*headroom_bytes)),
            FitVerdict::Tight { device, headroom_bytes } => {
                format!("Tight on {device} — only {} to spare", gb(*headroom_bytes))
            }
            FitVerdict::PartialOffload { device, gpu_bytes, needed_bytes } => format!(
                "Needs {}, {} has {} — runs with partial offload, expect it to be slow",
                gb(*needed_bytes),
                device,
                gb(*gpu_bytes)
            ),
            FitVerdict::DoesNotFit { needed_bytes, available_bytes } => {
                format!("Needs {}, this machine has {} — try a smaller quantization", gb(*needed_bytes), gb(*available_bytes))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::Accelerator;

    fn machine(ram_gb: u64, gpu: Option<(&str, u64)>) -> HardwareSnapshot {
        HardwareSnapshot {
            os: "test".into(),
            arch: "x86_64".into(),
            cpu_name: "test cpu".into(),
            physical_cores: Some(8),
            logical_cores: 16,
            total_memory: ram_gb << 30,
            available_memory: ram_gb << 30,
            accelerators: gpu
                .map(|(name, vram_gb)| Accelerator {
                    kind: AcceleratorKind::Cuda,
                    name: name.into(),
                    total_memory: Some(vram_gb << 30),
                    free_memory: Some(vram_gb << 30),
                    usable_memory: Some(vram_gb << 30),
                    driver: None,
                    index: 0,
                })
                .into_iter()
                .collect(),
        }
    }

    /// The whole reason this module exists: a 20 GB model on a 24 GB card "fits" until you ask
    /// for 128 k of context, and then it does not.
    #[test]
    fn context_is_part_of_the_bill() {
        let per_token = 2 * 32 * 8 * 128 * 2; // 32 layers, 8 KV heads, 128-dim heads, f16.
        let small = ModelRequirements::estimate(20 << 30, Some(per_token), 4096);
        let large = ModelRequirements::estimate(20 << 30, Some(per_token), 131_072);
        assert!(large.total_bytes > small.total_bytes + (15 << 30), "128k context costs real memory");

        let gpu = machine(64, Some(("RTX 4090", 24)));
        assert!(matches!(FitVerdict::assess(&small, &gpu), FitVerdict::Fits { .. } | FitVerdict::Tight { .. }));
        assert!(matches!(FitVerdict::assess(&large, &gpu), FitVerdict::PartialOffload { .. }));
    }

    #[test]
    fn a_model_larger_than_the_machine_says_so() {
        let req = ModelRequirements::estimate(140 << 30, Some(1 << 10), 4096);
        let verdict = FitVerdict::assess(&req, &machine(32, Some(("RTX 4090", 24))));
        assert!(matches!(verdict, FitVerdict::DoesNotFit { .. }));
        assert!(verdict.summary().contains("smaller quantization"));
    }

    #[test]
    fn without_a_gpu_the_verdict_is_about_ram() {
        let req = ModelRequirements::estimate(4 << 30, Some(1 << 10), 4096);
        let verdict = FitVerdict::assess(&req, &machine(16, None));
        assert!(matches!(verdict, FitVerdict::Fits { .. }), "got {verdict:?}");
    }

    /// A 256 k default context is a load failure on a laptop. The recommendation has to shrink.
    #[test]
    fn the_recommended_context_shrinks_to_what_fits() {
        let model = LocalModel {
            id: "big".into(),
            name: "big".into(),
            path: PathBuf::from("/models/big.gguf"),
            size_bytes: 8 << 30,
            quantization: None,
            architecture: Some("llama".into()),
            parameter_count: Some(13_000_000_000),
            context_length: Some(262_144),
            block_count: Some(40),
            expert_count: None,
            kv_cache_bytes_per_token: Some(2 * 40 * 8 * 128 * 2),
            has_chat_template: true,
            source: ModelSource::default(),
            sha256: None,
            modified_at: None,
        };
        let laptop = machine(16, None);
        let ctx = model.recommended_context(&laptop);
        assert!((2048..131_072).contains(&ctx), "got {ctx}");
        assert!(model.requirements(ctx).total_bytes <= laptop.best_usable_memory());

        // A workstation should not be punished for the laptop's limits.
        let workstation = machine(256, Some(("H100", 80)));
        assert!(model.recommended_context(&workstation) >= ctx);
    }
}
