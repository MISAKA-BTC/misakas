//! The current checker retains immutable action slots for canonical reorg
//! replay and funding ownership. Old moved-aside demands cannot be ignored or
//! counted as a safe cursor: their original slot/funding history is missing.
use std::{fs, path::Path};

pub fn refuse_legacy_demand_archive_v4(outbox: &Path) -> Result<(), String> {
    for entry in fs::read_dir(outbox).map_err(|e| e.to_string())? {
        let name = entry.map_err(|e| e.to_string())?.file_name();
        if name.as_encoded_bytes().starts_with(b"demand-") {
            return Err("legacy moved demand WAL is incompatible with immutable action slots; all files retained, no funding or signature rebound".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            loop {
                let path = std::env::temp_dir().join(format!(
                    "misaka-checker-legacy-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => panic!("{e}"),
                }
            }
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn modern_slots_remain_usable_without_moving_their_signed_subjects() {
        let dir = Directory::new();
        fs::write(dir.0.join("receipt.borsh"), b"first subject").unwrap();
        fs::write(dir.0.join("transaction.borsh"), b"first carrier").unwrap();
        fs::create_dir(dir.0.join("action-01")).unwrap();
        fs::write(dir.0.join("action-01/receipt.borsh"), b"second subject").unwrap();
        refuse_legacy_demand_archive_v4(&dir.0).unwrap();
        assert_eq!(fs::read(dir.0.join("receipt.borsh")).unwrap(), b"first subject");
        assert_eq!(fs::read(dir.0.join("transaction.borsh")).unwrap(), b"first carrier");
    }
    #[test]
    fn complete_partial_gapped_or_malformed_legacy_names_hold_all_existing_files() {
        for name in ["demand-00000.borsh", "demand-00000-transaction.borsh", "demand-00002.borsh", "demand-broken"] {
            let dir = Directory::new();
            let archive = dir.0.join(name);
            fs::write(&archive, b"old signed bytes").unwrap();
            let current = dir.0.join("receipt.borsh");
            fs::write(&current, b"current signed bytes").unwrap();
            assert!(refuse_legacy_demand_archive_v4(&dir.0).is_err());
            assert_eq!(fs::read(archive).unwrap(), b"old signed bytes");
            assert_eq!(fs::read(current).unwrap(), b"current signed bytes");
        }
    }
}
