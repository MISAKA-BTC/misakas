//! **Which root form does the on-disk A16 artifact actually derive?** (Relaunch 5 triage)
//!
//! The chain registers `PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT` for the graph-v2 A16 row, but
//! `CanonicalClassV1::artifact_root` derives the court-capable row's root as the A16
//! operand-inventory root, not the flat `artifact_digest()`. Prints both for a file so the two
//! spellings can be compared against the pin. Ignored: it needs a 1.7 GiB file named by
//! `PALW_A16_PATH`.
#[test]
#[ignore]
fn print_a16_root_forms() {
    let path = std::env::var("PALW_A16_PATH").expect("PALW_A16_PATH=/path/to/qwen25-1.5b-a16.palwart");
    let bytes = std::fs::read(&path).expect("read the artifact");
    let artifact = misaka_palw_base0::artifact::decode_artifact_file_v1(&bytes).expect("decode .palwart");
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2(
        kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B_A16,
    )
    .expect("graph-v2 A16 profile projects");
    let digest = artifact.artifact_digest();
    let inventory = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("inventory builds").root();
    println!("file            {path}");
    println!("class_id(v2)    {}", profile.shape_profile_id());
    println!("artifact_digest {digest}");
    println!("inventory_root  {inventory}");
    println!("pinned_const    {}", kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT);
    println!("digest==pin     {}", digest == kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT);
    println!("inventory==pin  {}", inventory == kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT);
}

/// **The shipped A16 row is the court-capable graph-v2 row, so its registered root MUST be the
/// inventory form** — the structural half of the probe above, runnable without the 1.7 GiB file.
///
/// `CanonicalClassV1::artifact_root` picks the root form by `state_chunk_map_id`: the four-byte
/// integer-KV map means "court-capable, register the operand-inventory root", anything else means
/// "v1 row, register the flat digest". The genesis card pinned the digest for a profile whose map
/// is the four-byte one, which is precisely the mismatch this asserts can never be silent again:
/// if this test holds, `PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT` is an inventory root by contract,
/// and the ignored probe is how a human checks the value against the file.
#[test]
fn the_shipped_a16_row_registers_the_inventory_root_form() {
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2(
        kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B_A16,
    )
    .expect("graph-v2 A16 profile projects");
    assert_eq!(
        profile.state_chunk_map_id,
        kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_map_id_v2(),
        "the registered A16 row is court-capable, so its root form is the operand-inventory root — \
         `PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT` must be `a16_inventory_v1(..).root()`, never `artifact_digest()`"
    );
    // The digest of the deployed file is a known constant; the pin must NOT be it.
    let digest_form = "c00faa480f2344d4a737e5b2e87ab6064d8d6e42c1ffeb6aa0a14ed62134299a7c9dc08f15342cefca1e29390810e6d2c5879f4c3853ebe43a9e2d47ed57ba17";
    assert_ne!(
        kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT.to_string(),
        digest_form,
        "the pin is the flat digest again — that is the two-mappings defect, re-pin to the inventory root"
    );
}
