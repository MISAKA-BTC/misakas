//! RFC08 checker read path for the versioned complete kernel codec (TypedKernelV4).
//!
//! A drawn checker asks a provider for the actual typed material of a claim it was assigned to check. The request is an
//! authenticated provider-transport frame (purpose `KernelMaterialReadV4`, signed by the checker's own native bond key);
//! the provider answers a bounded page only after its OWN current native state shows (a) the reader is a drawn seat of
//! that claim's current lock and (b) this provider holds a currently enrolled generation whose intrinsic context replays.
//! The page is read-only: it creates no consensus object, no liability and no GC/ACK permission.
//!
//! Every unit in a page is re-authenticated by the checker against the claim's native commitments
//! (`PalwTypedKernelMaterialBundleV4::verify`) before any reconstruction; a provider signature only identifies who served
//! the bytes. Neither identifies arithmetic truth (PRINCIPLES §8): the verdict is the checker's deterministic recomputation.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64,
    palw_material_capacity_v1::PALW_MATERIAL_MAX_UNITS_V1,
    palw_native_resource_v1::{NativeDemandLeaseV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, reserve_native_requests_v1},
    palw_prospective_kernel_material_v4::PalwTypedKernelMaterialBundleV4,
    palw_state_v2::PalwBondKeyV2,
};
use std::io::Read;

/// The largest page a provider serves in one authenticated frame. Paging keeps every frame under the transport frame cap
/// independently of the claim's total material size.
pub const KERNEL_MATERIAL_READ_PAGE_BYTES_V4: u64 = 768 * 1024;
/// The largest unit count a page may name (bounded independently of byte size so an all-tiny page cannot inflate indexes).
pub const KERNEL_MATERIAL_READ_PAGE_UNITS_V4: u32 = 4096;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct KernelMaterialReadRequestV4 {
    pub version: u16,
    /// The claim (material root) whose typed material is requested.
    pub root: Hash64,
    /// The checker's preferred generation (the native consent it observed); the provider may answer from another
    /// currently enrolled generation of the same root and says which in the page.
    pub preferred_consent: Hash64,
    /// The drawn checker; equals the transport sender.
    pub reader: PalwBondKeyV2,
    pub first_unit: u32,
    pub max_units: u32,
    pub max_bytes: u64,
}
impl KernelMaterialReadRequestV4 {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 4
            || self.max_units == 0
            || self.max_units > KERNEL_MATERIAL_READ_PAGE_UNITS_V4
            || self.max_bytes == 0
            || self.max_bytes > KERNEL_MATERIAL_READ_PAGE_BYTES_V4
            || self.first_unit as usize >= PALW_MATERIAL_MAX_UNITS_V1
        {
            return Err("kernel material read request version/page bounds invalid".into());
        }
        Ok(())
    }
}
impl NativeWireScanV1 for KernelMaterialReadRequestV4 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        <(u16, Hash64, Hash64, PalwBondKeyV2)>::scan_native_wire_v1(r)?.merge(<(u32, u32, u64)>::scan_native_wire_v1(r)?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum KernelMaterialReadResponseV4 {
    Page(KernelMaterialReadPageV4),
    Refused(String),
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct KernelMaterialReadPageV4 {
    pub root: Hash64,
    /// The generation actually served (a currently enrolled consent of `root`).
    pub consent: Hash64,
    pub total_units: u32,
    pub first_unit: u32,
    pub units: Vec<(u8, u32, Vec<u8>)>,
}
impl NativeWireScanV1 for KernelMaterialReadPageV4 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        <(Hash64, Hash64, u32, u32)>::scan_native_wire_v1(r)?.merge(Vec::<(u8, u32, Vec<u8>)>::scan_native_wire_v1(r)?)
    }
}
impl NativeWireScanV1 for KernelMaterialReadResponseV4 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        match r.byte()? {
            0 => KernelMaterialReadPageV4::scan_native_wire_v1(r),
            1 => String::scan_native_wire_v1(r),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "kernel material read response unknown")),
        }
    }
}

/// Serve one page of a decoded typed bundle. The caller has already established current native authority for this reader
/// and generation; this only bounds the page.
pub fn kernel_material_page_v4(
    bundle: &PalwTypedKernelMaterialBundleV4,
    consent: Hash64,
    request: &KernelMaterialReadRequestV4,
) -> Result<KernelMaterialReadPageV4, String> {
    request.validate()?;
    if bundle.claim != request.root {
        return Err("kernel material read names another claim".into());
    }
    let total = u32::try_from(bundle.units.len()).map_err(|_| "kernel material unit count exceeds u32")?;
    if request.first_unit > total {
        return Err("kernel material read starts past the inventory".into());
    }
    let mut units = Vec::new();
    let mut bytes = 0u64;
    for unit in bundle.units.iter().skip(request.first_unit as usize).take(request.max_units as usize) {
        let n = (unit.2.len() as u64).checked_add(4 + 1 + 4 + 4).ok_or("kernel page size overflow")?;
        if !units.is_empty() && bytes.checked_add(n).is_none_or(|next| next > request.max_bytes) {
            break;
        }
        if n > request.max_bytes {
            return Err("one kernel material unit exceeds the requested page bytes; ask with a larger page".into());
        }
        bytes += n;
        units.push(unit.clone());
    }
    Ok(KernelMaterialReadPageV4 { root: request.root, consent, total_units: total, first_unit: request.first_unit, units })
}

/// Checker-side bounded assembly of authenticated pages into one typed bundle. Every page must name the same root and
/// generation, arrive in order without gaps, and stay within the caller's explicit byte/unit caps (reserved before the
/// growth they admit). The assembled bundle is still unauthenticated until `PalwTypedKernelMaterialBundleV4::verify`.
pub struct KernelMaterialPageAssemblerV4 {
    root: Hash64,
    consent: Option<Hash64>,
    total: Option<u32>,
    units: Vec<(u8, u32, Vec<u8>)>,
    bytes: u64,
    max_bytes: u64,
    _leases: Vec<Option<NativeDemandLeaseV1>>,
}
impl KernelMaterialPageAssemblerV4 {
    pub fn new(root: Hash64, max_bytes: u64) -> Result<Self, String> {
        if max_bytes == 0 {
            return Err("kernel material assembly needs an explicit byte cap".into());
        }
        Ok(Self { root, consent: None, total: None, units: Vec::new(), bytes: 0, max_bytes, _leases: Vec::new() })
    }
    pub fn next_unit(&self) -> u32 {
        self.units.len() as u32
    }
    pub fn complete(&self) -> bool {
        self.total.is_some_and(|t| t as usize == self.units.len())
    }
    pub fn consent(&self) -> Option<Hash64> {
        self.consent
    }
    pub fn push(&mut self, page: KernelMaterialReadPageV4) -> Result<(), String> {
        if page.root != self.root
            || self.consent.is_some_and(|c| c != page.consent)
            || self.total.is_some_and(|t| t != page.total_units)
            || page.first_unit as usize != self.units.len()
            || page.units.is_empty() && page.first_unit != page.total_units
            || page.total_units as usize > PALW_MATERIAL_MAX_UNITS_V1
            || (page.first_unit as usize).checked_add(page.units.len()).is_none_or(|end| end > page.total_units as usize)
        {
            return Err("kernel material page substitutes root/generation/order/inventory".into());
        }
        let mut growth = 0u64;
        for unit in &page.units {
            growth = growth
                .checked_add(unit.2.len() as u64)
                .and_then(|n| n.checked_add(std::mem::size_of::<(u8, u32, Vec<u8>)>() as u64))
                .ok_or("kernel material assembly overflow")?;
        }
        let next = self.bytes.checked_add(growth).ok_or("kernel material assembly overflow")?;
        if next > self.max_bytes {
            return Err("kernel material assembly exceeds the checker's explicit byte cap".into());
        }
        self._leases.push(reserve_native_requests_v1(growth).map_err(|e| e.to_string())?);
        self.bytes = next;
        self.consent = Some(page.consent);
        self.total = Some(page.total_units);
        self.units.extend(page.units);
        Ok(())
    }
    /// Hands the assembled (still unauthenticated) bundle and its retained reservations to the caller.
    pub fn finish(self) -> Result<(PalwTypedKernelMaterialBundleV4, Hash64, Vec<Option<NativeDemandLeaseV1>>), String> {
        if !self.complete() {
            return Err("kernel material assembly is incomplete".into());
        }
        let consent = self.consent.ok_or("kernel material assembly has no generation")?;
        Ok((PalwTypedKernelMaterialBundleV4 { claim: self.root, units: self.units }, consent, self._leases))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(n), 0))
    }
    fn request(first: u32, units: u32, bytes: u64) -> KernelMaterialReadRequestV4 {
        KernelMaterialReadRequestV4 {
            version: 4,
            root: h(1),
            preferred_consent: h(2),
            reader: bond(3),
            first_unit: first,
            max_units: units,
            max_bytes: bytes,
        }
    }
    #[test]
    fn pages_assemble_in_order_and_refuse_gaps_or_substitution() {
        let bundle = PalwTypedKernelMaterialBundleV4 { claim: h(1), units: (0..5u32).map(|i| (0u8, i, vec![i as u8; 10])).collect() };
        let mut assembler = KernelMaterialPageAssemblerV4::new(h(1), 1 << 20).unwrap();
        let first = kernel_material_page_v4(&bundle, h(2), &request(0, 2, 1024)).unwrap();
        assert_eq!(first.units.len(), 2);
        assert!(assembler.push(kernel_material_page_v4(&bundle, h(2), &request(3, 2, 1024)).unwrap()).is_err(), "gap");
        assembler.push(first).unwrap();
        let mut other = kernel_material_page_v4(&bundle, h(2), &request(2, 3, 1024)).unwrap();
        other.consent = h(9);
        assert!(assembler.push(other).is_err(), "generation substitution");
        assembler.push(kernel_material_page_v4(&bundle, h(2), &request(2, 3, 1024)).unwrap()).unwrap();
        assert!(assembler.complete());
        let (assembled, consent, _) = assembler.finish().unwrap();
        assert_eq!((assembled, consent), (bundle, h(2)));
    }
    #[test]
    fn page_bounds_are_explicit_and_byte_capped() {
        let bundle = PalwTypedKernelMaterialBundleV4 { claim: h(1), units: (0..4u32).map(|i| (0u8, i, vec![0; 100])).collect() };
        assert!(request(0, 0, 10).validate().is_err());
        assert!(request(0, 1, KERNEL_MATERIAL_READ_PAGE_BYTES_V4 + 1).validate().is_err());
        let page = kernel_material_page_v4(&bundle, h(2), &request(0, 4, 250)).unwrap();
        assert_eq!(page.units.len(), 2, "the byte cap ends the page before the unit cap");
        assert!(kernel_material_page_v4(&bundle, h(2), &request(0, 4, 50)).is_err(), "a single oversize unit is refused");
        let mut wrong = request(0, 4, 1000);
        wrong.root = h(7);
        assert!(kernel_material_page_v4(&bundle, h(2), &wrong).is_err());
        let mut small = KernelMaterialPageAssemblerV4::new(h(1), 150).unwrap();
        assert!(small.push(kernel_material_page_v4(&bundle, h(2), &request(0, 4, 1000)).unwrap()).is_err());
    }
}
