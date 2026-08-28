//! The MISAKA runtime backend — reserved, and refusing.
//!
//! This is where the deterministic runtime this repository already carries for PALW
//! (`misaka-palw-base0`, `misaka-palw-reference2`) will be driven from: a class whose execution is
//! adjudicable, under the same UI, recorded in the same shape as every other backend.
//!
//! Until then it **refuses to run** rather than quietly handing the work to llama.cpp. The
//! substitution would be invisible in exactly the place it matters: a user who selected the MISAKA
//! runtime would get a record naming `misaka` as its backend, an `h_R` derived from llama.cpp's
//! identity, and a determinism class the execution does not belong to. A backend that cannot run
//! must say so before anything is loaded, which is what [`Availability`] is for.

use super::{Availability, GenerationRequest, InferenceBackend, LoadRequest, LoadedModel, StreamEvent};
use crate::{Error, Result};
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use misaka_studio_core::provenance::RuntimeDescriptor;

/// The class tag the in-tree runtime will register under. Named here so the descriptor this
/// backend reports is already the right shape when it starts working.
pub const MISAKA_CLASS_TAG: &str = "misaka-palw-base0/deterministic-integer/v1";

const REASON: &str = "the MISAKA deterministic runtime is not yet wired into the Studio";
const REMEDY: &str = "Choose llama.cpp (or Auto) in Settings. This backend is reserved for the in-tree PALW runtime and will refuse to run until it is connected — it will not silently use a different engine.";

pub struct MisakaBackend;

impl InferenceBackend for MisakaBackend {
    fn name(&self) -> &'static str {
        "misaka"
    }

    fn descriptor(&self) -> BoxFuture<'_, RuntimeDescriptor> {
        Box::pin(async {
            RuntimeDescriptor {
                backend: "misaka".into(),
                engine_commit: "unimplemented".into(),
                engine_patch_sha256: "unimplemented".into(),
                engine_build_number: 0,
                build_profile: "unimplemented".into(),
                class_tag: MISAKA_CLASS_TAG.into(),
            }
        })
    }

    fn availability(&self) -> BoxFuture<'_, Availability> {
        Box::pin(async { Availability::Unavailable { reason: REASON.into(), remedy: REMEDY.into() } })
    }

    fn load(&self, _request: LoadRequest) -> BoxFuture<'_, Result<LoadedModel>> {
        Box::pin(async { Err(Error::BackendUnavailable { backend: "misaka".into(), reason: REASON.into(), remedy: REMEDY.into() }) })
    }

    fn unload(&self) -> BoxFuture<'_, Result<()>> {
        Box::pin(async { Ok(()) })
    }

    fn loaded(&self) -> BoxFuture<'_, Option<LoadedModel>> {
        Box::pin(async { None })
    }

    fn generate(&self, _request: GenerationRequest) -> BoxFuture<'_, Result<BoxStream<'static, Result<StreamEvent>>>> {
        Box::pin(async { Err(Error::BackendUnavailable { backend: "misaka".into(), reason: REASON.into(), remedy: REMEDY.into() }) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_refuses_before_anything_is_loaded_and_says_why() {
        let backend = MisakaBackend;
        match backend.availability().await {
            Availability::Unavailable { reason, remedy } => {
                assert!(reason.contains("not yet wired"));
                assert!(remedy.contains("llama.cpp"), "the remedy names what to use instead");
            }
            other => panic!("expected unavailable, got {other:?}"),
        }
    }

    /// The substitution this backend exists to prevent: loading must fail rather than succeed on
    /// another engine.
    #[tokio::test]
    async fn loading_fails_rather_than_falling_back() {
        let backend = MisakaBackend;
        let result = backend
            .load(LoadRequest {
                model_id: "m".into(),
                model_path: "/models/m.gguf".into(),
                context_size: 4096,
                gpu_layers: None,
                threads: None,
                flash_attention: false,
                use_mmap: true,
                use_mlock: false,
                extra_args: Vec::new(),
            })
            .await;
        assert!(matches!(result, Err(Error::BackendUnavailable { .. })));
        assert!(backend.loaded().await.is_none());
    }

    /// Its determinism class must not collide with any engine the Studio actually drives.
    #[tokio::test]
    async fn its_class_is_its_own() {
        let descriptor = MisakaBackend.descriptor().await;
        assert_eq!(descriptor.class_tag, MISAKA_CLASS_TAG);
        assert_ne!(descriptor.class_tag, super::super::mock::MOCK_CLASS_TAG);
        assert!(!descriptor.class_tag.contains("llamacpp"));
    }
}
