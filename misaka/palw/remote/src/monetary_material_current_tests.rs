//! The production current-generation gate over a real admitted and cold-reopened
//! provider store. Native authority outcomes are injected; these are storage / gate
//! regressions, not an actual native reorg or provider-host acceptance campaign.
use super::*;
use kaspa_consensus_core::palw_material_capacity_v1::PalwMonetaryMaterialDemandV1;
use std::cell::Cell;

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
struct FixtureDir(PathBuf);
impl FixtureDir {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "misaka-current-provider-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        )))
    }
    fn file(&self) -> PathBuf {
        self.0.join("provider")
    }
}
impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn typed_store(file: &Path) -> (DurableMonetaryMaterialProviderV1, Hash64, Hash64, Hash64, PalwBondKeyV2) {
    use crate::monetary_gc_kernel_fixture::ledger_world::{position, World};
    use kaspa_consensus_core::{
        palw_material_capacity_v1::{PalwKernelMaterialBundleV1, PalwMaterialManifestV1},
        palw_monetary_material_v1::*,
    };
    let mut world = World::new();
    let job = world.post_job(2, &[3, 17], 1, 0x71);
    let made = world.honest(&job, 1);
    let claim = made.claim.id();
    world.block(4, vec![made.tx]);
    assert!(world.l.claims.contains_key(&claim));
    let bundle = PalwKernelMaterialBundleV1 {
        claim: Hash64::from_bytes(claim),
        positions: (0..made.trace.values.len()).map(|p| (0, p as u32, position(&made.trace, p as u32, |_| {}))).collect(),
    };
    let scope = bundle.verify(&world.l, 1 << 24).unwrap();
    let root = bundle.claim;
    let manifest = PalwMaterialManifestV1 { root, scopes: vec![scope] };
    let provider = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(8), 0));
    let enrollment = PalwMonetaryMaterialEnrollmentV1 {
        terms: PalwMonetaryMaterialTermsV1 {
            version: 1,
            root,
            dependencies: vec![],
            exec_plan_root: None,
            sponsor: provider,
            nonce: h(90),
            policy: h(91),
            max_material_bytes: 1 << 24,
            material_manifest: Some(manifest),
            native_session: None,
            window_daa: 436,
            enrollment_until_daa: 100,
            payment_relative_until_finality: true,
        },
        offers: vec![PalwMonetaryMaterialOfferV1 { provider, collateral_sompi: 1, prepaid_fee_sompi: 1, signature: vec![] }],
    };
    let mut store = DurableMonetaryMaterialProviderV1::open(file, 1 << 24, 1 << 26, Some(&world.l), |_| None).unwrap();
    store.prepare_material(provider, &enrollment, &[], std::slice::from_ref(&bundle), &[], Some(&world.l), |_| None).unwrap();
    let preferred = enrollment.id();
    let mut replacement = enrollment.clone();
    replacement.terms.nonce = h(92);
    // The same authenticated bytes can be retained under another signed
    // consent. This is a storage fixture, not native replacement acceptance.
    store.prepare_material(provider, &replacement, &[], std::slice::from_ref(&bundle), &[], Some(&world.l), |_| None).unwrap();
    let fallback = replacement.id();
    drop(store);
    let store = DurableMonetaryMaterialProviderV1::open(file, 1 << 24, 1 << 26, Some(&world.l), |_| None).unwrap();
    (store, preferred, fallback, root, provider)
}

#[test]
fn preferred_current_generation_cannot_answer_without_current_authority() {
    let dir = FixtureDir::new();
    let (store, preferred, _, root, provider) = typed_store(&dir.file());
    assert!(store.contexts.is_none());
    let demand = PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 };
    // The historical compatibility reader stays usable without current authority.
    assert!(store.answer_material_owned_v1(preferred, root, root, provider, demand).is_ok());
    let calls = Cell::new(0);
    let result = store.answer_current_generation_with_v4(
        preferred,
        root,
        root,
        false,
        |_, _| {
            calls.set(calls.get() + 1);
            Err("no current accepted generation".into())
        },
        |_, _| -> Result<(), String> { panic!("absent current authority must precede response assembly") },
    );
    assert_eq!(result.unwrap_err(), "no current accepted generation");
    assert_eq!(calls.get(), 2, "the preferred and fallback both need current authority");
}

#[test]
fn fallback_requires_its_own_authority_and_exact_read_keeps_its_consent() {
    let dir = FixtureDir::new();
    let (store, preferred, fallback, root, provider) = typed_store(&dir.file());
    let demand = PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 };
    let calls = Cell::new(0);
    let authority = |consent, _: &_| {
        calls.set(calls.get() + 1);
        if consent == preferred {
            Err("preferred no longer current".into())
        } else {
            assert_eq!(consent, fallback);
            Ok(())
        }
    };
    let answer = store
        .answer_current_generation_with_v4(preferred, root, root, false, authority, |consent, _| {
            store.store.provider_answer_action_v1(consent, root, root, provider, demand)
        })
        .unwrap();
    assert_eq!(calls.get(), 3, "fallback is checked before and after actual response assembly");
    drop(answer);
    let result = store.answer_current_generation_with_v4(
        preferred,
        root,
        root,
        true,
        |consent, _| {
            assert_eq!(consent, preferred);
            Err("exact consent unavailable".into())
        },
        |_, _| -> Result<(), String> { panic!("exact read must not use another consent") },
    );
    assert_eq!(result.unwrap_err(), "exact consent unavailable");
}

#[test]
fn current_generation_capacity_and_reorg_release_actual_answer_owner() {
    const CHILD: &str = "MISAKA_CURRENT_GENERATION_RESOURCE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "monetary_material::current_generation_tests::current_generation_capacity_and_reorg_release_actual_answer_owner",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    use kaspa_consensus_core::palw_native_resource_v1::{
        arm_native_logical_limits_v1, reserve_native_requests_v1, NativeSnapshotResourceLimitsV1,
    };
    let dir = FixtureDir::new();
    let (store, preferred, _, root, provider) = typed_store(&dir.file());
    const LIMIT: u64 = 1 << 24;
    // Store setup precedes arming this isolated process's logical response budget.
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let demand = PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 };
    let calls = Cell::new(0);
    let result = store.answer_current_generation_with_v4(
        preferred,
        root,
        root,
        false,
        |consent, _| {
            assert_eq!(consent, preferred);
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                Ok(())
            } else {
                Err("generation changed during response assembly".into())
            }
        },
        |consent, _| {
            let answer = store.store.provider_answer_action_v1(consent, root, root, provider, demand)?;
            assert!(reserve_native_requests_v1(LIMIT).is_err(), "real answer retains its response allocation ticket");
            Ok(answer)
        },
    );
    assert!(matches!(result, Err(ref reason) if reason == "generation changed during response assembly"));
    assert_eq!(calls.get(), 2, "a failed postcheck must not try a different generation");
    let occupied = reserve_native_requests_v1(LIMIT).unwrap();
    calls.set(0);
    let result = store.answer_current_generation_with_v4(
        preferred,
        root,
        root,
        false,
        |consent, _| {
            assert_eq!(consent, preferred);
            calls.set(calls.get() + 1);
            Ok(())
        },
        |consent, _| store.store.provider_answer_action_v1(consent, root, root, provider, demand),
    );
    assert!(matches!(result, Err(ref reason) if reason.contains("logical native capacity")), "actual local refusal is preserved");
    assert_eq!(calls.get(), 1, "assembly capacity refusal must not become successful consent fallback");
    drop(occupied);
    drop(reserve_native_requests_v1(LIMIT).unwrap());
}
