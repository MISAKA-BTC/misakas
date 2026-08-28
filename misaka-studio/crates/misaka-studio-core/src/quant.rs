//! Quantization: **what the trade actually was**.
//!
//! A person choosing between `…-Q4_K_M.gguf` and `…-IQ3_XXS.gguf` is choosing between two
//! numbers they cannot see — how much memory it needs, and how much of the model survived. This
//! module is the table that turns the tag into both.
//!
//! Two sources, in order of trust:
//!
//! 1. **`general.file_type` in the GGUF header** — stamped by the converter. Authoritative.
//! 2. **The filename** — a convention, and a convention people rename. Used only when the header
//!    has no file type, which happens for hand-converted files.
//!
//! The bits-per-weight figures are *effective* rates over a whole model, from llama.cpp's own
//! quantization tables: a K-quant mixes block types by tensor role (attention output and the
//! `ffn_down` projections keep more bits than the rest), so `Q4_K_M` is 4.85 bpw and not 4.

use serde::{Deserialize, Serialize};

/// How a quantization was built. The family is what predicts behaviour when the exact tag is
/// unknown — an `IQ*` needs an importance matrix and decodes more slowly; a `Q*_K` does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantFamily {
    /// Unquantized: F32, F16, BF16.
    Float,
    /// The original round-to-nearest block quants: `Q4_0`, `Q5_1`, `Q8_0`.
    Legacy,
    /// K-quants: per-tensor-role mixed precision. The default choice for most models.
    KQuant,
    /// I-quants: codebook quantization, calibrated with an importance matrix. Smaller at equal
    /// quality, slower to decode, and only as good as the calibration data.
    IQuant,
    /// Ternary (`TQ*`) and the MoE 4-bit float format (`MXFP4`).
    Exotic,
    /// A file type this table does not know. Reported honestly rather than guessed at.
    Unknown,
}

/// How much of the model is left, in one word.
///
/// Deliberately coarse. The honest resolution of "is this quantization good?" is three buckets
/// and a caveat, not a perplexity delta the app cannot measure on the user's machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantTier {
    /// 8 bpw and up — indistinguishable from the source weights in practice.
    Lossless,
    /// ~4.5 to 8 bpw — the band where quality loss is hard to notice. `Q4_K_M` lives here.
    Recommended,
    /// ~3 to 4.5 bpw — visible degradation on hard prompts; the right trade when memory is the
    /// binding constraint.
    Compact,
    /// Below ~3 bpw — runs where nothing else fits, and it shows.
    Aggressive,
    Unknown,
}

/// One quantization scheme.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quantization {
    /// Canonical tag, uppercase: `Q4_K_M`, `IQ4_XS`, `BF16`.
    pub label: String,
    /// Effective bits per weight over a whole model, or `None` when unknown.
    pub bits_per_weight: Option<f32>,
    pub family: QuantFamily,
    pub tier: QuantTier,
}

impl Quantization {
    fn known(label: &str, bpw: f32, family: QuantFamily) -> Self {
        Quantization { label: label.to_string(), bits_per_weight: Some(bpw), family, tier: tier_for(bpw) }
    }

    /// An unrecognised tag. Kept rather than dropped: showing `Q4_0_4_8` with no numbers beats
    /// showing nothing, and beats claiming it is something it is not.
    pub fn unknown(label: impl Into<String>) -> Self {
        Quantization { label: label.into(), bits_per_weight: None, family: QuantFamily::Unknown, tier: QuantTier::Unknown }
    }

    /// From `general.file_type` — llama.cpp's `LLAMA_FTYPE` enum.
    ///
    /// The gaps are real: 4-6 were `Q4_1_SOME_F16`, `Q4_2` and `Q4_3`, and 33-35 were the
    /// `Q4_0_4_4` repacking variants. All were removed upstream, so a file claiming one is
    /// reported as unknown rather than mapped to whatever now sits at that number.
    pub fn from_file_type(ftype: u32) -> Option<Self> {
        Some(match ftype {
            0 => Self::known("F32", 32.0, QuantFamily::Float),
            1 => Self::known("F16", 16.0, QuantFamily::Float),
            2 => Self::known("Q4_0", 4.5, QuantFamily::Legacy),
            3 => Self::known("Q4_1", 5.0, QuantFamily::Legacy),
            7 => Self::known("Q8_0", 8.5, QuantFamily::Legacy),
            8 => Self::known("Q5_0", 5.5, QuantFamily::Legacy),
            9 => Self::known("Q5_1", 6.0, QuantFamily::Legacy),
            10 => Self::known("Q2_K", 3.35, QuantFamily::KQuant),
            11 => Self::known("Q3_K_S", 3.44, QuantFamily::KQuant),
            12 => Self::known("Q3_K_M", 3.91, QuantFamily::KQuant),
            13 => Self::known("Q3_K_L", 4.27, QuantFamily::KQuant),
            14 => Self::known("Q4_K_S", 4.58, QuantFamily::KQuant),
            15 => Self::known("Q4_K_M", 4.85, QuantFamily::KQuant),
            16 => Self::known("Q5_K_S", 5.52, QuantFamily::KQuant),
            17 => Self::known("Q5_K_M", 5.68, QuantFamily::KQuant),
            18 => Self::known("Q6_K", 6.56, QuantFamily::KQuant),
            19 => Self::known("IQ2_XXS", 2.06, QuantFamily::IQuant),
            20 => Self::known("IQ2_XS", 2.31, QuantFamily::IQuant),
            21 => Self::known("Q2_K_S", 2.96, QuantFamily::KQuant),
            22 => Self::known("IQ3_XS", 3.3, QuantFamily::IQuant),
            23 => Self::known("IQ3_XXS", 3.06, QuantFamily::IQuant),
            24 => Self::known("IQ1_S", 1.56, QuantFamily::IQuant),
            25 => Self::known("IQ4_NL", 4.5, QuantFamily::IQuant),
            26 => Self::known("IQ3_S", 3.44, QuantFamily::IQuant),
            27 => Self::known("IQ3_M", 3.66, QuantFamily::IQuant),
            28 => Self::known("IQ2_S", 2.5, QuantFamily::IQuant),
            29 => Self::known("IQ2_M", 2.7, QuantFamily::IQuant),
            30 => Self::known("IQ4_XS", 4.25, QuantFamily::IQuant),
            31 => Self::known("IQ1_M", 1.75, QuantFamily::IQuant),
            32 => Self::known("BF16", 16.0, QuantFamily::Float),
            36 => Self::known("TQ1_0", 1.69, QuantFamily::Exotic),
            37 => Self::known("TQ2_0", 2.06, QuantFamily::Exotic),
            38 => Self::known("MXFP4", 4.25, QuantFamily::Exotic),
            _ => return None,
        })
    }

    /// Every tag this table knows, longest first.
    ///
    /// Length order is load-bearing for filename matching: `Q4_K_M` contains `Q4_K`, which
    /// contains `Q4`, and a shortest-first scan would call every K-quant a legacy `Q4_0`.
    fn all_tags() -> Vec<Self> {
        let mut tags: Vec<Self> = (0..=38).filter_map(Self::from_file_type).collect();
        tags.push(Self::known("Q4_K", 4.7, QuantFamily::KQuant));
        tags.push(Self::known("Q3_K", 3.91, QuantFamily::KQuant));
        tags.push(Self::known("Q5_K", 5.6, QuantFamily::KQuant));
        tags.sort_by(|a, b| b.label.len().cmp(&a.label.len()).then_with(|| a.label.cmp(&b.label)));
        tags
    }

    /// From a filename such as `Qwen3-4B-Instruct-Q4_K_M.gguf`.
    ///
    /// Only a fallback: a filename is a claim by whoever renamed the file last. Prefer
    /// [`Self::from_file_type`] whenever the header carries one.
    pub fn from_filename(name: &str) -> Option<Self> {
        let upper = name.to_ascii_uppercase();
        Self::all_tags().into_iter().find(|q| contains_token(&upper, &q.label))
    }

    /// The header first, the filename second, and `None` only when neither says anything.
    pub fn detect(file_type: Option<u32>, filename: &str) -> Option<Self> {
        file_type.and_then(Self::from_file_type).or_else(|| Self::from_filename(filename))
    }

    /// Bytes the weights alone occupy at this rate, for `parameters` weights.
    ///
    /// Used to sanity-check a file size against its declared parameter count, and to estimate a
    /// download before it exists on disk.
    pub fn weight_bytes(&self, parameters: u64) -> Option<u64> {
        let bpw = self.bits_per_weight?;
        Some(((parameters as f64) * (bpw as f64) / 8.0) as u64)
    }
}

fn tier_for(bpw: f32) -> QuantTier {
    if bpw >= 8.0 {
        QuantTier::Lossless
    } else if bpw >= 4.5 {
        QuantTier::Recommended
    } else if bpw >= 3.0 {
        QuantTier::Compact
    } else {
        QuantTier::Aggressive
    }
}

/// `haystack` contains `tag` bounded by non-alphanumeric characters (or the string's ends).
///
/// Prevents `Q8_0` matching inside `IQ8_0X` and, more importantly, stops a model literally named
/// `MythoMax-F16bit` from matching a tag mid-word.
fn contains_token(haystack: &str, tag: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(rel) = haystack[from..].find(tag) {
        let start = from + rel;
        let end = start + tag.len();
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let after_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_file_type_wins() {
        // The file claims IQ1_S in its name and Q4_K_M in its header. The header is the one the
        // converter wrote.
        let q = Quantization::detect(Some(15), "someone-renamed-this-IQ1_S.gguf").expect("detected");
        assert_eq!(q.label, "Q4_K_M");
        assert_eq!(q.tier, QuantTier::Recommended);
    }

    /// The bug this test exists for: a shortest-first scan calls `Q4_K_M` a `Q4_0` and reports
    /// 4.5 bpw for a 4.85 bpw file — small enough to look plausible, wrong on every model.
    #[test]
    fn filename_matching_prefers_the_longest_tag() {
        assert_eq!(Quantization::from_filename("Llama-3.2-3B-Q4_K_M.gguf").unwrap().label, "Q4_K_M");
        assert_eq!(Quantization::from_filename("Llama-3.2-3B-Q4_K_S.gguf").unwrap().label, "Q4_K_S");
        assert_eq!(Quantization::from_filename("Llama-3.2-3B-IQ4_XS.gguf").unwrap().label, "IQ4_XS");
        assert_eq!(Quantization::from_filename("Llama-3.2-3B-Q4_0.gguf").unwrap().label, "Q4_0");
        assert_eq!(Quantization::from_filename("gpt-oss-20b-MXFP4.gguf").unwrap().label, "MXFP4");
    }

    #[test]
    fn a_tag_inside_a_word_is_not_a_match() {
        assert!(Quantization::from_filename("model-fp16bit-merge.gguf").is_none());
    }

    #[test]
    fn removed_file_types_are_unknown_rather_than_misread() {
        // 4, 5, 6 and 33-35 were removed upstream. Mapping them to a neighbour would report a
        // bits-per-weight the file does not have.
        for ftype in [4, 5, 6, 33, 34, 35] {
            assert!(Quantization::from_file_type(ftype).is_none(), "ftype {ftype} must not map");
        }
    }

    #[test]
    fn weight_bytes_track_the_effective_rate() {
        let q = Quantization::from_file_type(15).unwrap();
        // 8B parameters at 4.85 bpw ≈ 4.85 GB of weights.
        let bytes = q.weight_bytes(8_000_000_000).unwrap();
        assert!((4_800_000_000..5_000_000_000).contains(&bytes), "got {bytes}");
    }
}
