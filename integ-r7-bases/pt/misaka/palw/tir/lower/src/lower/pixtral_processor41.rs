//! Explicit integer counterpart of the publisher's aspect-preserving Pixtral
//! processor. The specialization fixes the original raster dimensions and all
//! resize coefficients in canonical Program39 constants. Float/OpenCV agreement
//! is an independent source-fidelity gate, never asserted by this constructor.
use crate::{LowerError, Result};
use misaka_palw_tir as tir;
use serde::{Deserialize, Serialize};
use tir::{
    DType, Ref, TensorType,
    builder::ProgramBuilder,
    program_v2::{InputDecl, InputSource, OutputDecl},
    program_v39::*,
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessorProfileV41 {
    pub version: u16,
    pub original_height: u32,
    pub original_width: u32,
    pub maximum_image_size: u32,
    pub patch: u32,
    pub merge: u32,
    pub image_token: u32,
    pub break_token: u32,
    pub end_token: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessorGeometryV41 {
    pub original: [u32; 2],
    pub resized: [u32; 2],
    pub token_grid: [u32; 2],
    pub pixel_shift: u8,
    pub image_tokens: Vec<u32>,
}
fn bad(s: &str) -> LowerError {
    LowerError::bad(s)
}
fn round_even(n: u64, d: u64) -> u64 {
    let (q, r) = (n / d, n % d);
    q + u64::from(r > d - r || (r == d - r && q % 2 != 0))
}
pub fn geometry_v41(p: &ProcessorProfileV41) -> Result<ProcessorGeometryV41> {
    if p.version != 41
        || p.original_height == 0
        || p.original_width == 0
        || p.maximum_image_size == 0
        || p.maximum_image_size > 1540
        || p.patch != 14
        || p.merge != 2
        || p.image_token == p.break_token
        || p.image_token == p.end_token
        || p.break_token == p.end_token
    {
        return Err(bad("PIXTRAL41_ORIGINAL_PROCESSOR_PROFILE"));
    }
    let cells = (p.original_height as u64)
        .checked_mul(p.original_width as u64)
        .and_then(|n| n.checked_mul(3))
        .ok_or_else(|| bad("PIXTRAL41_ORIGINAL_EXTENT"))?;
    if cells.checked_mul(16).is_none_or(|n| n > 2 << 30) || cells.checked_mul(2).is_none_or(|n| n > 256 << 20) {
        return Err(bad("PIXTRAL41_ORIGINAL_RASTER_PROFILE_CAP"));
    }
    let longest = p.original_height.max(p.original_width) as u64;
    let mut shape = [p.original_height as u64, p.original_width as u64];
    if longest > p.maximum_image_size as u64 {
        for n in &mut shape {
            *n = round_even(*n * p.maximum_image_size as u64, longest);
        }
    }
    if shape.contains(&0) {
        return Err(bad("PIXTRAL41_PUBLISHER_ZERO_AXIS_REFUSAL"));
    }
    let unit = p.patch as u64 * p.merge as u64;
    let grid = [shape[0].div_ceil(unit) as u32, shape[1].div_ceil(unit) as u32];
    let resized = [grid[0] * unit as u32, grid[1] * unit as u32];
    let mut image_tokens = Vec::with_capacity((grid[0] * (grid[1] + 1)) as usize);
    for row in 0..grid[0] {
        image_tokens.extend(std::iter::repeat_n(p.image_token, grid[1] as usize));
        image_tokens.push(if row + 1 == grid[0] { p.end_token } else { p.break_token });
    }
    Ok(ProcessorGeometryV41 {
        original: [p.original_height, p.original_width],
        resized,
        token_grid: grid,
        pixel_shift: 6,
        image_tokens,
    })
}
/// Cubic convolution a=-3/4, half-pixel sampling, replicate boundary. Fixed
/// Q20 coefficients and two explicit Q6 rounding points are this new profile's
/// meaning; the publisher uses float32 OpenCV and needs differential evidence.
fn cubic(x: f64) -> f64 {
    let x = x.abs();
    if x <= 1. {
        ((1.25 * x - 2.25) * x) * x + 1.
    } else if x < 2. {
        ((-0.75 * x + 3.75) * x - 6.) * x + 3.
    } else {
        0.
    }
}
fn taps(source: u32, target: u32) -> ([Vec<i128>; 4], [Vec<i128>; 4]) {
    let mut indices: [Vec<i128>; 4] = std::array::from_fn(|_| Vec::with_capacity(target as usize));
    let mut weights = indices.clone();
    for out in 0..target {
        let x = (out as f64 + 0.5) * source as f64 / target as f64 - 0.5;
        let base = x.floor() as i64;
        let mut total = 0i128;
        for tap in 0..4 {
            let at = base + tap as i64 - 1;
            indices[tap].push(at.clamp(0, source as i64 - 1) as i128);
            let w = if tap == 3 { (1i128 << 20) - total } else { (cubic(x - at as f64) * (1u64 << 20) as f64).round() as i128 };
            weights[tap].push(w);
            total += w;
        }
    }
    (indices, weights)
}
pub fn lower_processor_v41(p: &ProcessorProfileV41, profile: SourceProfileV39) -> Result<(TirProgramV39, ProcessorGeometryV41)> {
    profile.check().map_err(LowerError::bad)?;
    let geometry = geometry_v41(p)?;
    let [h, w] = geometry.original;
    let [oh, ow] = geometry.resized;
    let mut pb = ProgramBuilder::new(1, 64);
    let input = pb.param("processor41.original_rgb", DType::I16, &[h, w, 3], false);
    let mut tables = Vec::new();
    for (source, target) in [(w, ow), (h, oh)] {
        let (indices, weights) = taps(source, target);
        let mut axis = Vec::new();
        for tap in 0..4 {
            axis.push((pb.konst(DType::Idx, &[target], &indices[tap]), pb.konst(DType::I32, &[target], &weights[tap])));
        }
        tables.push(axis);
    }
    let pre = {
        let mut b = pb.block_for_program_v39("processor41.bicubic", vec![], &profile).map_err(LowerError::bad)?;
        let mut horizontal = None;
        for (index, weight) in &tables[0] {
            let x = b.gather(input, *index, 1, 0);
            let q = b.reshape_fixed(*weight, &[1, ow, 1]);
            let x = b.mul(x, q, DType::I64);
            horizontal = Some(match horizontal {
                Some(sum) => b.add(sum, x, DType::I64),
                None => x,
            });
        }
        let q = b.c(DType::I64, 1 << 14);
        let x = b.div(horizontal.ok_or_else(|| bad("PIXTRAL41_HORIZONTAL_TAPS"))?, q, tir::Rounding::HalfAwayFromZero, DType::I32);
        let x = b.commit(x);
        let mut vertical = None;
        for (index, weight) in &tables[1] {
            let y = b.gather(x, *index, 0, 0);
            let q = b.reshape_fixed(*weight, &[oh, 1, 1]);
            let y = b.mul(y, q, DType::I64);
            vertical = Some(match vertical {
                Some(sum) => b.add(sum, y, DType::I64),
                None => y,
            });
        }
        let q = b.c(DType::I64, 1 << 20);
        let y = b.div(vertical.ok_or_else(|| bad("PIXTRAL41_VERTICAL_TAPS"))?, q, tir::Rounding::HalfAwayFromZero, DType::I32);
        let y = b.clamp(y, i16::MIN as i64, i16::MAX as i64, DType::I16);
        b.commit(y);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb
            .block_for_program_v39("processor41.output", vec![TensorType::fixed(DType::I16, &[oh, ow, 3])], &profile)
            .map_err(LowerError::bad)?;
        let y = b.clamp(Ref::CarryIn(0), i16::MIN as i64, i16::MAX as i64, DType::I16);
        b.commit(y);
        b.finish(&[])
    };
    let node = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    let recipe = pb.finish(pre, vec![], post, node);
    let program = TirProgramV39 {
        version: 39,
        profile,
        prim_set_id: PRIM_SET_ID_V39,
        token_bound: 1,
        history_bound: 64,
        inputs: vec![InputDecl {
            name: "processor41.original_rgb".into(),
            dtype: DType::I16,
            shape: vec![h, w, 3],
            source: InputSource::External { lo: 0, hi: 255 },
        }],
        params: vec![],
        consts: recipe.consts,
        states: vec![],
        blocks: recipe
            .blocks
            .iter()
            .map(|b| BlockV39 {
                name: b.name.clone(),
                carry_in: b.carry_in.iter().map(TensorTypeV39::from_frozen).collect(),
                nodes: b
                    .nodes
                    .iter()
                    .map(|n| {
                        let mut n = NodeV39::from_frozen(n);
                        for r in &mut n.inputs {
                            if *r == input {
                                *r = Ref::Input(tir::program_v2::FIRST_INPUT_REF_V2);
                            }
                        }
                        n
                    })
                    .collect(),
                carry_out: b.carry_out.clone(),
            })
            .collect(),
        schedule: recipe.schedule,
        output: OutputDecl::Final { node },
    };
    tir::validate_v39::validate_v39(&program).map_err(LowerError::bad)?;
    Ok((program, geometry))
}
/// Canonical RGBA8 ingress over white, before the protocol's RGB raster input.
/// Its provenance must still bind the original encoded/raster input; this helper
/// alone grants neither source fidelity nor authenticated native image authority.
pub fn rgba_over_white_v41(raw: &[u8]) -> Result<Vec<u8>> {
    if raw.len() % 4 != 0 || raw.len() > 256 << 20 {
        return Err(bad("PIXTRAL41_RGBA_EXTENT"));
    }
    let mut out = Vec::with_capacity(raw.len() / 4 * 3);
    for pixel in raw.chunks_exact(4) {
        let a = pixel[3] as u32;
        for c in &pixel[..3] {
            out.push(((*c as u32 * a + 255 * (255 - a) + 127) / 255) as u8);
        }
    }
    Ok(out)
}
