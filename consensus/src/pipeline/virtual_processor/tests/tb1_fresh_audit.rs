//! Private dealer fixture for a new signed MODEL_AUDIT and its independent PANEL.
//! The public archive carries only digests, signed READY rows and accepted blocks.
use super::*;
use sha2::{Digest, Sha512};
use std::{fs, os::unix::fs::DirBuilderExt, path::PathBuf};

fn seal(round: &[u8; 64], slot: usize, bytes: &[u8]) -> Hash64 {
    Hash64::from_bytes(
        Sha512::new()
            .chain_update(b"MISAKA/test/TB1/retained-input/v1")
            .chain_update(round)
            .chain_update((slot as u32).to_be_bytes())
            .chain_update(bytes)
            .finalize()
            .into(),
    )
}

fn private_dir(path: &Path) {
    fs::DirBuilder::new().mode(0o700).create(path).unwrap();
}

const ROLES: [&str; 2] = ["audit", "panel"];

pub(super) fn provision(
    scratch: &Path,
    ids: [&[u8; 64]; 2],
    public: &[ml_dsa_65::MLDSA65VerificationKey],
) -> ([[Hash64; 4]; 2], [[Hash64; 3]; 2]) {
    let root = PathBuf::from(std::env::var_os("MISAKA_PRIVATE_FRESH_CUSTODY_CREATE").expect("new custody directory"));
    private_dir(&root);
    let holders: [_; 3] = std::array::from_fn(|slot| root.join(format!("holder-{slot}")));
    for point in &holders {
        private_dir(point);
    }
    let mut seals = [[Hash64::default(); 3]; 2];
    let roots = std::array::from_fn(|i| {
        let staging = scratch.join(ROLES[i]);
        private_dir(&staging);
        let setup = stable_panel::provision(&staging, None, 0, ids[i]);
        for (slot, holder) in holders.iter().enumerate() {
            let dir = holder.join(ROLES[i]);
            private_dir(&dir);
            let name = format!("panel-dealer-input-{slot}.json");
            let bytes = fs::read(staging.join(&name)).unwrap();
            let mut file = File::create_new(dir.join(&name)).unwrap();
            file.write_all(&bytes).unwrap();
            file.sync_all().unwrap();
            seals[i][slot] = seal(ids[i], slot, &bytes);
            Tb1ExposureJournal::provision(&dir, ids[i], &public[slot], slot as u32).unwrap();
            File::open(holder).unwrap().sync_all().unwrap();
        }
        setup.roots
    });
    File::open(&root).unwrap().sync_all().unwrap();
    File::open(root.parent().unwrap()).unwrap().sync_all().unwrap();
    (roots, seals)
}

pub(super) fn observe(
    node: &Rig,
    registered: Hash64,
    preparation: Hash64,
    close: Hash64,
    cut: &PreparedAudit<'_>,
    rounds: [Tb1MiningRound; 2],
    qualified: [&Tb1Qualification<'_>; 2],
    roots: [[Hash64; 4]; 2],
    seals: [[Hash64; 3]; 2],
    public: &[ml_dsa_65::MLDSA65VerificationKey],
    identity: Identity,
    reader: bool,
) {
    let slot: usize = if reader { std::env::var("MISAKA_PRIVATE_FRESH_SLOT").unwrap().parse().unwrap() } else { 0 };
    assert!(slot < 3);
    let holder = if reader {
        PathBuf::from(std::env::var_os("MISAKA_PRIVATE_FRESH_HOLDER").expect("this holder's retained data"))
    } else {
        PathBuf::from(std::env::var_os("MISAKA_PRIVATE_FRESH_CUSTODY_CREATE").unwrap()).join("holder-0")
    };
    let ids = rounds.map(|r| r.id());
    let api = node.api().as_ref();
    let marker = cut.close_commitment(preparation);
    let freeze = cut.holder_freeze(api, close, qualified[0]).unwrap();
    let (tip, state) = node.chain.tip_state();
    let now = node.chain.daa_of(tip);
    assert!((rounds[0].release_from_daa..rounds[0].deadline_exclusive_daa).contains(&now));
    assert_eq!(cut.observe(api, close, qualified[0]).err().unwrap().kind(), std::io::ErrorKind::WouldBlock);
    for (i, role) in ROLES.iter().enumerate() {
        let dir = holder.join(role);
        let bytes = fs::read(dir.join(format!("panel-dealer-input-{slot}.json"))).unwrap();
        assert_eq!(seal(&ids[i], slot, &bytes), seals[i][slot]);
        let provision: stable_panel::Provision = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(provision.roots, roots[i]);
        let shares = provision.holder_shares();
        assert_eq!(shares.len(), 3);
        let mut journal = Tb1ExposureJournal::resume(&dir, &ids[i], &public[slot], slot as u32).unwrap();
        if i == 0 {
            let row = Tb1HolderRelease {
                context: Tb1ReleaseContext { round_ref: &ids[i], qual_root: qualified[i].root(), freeze_id: marker },
                holder_slot: slot,
                shares: &shares,
            };
            assert_eq!(
                journal.reserve_signing_message(api, freeze, qualified[i], &row, 2048).unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        }
    }
    peer_log!(
        "fresh retained audit {} {}",
        std::process::id(),
        serde_json::json!({"slot":slot,"cold":reader,"class":identity.class,"inventory":identity.inventory,
        "descriptor":identity.descriptor,"registered":registered,"preparation":preparation,"close":close,"freeze":Hash64::from_bytes(marker),
        "rounds":ids.map(Hash64::from_bytes),"qualifications":qualified.map(|q|Hash64::from_bytes(q.root())),"seals":seals.map(|s|s[slot]),"tip":tip,"state":state.state_root(),
        "now":now,"blue":api.get_header(tip).unwrap().blue_score,"audit_from":rounds[0].release_from_daa,"audit_deadline":rounds[0].deadline_exclusive_daa,
        "panel_from":rounds[1].release_from_daa,"panel_deadline":rounds[1].deadline_exclusive_daa,"pruning_depth":node.config.params.pruning_depth(),
        "finality_depth":node.config.params.finality_depth(),"publication":"held","signing":"held"})
    );
}
