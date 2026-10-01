//! **The feature lowerers of the first image profile** (RFC-0003 §6, activation step 6): a diffusion transformer
//! (SD3 / MMDiT), its VAE decoder and its sampler lowered to `TirProgramV2` stages and integer params. Everything
//! here is new, reusable and **configuration in, programs and a weight binder out**: a typed spec built from the
//! diffusers configs (`GenSpec`, a later file) names the dimensions; the weights are read by tensor name; a real
//! checkpoint changes numbers, not shape.
//!
//! The lowerers are library functions over `misaka-palw-tir`'s builders (the 25 primitives and `tir_library_v1`):
//!
//! * [`tables`] — pinned tables, registration-time data (PALW-EX-5): activation tables on the code grid, the
//!   timestep sinusoid, the sampler's sigma table;
//! * [`sink`] — the integer params a lowering produces, keyed by name and bound to a program's param indices;
//! * [`act`] — `ACT_TABLE_V1`, SiLU and GELU-tanh as `i16` tables on the code grid;
//! * [`ada`] — `MOD_ADALN_V1`, the modulated LayerNorm and the gated residual;
//! * [`fixture`] — the reduced SD3 fixture lowered end to end (shared by the lowerer's and the SDK's tests);
//! * [`calib`] / [`float`] — the per-site calibration and the float reference of the denoiser it is taken from;
//! * [`dit`] — the denoiser stage: patch embed, timestep, blocks, output, Euler update, one program;
//! * [`vae`] — the VAE decoder as a chain of single-position stages (resnet, attention, upsample, head);
//! * [`vae_float`] — the float reference of the VAE decoder, noting the sites the VAE lowering reads;
//! * [`block`] — one joint transformer block assembled from the lowerers;
//! * [`attn`] — `ATTN_JOINT_STREAMS_V1`, MMDiT's joint attention over both streams with committed row statistics;
//! * [`linear`] — a quantised linear map: `i8` weights per output channel, one narrowing per channel;
//! * [`embed`] — `PATCH_EMBED_V1`, `EMBED_TIMESTEP_TABLE_V1`, the conditioning embedders;
//! * [`conv`] — `CONV_DENSE_V1`, a dense 2-D convolution as im2col by a pinned index table and that linear map;
//! * [`sampler`] — the Euler schedule as pinned tables, the initial latent, `GEN_SAMPLER_AFFINE_V1`, unpatchify;
//! * [`stream`] — the one narrowing between code scales (the stream to codes and back);
//! * [`text`] — CLIP's text tower as the pipeline's two text stages (rows and pooled), through the HF frontend;
//! * [`pipeline`] — the text stages, the denoise scan and the VAE chain as one `TirPipelineV1`;
//! * [`norm`] — `NORM_GROUP_SPATIAL_V1`, GroupNorm with committed row partials (no cone reads a whole tensor);
//!
//! **Conventions** (the library's, spec 04b §11): activations are `i16` codes at a calibrated per-site scale
//! (`±32767`) where a MAC reads them, `i32` where they are carried (the residual stream, Q24 unit rows); an
//! accumulator is exact `i64`; a narrowing is `N[lo,hi](x; m, s, z) = clamp(sat64(HAFZ(x·m / 2^s)) + z)`. Every
//! lossy site is named; the float is only ever read at registration, and no float is left in a program.

pub mod act;
pub mod ada;
pub mod attn;
pub mod block;
pub mod calib;
pub mod conv;
pub mod dit;
pub mod embed;
pub mod fixture;
pub mod float;
pub mod linear;
pub mod norm;
pub mod pipeline;
pub mod sampler;
pub mod sink;
pub mod stream;
pub mod tables;
pub mod text;
pub mod vae;
pub mod vae_float;

#[cfg(test)]
pub(crate) mod testkit;
