//! `palw-evm-runner` — the process that runs model-written initcode, and the only one.
//!
//! **ADR-0078 SA-1:** *"Executing model-written code is the largest privilege in the lineage
//! (ADR-0079 §1), and the in-tree EVM is not exempt. `contract` / `code` under `evm/v1` runs
//! model-written initcode: on an ephemeral, isolated state with a gas ceiling from the transformer
//! manifest, in a separate process under ADR-0079 Decision 5's confinement, never against the
//! chain's EVM state and never inside the node process."*
//!
//! This binary is that separate process. Its whole vocabulary is two frames:
//!
//! ```text
//!   stdin   MEVJ — one canonical job: the run manifest's digest, the deploy data
//!                  (initcode ‖ constructor args) and the calls, each with its calldata,
//!                  its value and its gas limit. CRC-trailered.
//!   stdout  MEVR — one canonical result: the created address, the deploy gas, the runtime
//!                  code and one outcome per call (success, output, gas used) — or a refusal
//!                  code the caller turns into its own sentence. CRC-trailered.
//!   stderr  one line, and only when the job could not be read at all.
//!   exit    0 when a frame was written (a REFUSAL is a frame and exits 0), 2 otherwise.
//! ```
//!
//! What it does not have, by construction rather than by promise:
//!
//! * **No arguments.** `argv` is refused if it carries any, so nothing about a run is chosen at
//!   the command line.
//! * **No ceilings of its own to be told.** The job names the run manifest's DIGEST, not its
//!   numbers; the runner uses the ceilings it was COMPILED with and refuses a job that names any
//!   others. A caller cannot widen a gas ceiling by asking (ADR-0072 Decision 8's rule: a field
//!   the caller chooses freely and no rule pins is a nonce by another name).
//! * **No filesystem, no network, no clock, no randomness.** It reads stdin and writes stdout. The
//!   parent gives it an ephemeral working directory, `env_clear`s it, and — where the host has a
//!   backend that proved its own denials — the OS enforces the rest (ADR-0079 Decision 5).
//! * **No key material and no answer.** It never sees the prompt, the claim, the executor key or
//!   the test names: the parent sends bytes and gas, and the runner returns facts (ADR-0079
//!   Decision 4 — the process that parses a stranger's bytes holds nothing).
//!
//! A crash, a kill at the ceiling or a kill at the deadline is *no object* on the caller's side
//! (ADR-0078 Decision 2's parse-failure arm), never a panic in the caller's process. That is the
//! whole reason this file exists.

use misaka_palw_derive::kinds::code::{
    EvmJobResult, decode_evm_job, encode_evm_result, evm_v1_run_manifest_hash, execute_evm_job_in_this_process, refusal,
};
use std::io::{Read, Write};

fn main() {
    if std::env::args_os().len() > 1 {
        eprintln!("palw-evm-runner takes no arguments: the job is one MEVJ frame on stdin");
        std::process::exit(2);
    }

    let mut frame = Vec::new();
    if let Err(e) = std::io::stdin().lock().read_to_end(&mut frame) {
        eprintln!("palw-evm-runner: cannot read the job frame: {e}");
        std::process::exit(2);
    }

    // A job that does not parse is still answered with a frame: the caller learns "no object" from
    // a refusal it can read, not from a silence it has to interpret.
    let result = match decode_evm_job(&frame) {
        Ok(job) => execute_evm_job_in_this_process(&job),
        Err(e) => EvmJobResult::Refused { code: refusal::JOB_MALFORMED, index: 0, gas_used: 0, detail: e.to_string().into_bytes() },
    };

    let out = encode_evm_result(&evm_v1_run_manifest_hash(), &result);
    let mut stdout = std::io::stdout().lock();
    if stdout.write_all(&out).and_then(|()| stdout.flush()).is_err() {
        // The caller is gone. There is nothing to report to and nothing to clean up.
        std::process::exit(2);
    }
}
