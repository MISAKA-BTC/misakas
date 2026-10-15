//! Archive39 is plain data: exclusive bounded files under original identities.
//! (Unbuilt test source; the consolidated validation phase runs it.)
use kaspa_consensus_core::palw_native_execution_v39::NativeOriginalSourceArchiveV39;
use misaka_palw_sdk::native_archive_v39::NativeArchiveDirV39;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("archive39-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn absent_bodies_are_none_and_bounds_refuse_oversized_files() {
    let dir = scratch("bounds");
    let archive = NativeArchiveDirV39::create(&dir).unwrap();
    assert_eq!(archive.source([1; 64], 10).unwrap(), None, "an absent body is never invented");
    assert_eq!(archive.consent([2; 64], 10).unwrap(), None);
    std::fs::write(dir.join("class").join(misaka_palw_sdk::runtime_pack::commit::hex(&[3u8; 64])), vec![9u8; 20]).unwrap();
    assert!(archive.class([3; 64], 19).is_err(), "a body above the caller's original typed bound is refused");
    assert_eq!(archive.class([3; 64], 20).unwrap(), Some(vec![9u8; 20]));
    let _ = std::fs::remove_dir_all(&dir);
}
