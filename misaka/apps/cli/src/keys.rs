//! Secret-key handling for the keyed (Tier B) commands.
//!
//! HARD RULE: the secret is NEVER accepted as a CLI value argument and NEVER read from the
//! environment (ADR-0063 SA-1). A seed on the command line is in `ps` output and in the shell
//! history on every host it was ever typed on; a seed in an environment variable is inherited by
//! every child this process spawns — the model worker included, until ADR-0079 R-01 lands. It is
//! loaded only from a permission-checked file (`--key-file`) or stdin (`--key-stdin`). An
//! encrypted keystore (`--wallet` + password) is a planned follow-up; the `KeySource` shape below
//! extends to it without changing call sites.

use std::io::{Read, Write};

use kaspa_addresses::{Address, Prefix};
use kaspa_pq_validator_core::{VALIDATOR_SEED_LEN, ValidatorKey, load_validator_seed};
use zeroize::Zeroizing;

use crate::{CliError, exit};

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
                let s = read_all_stdin("--key-stdin")?;
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

/// Read stdin to end, into a buffer that wipes itself on drop. The seed spends its whole life in
/// this crate inside `Zeroizing`; a plain `String` would leave 64 hex characters of secret in the
/// allocator's free list for whatever allocates next.
fn read_all_stdin(what: &str) -> Result<Zeroizing<String>, CliError> {
    let mut s = Zeroizing::new(String::new());
    std::io::stdin().read_to_string(&mut s).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("{what} read: {e}")))?;
    Ok(s)
}

fn decode_seed_hex(s: &str) -> Result<[u8; VALIDATOR_SEED_LEN], CliError> {
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if h.len() != VALIDATOR_SEED_LEN * 2 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        // **The seed is never echoed, not even in the error that rejects it.** A near-miss (63
        // characters, a stray space) is still the operator's live secret, and an error message is
        // the one place a secret escapes into a terminal, a log and a support ticket at once. The
        // length is safe to name; the value is not.
        return Err(CliError::new(
            exit::WALLET_LOCKED,
            format!("seed must be {} hex chars ({VALIDATOR_SEED_LEN}-byte ML-DSA-87 seed), got {}", VALIDATOR_SEED_LEN * 2, h.len()),
        ));
    }
    let mut seed = [0u8; VALIDATOR_SEED_LEN];
    faster_hex::hex_decode(h.as_bytes(), &mut seed).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("bad seed hex: {e}")))?;
    Ok(seed)
}

/// The one mode a key file may have: readable and writable by its owner, by nobody else.
pub const KEY_FILE_MODE: u32 = 0o600;

/// **A key file this process READS must be 0600** (ADR-0063 SA-1).
///
/// `load_validator_seed` warns and continues, which is the right call for a command that is only
/// spending an existing key: refusing there would strand a working fleet on a mode nobody chose.
/// Import is the opposite situation — the operator is handing this tree a secret for the first
/// time — so it is the moment to refuse, while the fix (`chmod 600`) is still free.
///
/// Non-Unix hosts have no mode to check; the file still inherits its directory's ACL, which is the
/// same treatment `generate` gives the file it writes.
#[cfg(unix)]
pub fn require_key_file_mode(path: &str) -> Result<(), CliError> {
    use std::os::unix::fs::PermissionsExt as _;
    let meta = std::fs::metadata(path).map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("stat {path}: {e}")))?;
    let mode = meta.permissions().mode() & 0o777;
    if mode != KEY_FILE_MODE {
        return Err(CliError::new(
            exit::WALLET_LOCKED,
            format!(
                "{path} is mode {mode:04o}, and a seed file must be {KEY_FILE_MODE:04o} — every other bit is somebody else who can read the key. Run `chmod 600 {path}` and retry."
            ),
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn require_key_file_mode(_path: &str) -> Result<(), CliError> {
    Ok(())
}

/// Create `path` with `O_EXCL` at mode 0600 and write `bytes`, then **verify the mode that
/// actually landed** and refuse if it is anything else.
///
/// `OpenOptions::mode` is a request the umask can only subtract from, so the created file can
/// never be more permissive than 0600 — but "cannot be more permissive" is a claim about this
/// build's syscalls, not about the file the operator is left holding (a `CAP_MKNOD` overlay, an
/// NFS mount that ignores the mode, a umask of 0177 that yields 0400). SA-1 says a key file that
/// is not 0600 is not written, so the mode is READ BACK and a wrong one takes the file with it —
/// leaving a half-written secret at a mode nobody checked is the failure this guard exists for.
fn write_key_file_0600(path: &str, bytes: &[u8]) -> Result<(), CliError> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true); // O_EXCL: never clobber an existing key
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(KEY_FILE_MODE);
    }
    let mut f = opts
        .open(path)
        .map_err(|e| CliError::new(exit::GENERIC, format!("create {path}: {e} (refusing to overwrite an existing key file)")))?;
    if let Err(e) = f.write_all(bytes) {
        let _ = std::fs::remove_file(path);
        return Err(CliError::new(exit::GENERIC, format!("write {path}: {e}")));
    }
    drop(f);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        // Ask once, then check. `set_permissions` fixes a restrictive umask (0400) as readily as a
        // permissive one; the check below is what makes the outcome a fact rather than a request.
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(KEY_FILE_MODE));
        if let Err(e) = require_key_file_mode(path) {
            let _ = std::fs::remove_file(path);
            return Err(CliError::new(
                exit::GENERIC,
                format!("{} — the file was removed rather than left holding a secret at a mode this tree did not choose", e.msg),
            ));
        }
    }
    Ok(())
}

/// `misaka key gen --out <path>`: generate a fresh 32-byte ML-DSA-87 seed,
/// write it hex-encoded to `path` (mode 0600, REFUSE to overwrite), and return
/// the derived funding (P2PKH-ML-DSA) address for `prefix`.
///
/// The seed is NOT returned. It used to be, and the caller named it `_seed` and dropped it — so
/// the only effect of returning it was a second unzeroized copy of a brand-new secret on the
/// caller's stack. What an operator needs is the address; the seed is in the file.
pub fn generate(path: &str, prefix: Prefix) -> Result<Address, CliError> {
    let mut seed = Zeroizing::new([0u8; VALIDATOR_SEED_LEN]);
    fill_random(seed.as_mut())?;
    write_seed_and_derive(path, prefix, &seed)
}

/// Where an imported seed is read from. **Neither arm is argv and neither arm is the
/// environment** (ADR-0063 SA-1) — that is the whole point of the type: a third variant carrying
/// the secret itself would have to be written down here, in the one place this rule is stated.
#[derive(Clone, Copy, Debug)]
pub enum SeedSource<'a> {
    /// Piped in: `cat seed.hex | misaka key import --out … --hex-stdin`.
    Stdin,
    /// Named by PATH — the path is an argument, the secret is not. The file must be 0600.
    File(&'a str),
}

/// `misaka key import`: write an EXISTING 32-byte ML-DSA-87 seed to the 0600 file the rest of the
/// CLI consumes (ADR-0063 D1, hardened by SA-1).
///
/// A key this tree cannot import is a key this tree cannot spend, so every backup, air-gapped
/// host and second machine was unreachable — `key gen` and `key address` were the whole surface.
///
/// **The secret arrives on stdin or in a 0600 file, never as an argument and never through the
/// environment.** An argument lands in the shell history, the process table and every `ps` on the
/// box; an environment variable is inherited by every child this process spawns. Same `O_EXCL` +
/// verified-0600 write as `generate`, so an import can no more clobber an existing key than a
/// generate can, and it can no more leave a world-readable one.
///
/// BIP39 is deliberately absent. The web wallet carries a bip39 implementation and this tree
/// carries none, so the two have no agreed derivation to this seed — an import that guessed one
/// would hand back a different address in silence, which is worse than refusing. Specifying that
/// derivation is a prerequisite of importing a mnemonic, not part of this command; when it is
/// specified, `key_roles` holds the role separation SA-2 requires of it.
pub fn import(path: &str, prefix: Prefix, source: SeedSource<'_>) -> Result<Address, CliError> {
    let hex = match source {
        SeedSource::Stdin => read_all_stdin("--hex-stdin")?,
        SeedSource::File(from) => {
            require_key_file_mode(from)?;
            let mut s = Zeroizing::new(String::new());
            std::fs::File::open(from)
                .and_then(|mut f| f.read_to_string(&mut s))
                .map_err(|e| CliError::new(exit::WALLET_LOCKED, format!("--hex-file {from}: {e}")))?;
            s
        }
    };
    let seed = Zeroizing::new(decode_seed_hex(hex.trim())?);
    write_seed_and_derive(path, prefix, &seed)
}

/// Hex-encode `seed` into a self-wiping buffer, write the 0600 file, and return the address.
/// One place, so `gen` and `import` cannot disagree about how a key file is written.
fn write_seed_and_derive(path: &str, prefix: Prefix, seed: &[u8; VALIDATOR_SEED_LEN]) -> Result<Address, CliError> {
    let mut hex = Zeroizing::new(vec![0u8; VALIDATOR_SEED_LEN * 2]);
    faster_hex::hex_encode(seed, hex.as_mut()).expect("hex encode");
    write_key_file_0600(path, &hex)?;
    Ok(ValidatorKey::from_seed(*seed).funding_address(prefix))
}

/// Dependency-free CSPRNG: 32 bytes from the OS.
fn fill_random(buf: &mut [u8]) -> Result<(), CliError> {
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .map_err(|e| CliError::new(exit::GENERIC, format!("/dev/urandom: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("misaka-key-sa1-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[cfg(unix)]
    fn mode_of(p: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    /// **SA-1: a source file whose mode is not 0600 is refused, not warned about.**
    ///
    /// The mode is the whole authorisation on a seed file: 0644 means every account on the host
    /// already has the key, and importing it would copy that exposure into the tree's own file
    /// while reporting success. The fix is one `chmod`, so the refusal costs an operator nothing
    /// and the acceptance would cost them the bond.
    #[cfg(unix)]
    #[test]
    fn a_source_seed_file_that_is_not_0600_is_refused() {
        use std::os::unix::fs::PermissionsExt as _;
        let src = scratch("loose-source.hex");
        let _ = std::fs::remove_file(&src);
        std::fs::write(&src, SEED_HEX).unwrap();
        std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o644)).unwrap();

        let out = scratch("loose-out.key");
        let _ = std::fs::remove_file(&out);
        let err = import(out.to_str().unwrap(), Prefix::Testnet, SeedSource::File(src.to_str().unwrap())).unwrap_err();
        assert!(err.msg.contains("0644"), "the refusal must name the mode it found: {}", err.msg);
        assert!(err.msg.contains("chmod 600"), "and the fix: {}", err.msg);
        assert!(!out.exists(), "a refused import writes nothing");

        // …and the same file at 0600 imports.
        std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o600)).unwrap();
        let addr = import(out.to_str().unwrap(), Prefix::Testnet, SeedSource::File(src.to_str().unwrap())).unwrap();
        assert_eq!(mode_of(&out), 0o600, "and what it writes is 0600");
        // The address is the seed's, not a fresh key's: an import that silently generated would be
        // the exact failure D1 exists to prevent.
        assert_eq!(addr, ValidatorKey::from_seed(super::decode_seed_hex(SEED_HEX).unwrap()).funding_address(Prefix::Testnet));
        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&out);
    }

    /// **SA-1: the file this command WRITES is 0600, whatever the umask says.**
    ///
    /// `OpenOptions::mode` is a request the umask subtracts from, so a 0177 umask yields 0400 and
    /// a host that ignores the mode yields whatever it likes. The write reads the mode back.
    #[cfg(unix)]
    #[test]
    fn a_written_key_file_is_0600_and_refuses_to_clobber() {
        let out = scratch("written.key");
        let _ = std::fs::remove_file(&out);
        generate(out.to_str().unwrap(), Prefix::Testnet).unwrap();
        assert_eq!(mode_of(&out), 0o600);
        // O_EXCL: a second write at the same path is an error, not a silent replacement of a key
        // whose only copy may be that file.
        let err = generate(out.to_str().unwrap(), Prefix::Testnet).unwrap_err();
        assert!(err.msg.contains("refusing to overwrite"), "{}", err.msg);
        let _ = std::fs::remove_file(&out);
    }

    /// A malformed seed is refused WITHOUT the seed appearing in the error. The error is the one
    /// place a secret reaches a terminal, a log and a support ticket in a single step.
    #[test]
    fn a_rejection_never_echoes_the_seed() {
        let bad = "0123456789abcdef";
        let err = decode_seed_hex(bad).unwrap_err();
        assert!(!err.msg.contains(bad), "the rejected value must not be quoted back: {}", err.msg);
        assert!(err.msg.contains("64 hex chars"), "{}", err.msg);
    }
}
