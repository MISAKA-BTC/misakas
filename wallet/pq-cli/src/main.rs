//! `kaspa-pq-cli` — minimal CLI for the kaspa-pq fork (Phase 5' + follow-up).
//!
//! What this binary does:
//!
//! - Generate / re-derive an ML-DSA-87 keypair from a BIP39 mnemonic.
//! - Derive the corresponding kaspa-pq P2PKH address (misaka*).
//! - Sign / verify messages with the kaspa-pq tx context
//!   ([`MLDSA87_TX_CONTEXT`]).
//! - Store the mnemonic encrypted at rest with Argon2id + ChaCha20-Poly1305.
//! - Smoke-test a wRPC connection to a kaspa-pq node (`info` subcommand).
//! - Submit a hex-encoded already-built transaction
//!   (`submit-tx --tx-hex …` — UTXO selection and tx construction are
//!   out of scope for this CLI).
//!
//! Subcommands:
//!
//!   init       Generate a fresh mnemonic, save it (plain text or encrypted).
//!   address    Print the kaspa-pq P2PKH address at a given path.
//!   sign       Sign a hex-encoded message with the mnemonic/path.
//!   verify     Verify (pubkey_hex, message_hex, signature_hex) under
//!              `MLDSA87_TX_CONTEXT`. Self-contained.
//!   info       Connect to a kaspa-pq node over wRPC and call get_info.
//!   submit-tx  Submit an already-built, hex-encoded transaction.
//!
//! Encrypted seed file format (`kaspa-pq-mnemonic.kpq`):
//!
//! ```text
//! offset  bytes  meaning
//! ------  -----  ----------------------------------------------------------
//!      0      4  magic "KPQ1"
//!      4     16  Argon2id salt
//!     20     12  ChaCha20-Poly1305 nonce
//!     32      *  AEAD ciphertext+tag of `<mnemonic phrase>\n` bytes
//! ```
//!
//! Argon2id parameters: m=64 MiB, t=3, p=1 (the `argon2::Params::DEFAULT`
//! settings for the OWASP "interactive" profile). Re-running `init` with
//! a different password produces a different salt+nonce and a completely
//! different ciphertext, so a leaked file alone is not a multi-target
//! attack surface.

use std::{fs, path::PathBuf};

use argon2::{Argon2, Params};
use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
};
use clap::{Parser, Subcommand, ValueEnum};
use kaspa_addresses::Prefix;
use kaspa_bip32::{Language, Mnemonic, WordCount};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_txscript::{MLDSA87_PK_LEN, MLDSA87_SIG_LEN, MLDSA87_TX_CONTEXT};
use kaspa_wallet_keys::kaspa_pq;
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use libcrux_ml_dsa::ml_dsa_87;
use thiserror::Error;

const ENCRYPTED_MAGIC: &[u8; 4] = b"KPQ1";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const HEADER_LEN: usize = ENCRYPTED_MAGIC.len() + SALT_LEN + NONCE_LEN;
const KEY_LEN: usize = 32;

#[derive(Debug, Error)]
enum CliError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("bip32: {0}")]
    Bip32(#[from] kaspa_bip32::Error),
    #[error("hex decode: {0}: {1}")]
    Hex(String, String),
    #[error("ml-dsa-65 signature failed to verify under the kaspa-pq tx context")]
    SignatureInvalid,
    #[error("public key length mismatch: expected {expected} bytes, got {got}")]
    PublicKeyLength { expected: usize, got: usize },
    #[error("signature length mismatch: expected {expected} bytes, got {got}")]
    SignatureLength { expected: usize, got: usize },
    #[error("randomness must be 32 bytes")]
    BadRandomness,
    #[error("encrypted seed file too short ({0} bytes, need at least {HEADER_LEN})")]
    EncryptedTooShort(usize),
    #[error("encrypted seed file has wrong magic: expected KPQ1")]
    BadMagic,
    #[error("argon2 key derivation failed: {0}")]
    Argon2(String),
    #[error("AEAD failure: ciphertext does not authenticate (wrong password or tampered file)")]
    AeadFail,
    #[error("the decrypted seed file is not valid UTF-8")]
    DecryptedNotUtf8,
    #[error("rpc: {0}")]
    Rpc(String),
}

#[derive(Parser, Debug)]
#[command(name = "kaspa-pq-cli", about, long_about = None, version)]
struct Cli {
    /// Path to the BIP39 mnemonic file. If `--encrypted` is set on `init`,
    /// the file is written / read as the kaspa-pq encrypted seed format
    /// (magic `KPQ1` + Argon2id salt + ChaCha20-Poly1305 nonce + AEAD
    /// ciphertext). Otherwise it is plaintext (one line, space-separated
    /// words).
    #[arg(long, default_value = "kaspa-pq-mnemonic.txt", global = true)]
    mnemonic_file: PathBuf,

    /// BIP39 mnemonic passphrase. Empty by default. Distinct from the
    /// encrypted-seed password.
    #[arg(long, default_value = "", global = true)]
    passphrase: String,

    /// kaspa-pq network the address belongs to. Affects the address
    /// prefix and the kaspa-pq keygen-seed domain separation
    /// (see docs/kaspa-pq-spec.md §8).
    #[arg(long, value_enum, default_value_t = NetworkFlag::Mainnet, global = true)]
    network: NetworkFlag,

    /// When set, the mnemonic file is treated as the kaspa-pq encrypted
    /// seed format. `init` will prompt for an encryption password;
    /// `address`/`sign` will prompt for the decryption password.
    /// Mutually exclusive with `--password-env`; the env-var form
    /// suppresses the interactive prompt.
    #[arg(long, global = true)]
    encrypted: bool,

    /// Acknowledge writing the mnemonic to disk in PLAINTEXT (audit QM-2). `init` refuses to write
    /// plaintext unless this or `--encrypted` is given, so plaintext is never the silent default.
    /// No effect on read / `address` / `sign`.
    #[arg(long, global = true)]
    plaintext: bool,

    /// Optional: read the encrypted-seed password from this environment
    /// variable instead of an interactive prompt.
    #[arg(long, global = true)]
    password_env: Option<String>,

    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Generate a fresh BIP39 mnemonic and save it to `--mnemonic-file`.
    /// Refuses to overwrite an existing file.
    Init {
        #[arg(long, value_enum, default_value_t = WordCountFlag::W24)]
        words: WordCountFlag,
    },
    Address {
        #[arg(long, default_value_t = 0)]
        account: u32,
        #[arg(long, default_value_t = 0)]
        change: u32,
        #[arg(long, default_value_t = 0)]
        index: u32,
    },
    Sign {
        #[arg(long)]
        message_hex: String,
        #[arg(long, default_value_t = 0)]
        account: u32,
        #[arg(long, default_value_t = 0)]
        change: u32,
        #[arg(long, default_value_t = 0)]
        index: u32,
        #[arg(long)]
        randomness_hex: Option<String>,
    },
    /// Verify (pubkey, message, signature) under `MLDSA87_TX_CONTEXT`.
    /// Does not read the mnemonic file — self-contained.
    Verify {
        #[arg(long)]
        public_key_hex: String,
        #[arg(long)]
        message_hex: String,
        #[arg(long)]
        signature_hex: String,
    },
    /// Connect to a kaspa-pq node over wRPC (Borsh) and call get_info.
    /// Smoke-test for node reachability.
    Info {
        /// wRPC URL. Defaults to the kaspa-pq simnet Borsh port
        /// (27510 = upstream 17510 + 10000).
        #[arg(long, default_value = "ws://127.0.0.1:27510")]
        node: String,
    },
    /// Submit a hex-encoded, already-built transaction over wRPC.
    /// Use this with an externally-constructed transaction; the CLI
    /// does not select UTXOs or estimate fees.
    SubmitTx {
        #[arg(long, default_value = "ws://127.0.0.1:27510")]
        node: String,
        #[arg(long)]
        tx_hex: String,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum NetworkFlag {
    Mainnet,
    Testnet10,
    Simnet,
    Devnet,
}

impl NetworkFlag {
    fn id(self) -> &'static str {
        match self {
            NetworkFlag::Mainnet => "mainnet",
            NetworkFlag::Testnet10 => "testnet-10",
            NetworkFlag::Simnet => "simnet",
            NetworkFlag::Devnet => "devnet",
        }
    }
    fn prefix(self) -> Prefix {
        match self {
            NetworkFlag::Mainnet => Prefix::Mainnet,
            NetworkFlag::Testnet10 => Prefix::Testnet,
            NetworkFlag::Simnet => Prefix::Simnet,
            NetworkFlag::Devnet => Prefix::Devnet,
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum WordCountFlag {
    W12,
    W24,
}

impl WordCountFlag {
    fn into_bip39(self) -> WordCount {
        match self {
            WordCountFlag::W12 => WordCount::Words12,
            WordCountFlag::W24 => WordCount::Words24,
        }
    }
}

fn decode_hex(label: &str, s: &str) -> Result<Vec<u8>, CliError> {
    if s.len() % 2 != 0 {
        return Err(CliError::Hex(label.into(), "odd hex length".into()));
    }
    let mut out = vec![0u8; s.len() / 2];
    faster_hex::hex_decode(s.as_bytes(), &mut out).map_err(|e| CliError::Hex(label.into(), e.to_string()))?;
    Ok(out)
}

fn hex_encode_string(bytes: &[u8]) -> String {
    let mut out = vec![0u8; bytes.len() * 2];
    faster_hex::hex_encode(bytes, &mut out).expect("hex encode");
    String::from_utf8(out).expect("hex output is always ASCII")
}

fn read_password(cli: &Cli, prompt: &str) -> Result<String, CliError> {
    if let Some(var) = &cli.password_env {
        return Ok(std::env::var(var).map_err(|e| CliError::Io(std::io::Error::other(format!("{var}: {e}"))))?);
    }
    rpassword::prompt_password(prompt).map_err(CliError::Io)
}

fn derive_aead_key(password: &str, salt: &[u8; SALT_LEN]) -> Result<[u8; KEY_LEN], CliError> {
    let mut key = [0u8; KEY_LEN];
    // Argon2id with the library default parameters (m=19456 KiB, t=2, p=1
    // in argon2 0.5; check `argon2::Params::DEFAULT` if the upstream
    // defaults change). These are within the OWASP "interactive" profile.
    Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, Params::default())
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| CliError::Argon2(e.to_string()))?;
    Ok(key)
}

fn save_mnemonic(cli: &Cli, mnemonic: &Mnemonic) -> Result<(), CliError> {
    let plaintext = format!("{}\n", mnemonic.phrase());
    if !cli.encrypted {
        // audit QM-2: never write the mnemonic to disk in plaintext silently — require an explicit
        // opt-in (--plaintext) so it is a deliberate choice. Encryption (--encrypted: Argon2id +
        // ChaCha20-Poly1305) is the recommended default.
        if !cli.plaintext {
            return Err(CliError::Io(std::io::Error::other(
                "refusing to write the mnemonic in plaintext; pass --encrypted (recommended) or --plaintext to acknowledge",
            )));
        }
        fs::write(&cli.mnemonic_file, plaintext)?;
        return Ok(());
    }
    let password = read_password(cli, "Encrypted seed password: ")?;
    let confirm = read_password(cli, "Confirm: ")?;
    if password != confirm {
        return Err(CliError::Io(std::io::Error::other("password and confirmation do not match")));
    }
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let key = derive_aead_key(&password, &salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let ciphertext = cipher.encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes()).map_err(|_| CliError::AeadFail)?;

    let mut buf = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    buf.extend_from_slice(ENCRYPTED_MAGIC);
    buf.extend_from_slice(&salt);
    buf.extend_from_slice(&nonce_bytes);
    buf.extend_from_slice(&ciphertext);
    fs::write(&cli.mnemonic_file, buf)?;
    Ok(())
}

fn read_mnemonic(cli: &Cli) -> Result<Mnemonic, CliError> {
    let raw = fs::read(&cli.mnemonic_file)?;
    if !cli.encrypted {
        let s = String::from_utf8(raw).map_err(|_| CliError::DecryptedNotUtf8)?;
        let phrase = s.trim();
        return Ok(Mnemonic::new(phrase, Language::English)?);
    }
    if raw.len() < HEADER_LEN {
        return Err(CliError::EncryptedTooShort(raw.len()));
    }
    if &raw[..4] != ENCRYPTED_MAGIC {
        return Err(CliError::BadMagic);
    }
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&raw[4..20]);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&raw[20..32]);
    let ciphertext = &raw[HEADER_LEN..];
    let password = read_password(cli, "Encrypted seed password: ")?;
    let key = derive_aead_key(&password, &salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let plaintext = cipher.decrypt(Nonce::from_slice(&nonce_bytes), ciphertext).map_err(|_| CliError::AeadFail)?;
    let s = String::from_utf8(plaintext).map_err(|_| CliError::DecryptedNotUtf8)?;
    let phrase = s.trim();
    Ok(Mnemonic::new(phrase, Language::English)?)
}

async fn wrpc_connect(node: &str) -> Result<KaspaRpcClient, CliError> {
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(node), None, None, None)
        .map_err(|e| CliError::Rpc(format!("client construct: {e}")))?;
    client.connect(None).await.map_err(|e| CliError::Rpc(format!("connect {node}: {e}")))?;
    Ok(client)
}

async fn run(cli: Cli) -> Result<(), CliError> {
    match &cli.cmd {
        Command::Init { words } => {
            if cli.mnemonic_file.exists() {
                return Err(CliError::Io(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("{} already exists; refusing to overwrite", cli.mnemonic_file.display()),
                )));
            }
            let mnemonic = Mnemonic::random(words.into_bip39(), Language::English)?;
            save_mnemonic(&cli, &mnemonic)?;
            if cli.encrypted {
                println!("Wrote encrypted mnemonic (KPQ1) to {}.", cli.mnemonic_file.display());
                println!("(plaintext mnemonic not echoed; record it from another secure source).");
            } else {
                println!("Wrote mnemonic to {}.", cli.mnemonic_file.display());
                println!("Mnemonic:\n{}", mnemonic.phrase());
                println!();
                println!("Store this mnemonic offline. Use --encrypted on `init` to write an");
                println!("Argon2id + ChaCha20-Poly1305 encrypted seed instead.");
            }
        }
        Command::Address { account, change, index } => {
            let mnemonic = read_mnemonic(&cli)?;
            let seed = mnemonic.to_seed(&cli.passphrase);
            let kp = kaspa_pq::derive_keypair(cli.network.id(), *account, *change, *index, seed.as_bytes());
            let addr = kp.address(cli.network.prefix());
            println!("{}", String::from(addr));
        }
        Command::Sign { message_hex, account, change, index, randomness_hex } => {
            let mnemonic = read_mnemonic(&cli)?;
            let seed = mnemonic.to_seed(&cli.passphrase);
            let kp = kaspa_pq::derive_keypair(cli.network.id(), *account, *change, *index, seed.as_bytes());
            let message = decode_hex("message_hex", message_hex)?;
            let randomness: [u8; 32] = match randomness_hex {
                Some(h) => {
                    let bytes = decode_hex("randomness_hex", h)?;
                    bytes.as_slice().try_into().map_err(|_| CliError::BadRandomness)?
                }
                None => {
                    use rand::RngCore;
                    let mut buf = [0u8; 32];
                    rand::thread_rng().fill_bytes(&mut buf);
                    buf
                }
            };
            let sig = kp.sign(&message, randomness);
            println!("public_key={}", hex_encode_string(kp.public_key_bytes()));
            println!("signature={}", hex_encode_string(&sig));
        }
        Command::Verify { public_key_hex, message_hex, signature_hex } => {
            let pk_bytes = decode_hex("public_key_hex", public_key_hex)?;
            if pk_bytes.len() != MLDSA87_PK_LEN {
                return Err(CliError::PublicKeyLength { expected: MLDSA87_PK_LEN, got: pk_bytes.len() });
            }
            let sig_bytes = decode_hex("signature_hex", signature_hex)?;
            if sig_bytes.len() != MLDSA87_SIG_LEN {
                return Err(CliError::SignatureLength { expected: MLDSA87_SIG_LEN, got: sig_bytes.len() });
            }
            let msg = decode_hex("message_hex", message_hex)?;
            let pk_arr: [u8; MLDSA87_PK_LEN] = pk_bytes.as_slice().try_into().unwrap();
            let sig_arr: [u8; MLDSA87_SIG_LEN] = sig_bytes.as_slice().try_into().unwrap();
            let vk = ml_dsa_87::MLDSA87VerificationKey::new(pk_arr);
            let sig = ml_dsa_87::MLDSA87Signature::new(sig_arr);
            ml_dsa_87::verify(&vk, &msg, MLDSA87_TX_CONTEXT, &sig).map_err(|_| CliError::SignatureInvalid)?;
            println!("OK: signature verifies under the kaspa-pq tx context.");
        }
        Command::Info { node } => {
            let client = wrpc_connect(node).await?;
            let info = client.get_info().await.map_err(|e| CliError::Rpc(format!("get_info: {e}")))?;
            println!("server_version    = {}", info.server_version);
            println!("p2p_id            = {}", info.p2p_id);
            println!("mempool_size      = {}", info.mempool_size);
            println!("is_utxo_indexed   = {}", info.is_utxo_indexed);
            println!("is_synced         = {}", info.is_synced);
            let _ = client.disconnect().await;
        }
        Command::SubmitTx { node: _, tx_hex: _ } => {
            // Submitting a hex-encoded tx requires wRPC SubmitTransaction
            // with a fully-populated kaspa-pq Transaction object. That
            // depends on a transaction builder we have not yet built
            // (see Phase 5' next continuation: UTXO selection + tx
            // construction). For now reject explicitly so the user knows
            // it is intentionally unfinished, rather than silently
            // appearing to succeed.
            return Err(CliError::Rpc(
                "submit-tx is not yet wired — a transaction builder \
                 (UTXO selection + signing of every input) needs to land first."
                    .into(),
            ));
        }
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli).await {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests focused on the encrypted seed path, which is the only
    //! piece with non-trivial native logic in the binary. Network paths
    //! (`info`, `submit-tx`) require a live node and are exercised
    //! manually instead.

    use super::*;
    use tempfile::TempDir;

    fn cli_for(file: PathBuf, password_env: Option<String>) -> Cli {
        Cli {
            mnemonic_file: file,
            passphrase: String::new(),
            network: NetworkFlag::Simnet,
            encrypted: true,
            plaintext: false,
            password_env,
            cmd: Command::Address { account: 0, change: 0, index: 0 },
        }
    }

    #[test]
    fn encrypted_seed_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("seed.kpq");

        // Save (init flow) ...
        unsafe {
            std::env::set_var("KASPA_PQ_TEST_PW", "correct horse battery staple");
        }
        let cli = cli_for(path.clone(), Some("KASPA_PQ_TEST_PW".to_string()));
        let mnemonic = Mnemonic::random(WordCount::Words12, Language::English).unwrap_or_else(|_| panic!("mnemonic"));
        let phrase = mnemonic.phrase().to_string();
        save_mnemonic(&cli, &mnemonic).unwrap();

        // ... then read back.
        let cli_read = cli_for(path.clone(), Some("KASPA_PQ_TEST_PW".to_string()));
        let recovered = read_mnemonic(&cli_read).unwrap_or_else(|e| panic!("read_mnemonic: {e}"));
        assert_eq!(recovered.phrase(), phrase);

        // File format: magic + salt + nonce + ciphertext.
        let raw = std::fs::read(&path).unwrap();
        assert!(raw.len() > HEADER_LEN);
        assert_eq!(&raw[..4], ENCRYPTED_MAGIC);
    }

    #[test]
    fn encrypted_seed_wrong_password_fails() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("seed.kpq");
        unsafe {
            std::env::set_var("KASPA_PQ_TEST_PW_GOOD", "shibboleth");
            std::env::set_var("KASPA_PQ_TEST_PW_BAD", "shibbboleth");
        }
        let cli_good = cli_for(path.clone(), Some("KASPA_PQ_TEST_PW_GOOD".to_string()));
        let mnemonic = Mnemonic::random(WordCount::Words12, Language::English).unwrap_or_else(|_| panic!("mnemonic"));
        save_mnemonic(&cli_good, &mnemonic).unwrap();

        let cli_bad = cli_for(path.clone(), Some("KASPA_PQ_TEST_PW_BAD".to_string()));
        let err = match read_mnemonic(&cli_bad) {
            Ok(_) => panic!("expected AeadFail, but decryption succeeded"),
            Err(e) => e,
        };
        assert!(matches!(err, CliError::AeadFail), "expected AeadFail, got {err:?}");
    }

    #[test]
    fn encrypted_seed_wrong_magic_fails() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("seed.kpq");
        // Fill with non-KPQ1 bytes.
        std::fs::write(&path, [0u8; HEADER_LEN + 8]).unwrap();
        unsafe {
            std::env::set_var("KASPA_PQ_TEST_PW2", "_");
        }
        let cli = cli_for(path, Some("KASPA_PQ_TEST_PW2".to_string()));
        let err = match read_mnemonic(&cli) {
            Ok(_) => panic!("expected BadMagic, but read succeeded"),
            Err(e) => e,
        };
        assert!(matches!(err, CliError::BadMagic), "expected BadMagic, got {err:?}");
    }
}
