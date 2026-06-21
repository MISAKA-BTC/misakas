//! Secret-key handling for the keyed (Tier B) commands.
//!
//! HARD RULE: the secret is NEVER accepted as a CLI value argument (it would
//! leak into shell history + the process list). It is loaded only from a
//! permission-checked file (`--key-file`) or stdin (`--key-stdin`). An
//! encrypted keystore (`--wallet` + password) is a planned follow-up; the
//! `KeySource` shape below extends to it without changing call sites.

use std::io::{Read, Write};

use kaspa_addresses::{Address, Prefix};
use kaspa_pq_validator_core::{load_validator_seed, ValidatorKey, VALIDATOR_SEED_LEN};

use crate::{exit, CliError};

/// Where the 32-byte ML-DSA-87 seed comes from. Exactly one source must be set.
pub struct KeySource {
    pub key_file: Option<String>,
    pub key_stdin: bool,
}

impl KeySource {
    pub fn resolve(&self) -> Result<[u8; VALIDATOR_SEED_LEN], CliError> {
        match (&self.key_file, self.key_stdin) {
            (Some(_), true) => Err(CliError::new(exit::GENERIC, "specify only one of --key-file / --key-stdin".to_string())),
            (Some(path), false) => {
                // load_validator_seed warns on world/group-readable perms + hex-decodes exactly 32 bytes.
                load_validator_seed(path).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("--key-file: {e}")))
            }
            (None, true) => {
                let mut s = String::new();
                std::io::stdin()
                    .read_to_string(&mut s)
                    .map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("--key-stdin read: {e}")))?;
                decode_seed_hex(s.trim())
            }
            (None, false) => Err(CliError::new(
                exit::WALLET_LOCKED,
                "no key source — pass --key-file <path> or --key-stdin (the secret is never taken on the command line)".to_string(),
            )),
        }
    }

    pub fn load_key(&self) -> Result<ValidatorKey, CliError> {
        Ok(ValidatorKey::from_seed(self.resolve()?))
    }
}

fn decode_seed_hex(s: &str) -> Result<[u8; VALIDATOR_SEED_LEN], CliError> {
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if h.len() != VALIDATOR_SEED_LEN * 2 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CliError::new(
            exit::WALLET_LOCKED,
            format!("seed must be {} hex chars ({VALIDATOR_SEED_LEN}-byte ML-DSA-87 seed), got {}", VALIDATOR_SEED_LEN * 2, h.len()),
        ));
    }
    let mut seed = [0u8; VALIDATOR_SEED_LEN];
    faster_hex::hex_decode(h.as_bytes(), &mut seed).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("bad seed hex: {e}")))?;
    Ok(seed)
}

/// `misaka key gen --out <path>`: generate a fresh 32-byte ML-DSA-87 seed,
/// write it hex-encoded to `path` (mode 0600, REFUSE to overwrite), and return
/// the derived funding (P2PKH-ML-DSA) address for `prefix`.
pub fn generate(path: &str, prefix: Prefix) -> Result<(Address, [u8; VALIDATOR_SEED_LEN]), CliError> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut seed = [0u8; VALIDATOR_SEED_LEN];
    fill_random(&mut seed)?;
    let mut hex = vec![0u8; VALIDATOR_SEED_LEN * 2];
    faster_hex::hex_encode(&seed, &mut hex).expect("hex encode");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true) // O_EXCL: never clobber an existing key
        .mode(0o600)
        .open(path)
        .map_err(|e| CliError::new(exit::GENERIC, format!("create {path}: {e} (refusing to overwrite an existing key file)")))?;
    f.write_all(&hex).map_err(|e| CliError::new(exit::GENERIC, format!("write {path}: {e}")))?;
    let addr = ValidatorKey::from_seed(seed).funding_address(prefix);
    Ok((addr, seed))
}

/// Dependency-free CSPRNG: 32 bytes from the OS.
fn fill_random(buf: &mut [u8]) -> Result<(), CliError> {
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .map_err(|e| CliError::new(exit::GENERIC, format!("/dev/urandom: {e}")))
}
