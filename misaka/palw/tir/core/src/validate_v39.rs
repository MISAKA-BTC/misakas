//! Program39 admission. Legacy primitive type relations are reused locally, not
//! a legacy program/parser/execution identity. SelectedH is checked on every
//! symbolic interval of the original history, so a fixed-k surrogate cannot
//! authorize the short-history cases.
use crate::{DType, Dim, Prim, TensorType, program::*, program_v2::*, program_v39::*};
use borsh::BorshDeserialize;
use std::{
    collections::BTreeSet,
    io::{Cursor, Read},
};
fn get<T: BorshDeserialize>(r: &mut Cursor<&[u8]>) -> Result<T, String> {
    T::deserialize_reader(r).map_err(|_| "program39 truncated or unknown canonical field".into())
}
fn count(r: &mut Cursor<&[u8]>, max: u32) -> Result<u32, String> {
    let n: u32 = get(r)?;
    if n > max {
        return Err("program39 preallocation count bound".into());
    }
    Ok(n)
}
fn skip(r: &mut Cursor<&[u8]>, n: u64) -> Result<(), String> {
    let end = r.position().checked_add(n).filter(|n| *n <= r.get_ref().len() as u64).ok_or("program39 truncated extent")?;
    r.set_position(end);
    Ok(())
}
fn name(r: &mut Cursor<&[u8]>) -> Result<(), String> {
    let n = count(r, 128)?;
    skip(r, n as u64)
}
fn shape(r: &mut Cursor<&[u8]>) -> Result<(), String> {
    let n = count(r, 4)?;
    skip(r, n as u64 * 4)
}
fn dtype(r: &mut Cursor<&[u8]>) -> Result<(), String> {
    let _: DType = get(r)?;
    Ok(())
}
fn ty(r: &mut Cursor<&[u8]>) -> Result<(), String> {
    dtype(r)?;
    for _ in 0..count(r, 4)? {
        let _: DimV39 = get(r)?;
    }
    Ok(())
}
fn frozen_prim(r: &mut Cursor<&[u8]>) -> Result<(), String> {
    let tag: u8 = get(r)?;
    match tag {
        0 | 4 | 7 | 8 | 9 | 10 | 11 | 16 | 17 | 18 | 19 | 21 => {}
        1 => {
            let n = count(r, 4)?;
            skip(r, n as u64)?;
        }
        2 => skip(r, 5)?,
        3 | 12 | 13 => skip(r, 1)?,
        5 => skip(r, 17)?,
        6 => skip(r, 2)?,
        14 => {
            let _: crate::Rounding = get(r)?;
        }
        15 => skip(r, 16)?,
        20 => {
            let _: crate::Cmp = get(r)?;
        }
        22 => skip(r, 5)?,
        23 | 24 => skip(r, 2)?,
        _ => return Err("program39 unknown frozen primitive tag".into()),
    }
    Ok(())
}
pub fn preflight_program_v39(raw: &[u8]) -> Result<(), String> {
    let mut r = Cursor::new(raw);
    if get::<u16>(&mut r)? != 39 {
        return Err("program39 version".into());
    }
    let profile: SourceProfileV39 = get(&mut r)?;
    profile.check()?;
    if raw.len() > profile.max_program_bytes as usize {
        return Err("program39 original profile byte bound".into());
    }
    skip(&mut r, 72)?; // primitive descriptor + token/history bounds
    for _ in 0..count(&mut r, 16)? {
        name(&mut r)?;
        dtype(&mut r)?;
        shape(&mut r)?;
        let _: InputSource = get(&mut r)?;
    }
    for _ in 0..count(&mut r, profile.max_parameters as u32)? {
        name(&mut r)?;
        dtype(&mut r)?;
        shape(&mut r)?;
        let _: bool = get(&mut r)?;
    }
    for _ in 0..count(&mut r, 4096)? {
        dtype(&mut r)?;
        shape(&mut r)?;
        let n = count(&mut r, 65536)?;
        skip(&mut r, n as u64)?;
    }
    for _ in 0..count(&mut r, profile.max_states as u32)? {
        name(&mut r)?;
        let _: StateKind = get(&mut r)?;
        dtype(&mut r)?;
        shape(&mut r)?;
        let _: bool = get(&mut r)?;
    }
    for _ in 0..count(&mut r, profile.max_blocks as u32)? {
        name(&mut r)?;
        for _ in 0..count(&mut r, 8)? {
            ty(&mut r)?;
        }
        for _ in 0..count(&mut r, profile.max_nodes_per_block as u32)? {
            match get::<u8>(&mut r)? {
                0 => frozen_prim(&mut r)?,
                1 => skip(&mut r, 5)?,
                _ => return Err("program39 unknown primitive namespace".into()),
            }
            for _ in 0..count(&mut r, 8)? {
                let _: Ref = get(&mut r)?;
            }
            ty(&mut r)?;
            let _: bool = get(&mut r)?;
        }
        let n = count(&mut r, 8)?;
        skip(&mut r, 2 * n as u64)?;
    }
    skip(&mut r, 1)?;
    let n = count(&mut r, 1024)?;
    skip(&mut r, n as u64 + 1)?;
    let _: OutputDecl = get(&mut r)?;
    if r.position() != raw.len() as u64 {
        return Err("program39 trailing bytes".into());
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub struct ProgramInfoV39 {
    pub windows: Vec<Option<u32>>,
    pub state_writers: Vec<(usize, usize, u16)>,
}
fn static_elements(shape: &[u32], cap: u64) -> Result<u64, String> {
    if shape.len() > 4 || shape.iter().any(|n| *n == 0 || *n > 1 << 24) {
        return Err("program39 static shape".into());
    }
    shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64).filter(|n| *n <= cap).ok_or("program39 static element extent".into()))
}
fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 128 && n.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'/' | b'-'))
}
fn check_type(t: &TensorTypeV39, w: Option<u32>, cap: u64) -> Result<(), String> {
    if t.shape.len() > 4
        || t.shape.iter().any(|d| match d {
            DimV39::Fixed(n) => *n == 0 || *n > 1 << 24,
            DimV39::H => w.is_none(),
            DimV39::SelectedH(c) => w.is_none() || *c == 0 || *c > w.unwrap_or(0),
        })
        || t.elements_at(w.unwrap_or(1))? > cap
    {
        return Err("program39 dynamic tensor extent".into());
    }
    Ok(())
}
fn symbolic(t: &TensorTypeV39, from: u32) -> TensorType {
    TensorType {
        dtype: t.dtype,
        shape: t
            .shape
            .iter()
            .map(|d| match d {
                DimV39::Fixed(n) => Dim::Fixed(*n),
                DimV39::H => Dim::H,
                DimV39::SelectedH(c) if from <= *c => Dim::H,
                DimV39::SelectedH(c) => Dim::Fixed(*c),
            })
            .collect(),
    }
}
fn check_frozen(p: &Prim, ins: &[TensorTypeV39], out: &TensorTypeV39, states: &[StateDecl], window: u32) -> Result<(), String> {
    if let Prim::Gather { axis, batch_dims } = p {
        let (a, b) = (*axis as usize, *batch_dims as usize);
        if a >= ins[0].shape.len()
            || b > a
            || b > ins[1].shape.len()
            || ins[0].shape[..b] != ins[1].shape[..b]
            || ins[1].dtype == DType::I128
            || ins[0].dtype != out.dtype
        {
            return Err("program39 Gather axis/index/batch dtype".into());
        }
        let mut expected = ins[0].shape[..a].to_vec();
        expected.extend_from_slice(&ins[1].shape[b..]);
        expected.extend_from_slice(&ins[0].shape[a + 1..]);
        if expected != out.shape {
            return Err("program39 Gather exact dynamic output".into());
        }
        return Ok(());
    }
    let mut starts = BTreeSet::from([1]);
    for t in ins.iter().chain(std::iter::once(out)) {
        for d in &t.shape {
            if let DimV39::SelectedH(c) = d {
                if *c < window {
                    starts.insert(c + 1);
                }
            }
        }
    }
    for from in starts {
        crate::validate::check_node_type(p, &ins.iter().map(|t| symbolic(t, from)).collect::<Vec<_>>(), &symbolic(out, from), states)?;
    }
    Ok(())
}
pub fn validate_v39(p: &TirProgramV39) -> Result<ProgramInfoV39, String> {
    p.profile.check()?;
    if p.version != 39
        || p.prim_set_id != PRIM_SET_ID_V39
        || p.token_bound == 0
        || p.history_bound == 0
        || p.history_bound > 1 << 21
        || p.encode().len() > p.profile.max_program_bytes as usize
        || p.blocks.len() < 2
        || p.blocks.len() > p.profile.max_blocks as usize
        || p.params.len() > p.profile.max_parameters as usize
        || p.inputs.len() > 16
        || p.states.len() > p.profile.max_states as usize
        || p.consts.len() > 4096
        || p.schedule.layers.len() > 1024
    {
        return Err("program39 original header/declaration profile".into());
    }
    let mut names = BTreeSet::new();
    for d in &p.params {
        if !valid_name(&d.name) || !names.insert(d.name.as_str()) || matches!(d.dtype, DType::I128) {
            return Err("program39 parameter namespace/type".into());
        }
        static_elements(&d.shape, p.profile.max_parameter_elements)?;
    }
    let mut raw = 0u64;
    let mut expanded = 0u64;
    for i in &p.inputs {
        if !valid_name(&i.name) || !names.insert(i.name.as_str()) || matches!(i.dtype, DType::I128) {
            return Err("program39 input namespace/type".into());
        }
        let n = static_elements(&i.shape, p.profile.max_value_elements)?;
        raw =
            raw.checked_add(n.checked_mul(i.dtype.width() as u64).ok_or("program39 input raw product")?).ok_or("program39 raw sum")?;
        expanded = expanded
            .checked_add(n.checked_mul(16).ok_or("program39 input expanded product")?)
            .ok_or("program39 input expanded sum")?;
        match i.source {
            InputSource::External { lo, hi } => {
                if lo > hi || !i.dtype.contains(lo as i128) || !i.dtype.contains(hi as i128) {
                    return Err("program39 external input range".into());
                }
            }
            InputSource::Random { domain, dist, per_step } => {
                let (bits, rule) = random_input_domain_v2(domain).ok_or("program39 unknown random domain")?;
                if matches!(rule, RandStepRule::Zero) && per_step || matches!(rule, RandStepRule::PerStep) && !per_step {
                    return Err("program39 random step differs".into());
                }
                if !matches!(dist, RandomDist::Normal) && dist != (RandomDist::Uniform { bits }) {
                    return Err("program39 random word differs".into());
                }
                let (lo, hi) = i.interval();
                if !i.dtype.contains(lo) || !i.dtype.contains(hi) {
                    return Err("program39 random dtype interval".into());
                }
            }
        }
    }
    if raw > p.profile.max_input_raw_bytes || expanded > p.profile.max_input_expanded_bytes {
        return Err("program39 original input byte profile".into());
    }
    let mut const_bytes = 0usize;
    let mut constants = BTreeSet::new();
    for c in &p.consts {
        let n = static_elements(&c.shape, p.profile.max_value_elements)?;
        if n.checked_mul(c.dtype.width() as u64) != Some(c.data.len() as u64)
            || !constants.insert((c.dtype, c.shape.clone(), c.data.clone()))
        {
            return Err("program39 inline constant extent/duplicate".into());
        }
        const_bytes = const_bytes.checked_add(c.data.len()).ok_or("program39 constants overflow")?;
    }
    if const_bytes > 65536 {
        return Err("program39 inline constant cap".into());
    }
    let mut snames = BTreeSet::new();
    for s in &p.states {
        if !valid_name(&s.name) || !snames.insert(s.name.as_str()) || !matches!(s.dtype, DType::I8 | DType::I16 | DType::I32) {
            return Err("program39 state namespace/type".into());
        }
        static_elements(&s.shape, p.profile.max_value_elements)?;
        match s.kind {
            StateKind::Fixed { lo, hi } => {
                if lo > 0 || hi < 0 || lo > hi || !s.dtype.contains(lo as i128) || !s.dtype.contains(hi as i128) {
                    return Err("program39 fixed state range".into());
                }
            }
            StateKind::Hist { window } => {
                if window == 0 || window > p.history_bound || s.shape.len() > 3 {
                    return Err("program39 history window/rank".into());
                }
            }
        }
    }
    let (pre, post) = (p.schedule.pre as usize, p.schedule.post as usize);
    if pre >= p.blocks.len()
        || post >= p.blocks.len()
        || pre == post
        || p.schedule.layers.iter().any(|b| *b as usize >= p.blocks.len() || *b as usize == pre || *b as usize == post)
    {
        return Err("program39 static schedule".into());
    }
    let layers: BTreeSet<_> = p.schedule.layers.iter().map(|b| *b as usize).collect();
    if layers.len() + 2 != p.blocks.len() {
        return Err("program39 unused source block".into());
    }
    if !p.blocks[pre].carry_in.is_empty() || !p.blocks[post].carry_out.is_empty() {
        return Err("program39 pre/post carry".into());
    }
    let mut block_names = BTreeSet::new();
    for b in &p.blocks {
        if !valid_name(&b.name)
            || !block_names.insert(b.name.as_str())
            || b.nodes.is_empty()
            || b.nodes.len() > p.profile.max_nodes_per_block as usize
            || b.carry_in.len() > 8
            || b.carry_out.len() > 8
            || b.carry_out.iter().any(|n| *n as usize >= b.nodes.len())
        {
            return Err("program39 block profile/carry".into());
        }
    }
    let carry: Vec<_> = p.blocks[pre].carry_out.iter().map(|n| p.blocks[pre].nodes[*n as usize].out.clone()).collect();
    if carry.iter().any(|t| !t.dtype.committable() || t.shape.iter().any(|d| !matches!(d, DimV39::Fixed(_)))) {
        return Err("program39 carry must have fixed committed type".into());
    }
    for (bi, b) in p.blocks.iter().enumerate() {
        if bi != pre && b.carry_in != carry
            || layers.contains(&bi) && b.carry_out.iter().map(|n| b.nodes[*n as usize].out.clone()).collect::<Vec<_>>() != carry
        {
            return Err("program39 carry signature differs".into());
        }
    }
    let output = p.blocks[post].nodes.get(p.output.node() as usize).ok_or("program39 output node absent")?;
    if !output.commit || !output.out.dtype.committable() || output.out.shape.iter().any(|d| !matches!(d, DimV39::Fixed(_))) {
        return Err("program39 output must be fixed committed tensor".into());
    }
    let mut windows = Vec::new();
    let mut writers = Vec::new();
    let mut global_written = BTreeSet::new();
    let (mut used_params, mut used_consts, mut used_inputs, mut used_states) =
        (BTreeSet::new(), BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for (bi, b) in p.blocks.iter().enumerate() {
        let is_layer = layers.contains(&bi);
        let mut window = None;
        for n in &b.nodes {
            if let NodePrimV39::Frozen(Prim::HistAppend { state }) = &n.prim {
                let s = p.states.get(*state as usize).ok_or("program39 history state absent")?;
                let StateKind::Hist { window: w } = s.kind else {
                    return Err("program39 history kind".into());
                };
                if window.is_some_and(|x| x != w) {
                    return Err("program39 unequal history windows in block".into());
                }
                window = Some(w);
            }
        }
        windows.push(window);
        let mut written = BTreeSet::new();
        let mut used_nodes = BTreeSet::new();
        let mut roots: BTreeSet<usize> = b.carry_out.iter().map(|n| *n as usize).collect();
        if bi == post {
            roots.insert(p.output.node() as usize);
        }
        for (ni, n) in b.nodes.iter().enumerate() {
            check_type(&n.out, window, p.profile.max_value_elements)?;
            let arity = match &n.prim {
                NodePrimV39::Frozen(pr) => pr.arity(),
                NodePrimV39::HistorySelect { .. } => (1, 1),
            };
            if n.inputs.len() < arity.0 || n.inputs.len() > arity.1 || n.inputs.len() > 8 || n.commit && !n.out.dtype.committable() {
                return Err("program39 primitive arity/commit type".into());
            }
            let mut ins = Vec::new();
            for r in &n.inputs {
                let t = match *r {
                    Ref::Node(j) => {
                        if j as usize >= ni {
                            return Err("program39 non-backward node reference".into());
                        }
                        used_nodes.insert(j as usize);
                        b.nodes[j as usize].out.clone()
                    }
                    Ref::CarryIn(j) => b.carry_in.get(j as usize).cloned().ok_or("program39 carry reference")?,
                    Ref::Param(j) => {
                        used_params.insert(j as usize);
                        let d = p.params.get(j as usize).ok_or("program39 parameter reference")?;
                        if d.per_layer && !is_layer {
                            return Err("program39 per-layer parameter role".into());
                        }
                        TensorTypeV39::fixed(d.dtype, &d.shape)
                    }
                    Ref::Const(j) => {
                        used_consts.insert(j as usize);
                        let d = p.consts.get(j as usize).ok_or("program39 constant reference")?;
                        TensorTypeV39::fixed(d.dtype, &d.shape)
                    }
                    Ref::State(j) => {
                        used_states.insert(j as usize);
                        let d = p.states.get(j as usize).ok_or("program39 state reference")?;
                        if !matches!(d.kind, StateKind::Fixed { .. }) || d.per_layer && !is_layer {
                            return Err("program39 state read role".into());
                        }
                        TensorTypeV39::fixed(d.dtype, &d.shape)
                    }
                    Ref::Input(j) if j < 2 => TensorTypeV39::scalar(DType::Idx),
                    Ref::Input(j) => {
                        used_inputs.insert(j as usize - 2);
                        let d = p.inputs.get(j as usize - 2).ok_or("program39 input reference")?;
                        TensorTypeV39::fixed(d.dtype, &d.shape)
                    }
                };
                ins.push(t);
            }
            match &n.prim {
                NodePrimV39::HistorySelect { axis, cap } => {
                    let a = *axis as usize;
                    if *cap == 0
                        || window.is_none_or(|w| *cap > w)
                        || ins[0].shape.get(a) != Some(&DimV39::H)
                        || n.out.dtype != DType::Idx
                        || !n.commit
                    {
                        return Err("program39 HistorySelect must bind original full H and committed Idx".into());
                    }
                    let mut expected = ins[0].shape.clone();
                    expected[a] = DimV39::SelectedH(*cap);
                    if n.out.shape != expected {
                        return Err("program39 HistorySelect exact min(H,cap) output".into());
                    }
                }
                NodePrimV39::Frozen(pr) => {
                    check_frozen(pr, &ins, &n.out, &p.states, window.unwrap_or(1))?;
                    match pr {
                        Prim::TopK { .. } if !n.commit => return Err("program39 TopK must commit".into()),
                        Prim::StateWrite { state } | Prim::HistAppend { state } => {
                            used_states.insert(*state as usize);
                            let s = p.states.get(*state as usize).ok_or("program39 write state absent")?;
                            if !n.commit
                                || s.per_layer && !is_layer
                                || !written.insert(*state)
                                || bi == post && (!p.output.allows_post_writes() || matches!(pr, Prim::HistAppend { .. }))
                            {
                                return Err("program39 state write role/uniqueness".into());
                            }
                            if !s.per_layer && !global_written.insert(*state) {
                                return Err("program39 multiple global writes per position".into());
                            }
                            writers.push((bi, ni, *state));
                            roots.insert(ni);
                        }
                        _ => {}
                    }
                }
            }
            if n.commit {
                roots.insert(ni);
            }
        }
        // Full backward liveness. Dead nodes cannot be hidden from quota/courts.
        let mut live = roots;
        for ni in (0..b.nodes.len()).rev() {
            if live.contains(&ni) {
                for r in &b.nodes[ni].inputs {
                    if let Ref::Node(j) = r {
                        live.insert(*j as usize);
                    }
                }
            }
        }
        if live.len() != b.nodes.len() {
            return Err("program39 dead source node".into());
        }
    }
    if used_params.len() != p.params.len()
        || used_consts.len() != p.consts.len()
        || used_inputs.len() != p.inputs.len()
        || used_states.len() != p.states.len()
    {
        return Err("program39 unused original declaration".into());
    }
    let mut state_instances = BTreeSet::new();
    for (block, layer) in p.occurrences() {
        for (bi, _, state) in &writers {
            if *bi == block as usize {
                let slot = (*state, if p.states[*state as usize].per_layer { layer } else { None });
                if !state_instances.insert(slot) {
                    return Err("program39 multiple original state-instance writes".into());
                }
            }
        }
    }
    for (i, state) in p.states.iter().enumerate() {
        if state.per_layer {
            for layer in 0..p.schedule.layers.len() {
                if !state_instances.contains(&(i as u16, Some(layer as u16))) {
                    return Err("program39 unmaintained per-layer state instance".into());
                }
            }
        } else if !state_instances.contains(&(i as u16, None)) {
            return Err("program39 unmaintained global state instance".into());
        }
    }
    Ok(ProgramInfoV39 { windows, state_writers: writers })
}
