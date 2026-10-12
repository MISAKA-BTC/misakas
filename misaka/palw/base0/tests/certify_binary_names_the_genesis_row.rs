//! **The same equality as `artifact_names_the_genesis_row`, through the BINARY.**
//!
//! That file asserts at the library level: it calls `a16_row_for_artifact_shape_v1` and
//! `a16_ladder_row_v1` directly. This one runs `palw-certify` the way an operator does — through
//! its own argument parsing, its own `(artifact, model_id, n_ctx)` match arm, its own printing —
//! and reads the class id out of what it actually wrote to stdout.
//!
//! The gap between the two is real and is where this defect lived. The library route was rewritten
//! to MATCH a catalog row rather than project a graph, and two of the binary's three bind forms
//! were re-pointed at it; `--n-ctx` alone was still calling the projecting helper. A library test
//! could not see that, because the library function it called was already correct. The wiring was
//! the wrong half.
//!
//! **Two modes, and the skip is loud.** The `--n-ctx` form needs nothing on disk, so it runs
//! everywhere, always. The `--artifact` form needs the 1.7 GiB `.palwart` and runs when
//! `MISAKA_PALW_ARTIFACT` names one — announcing itself when it does not, because a check that
//! quietly did not run is worth less than no check.

use std::process::Command;

/// The class id `palw-certify` printed, read out of the line it writes on success.
///
/// Parsed rather than pattern-matched loosely: the id is the last whitespace-delimited token of
/// the `wrote …: ClassLaneCertified, <lane> lane, class <id>` line. A test that searched stdout for
/// the expected id would pass on a build that printed it in a diagnostic and bound something else.
fn class_id_from(stdout: &str) -> String {
    let line = stdout
        .lines()
        .find(|l| l.contains("ClassLaneCertified") && l.contains(" class "))
        .unwrap_or_else(|| panic!("palw-certify printed no `ClassLaneCertified … class <id>` line:\n{stdout}"));
    line.rsplit(" class ")
        .next()
        .expect("the line contains ` class `")
        .split_whitespace()
        .next()
        .expect("a class id follows")
        .to_string()
}

/// The row 5f registers, derived here rather than quoted. Three adjacent ids are loose in this
/// project's notes for three different things, and a class id quoted from a summary is what burned
/// n_ctx 17 on 2026-08-28.
fn genesis_class_id() -> String {
    misaka_palw_base0::classes::a16_graph_v5_row_v1().expect("the graph-v5 dense row projects").profile.shape_profile_id().to_string()
}

fn run(args: &[&str], out: &std::path::Path) -> String {
    let output =
        Command::new(env!("CARGO_BIN_EXE_palw-certify")).args(args).arg("--out").arg(out).output().expect("palw-certify runs");
    assert!(
        output.status.success(),
        "palw-certify {args:?} failed ({}):\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// **`bind --n-ctx 512`, on both lanes, names the class genesis registers.**
///
/// This is the form that was projecting. It printed a class id, `covering_rc_family_v1` found a
/// family for it (a v2 projection at 512 reaches kernels the graph-v2 family covers), the object
/// was well-formed, and the certificate named a class the chain will never hold — with no error
/// anywhere. The first symptom would have been a free-prompt refusal that reads like the width
/// wall, weeks later.
#[test]
fn the_binary_binds_the_genesis_row_at_the_registered_width() {
    let dir = std::env::temp_dir().join("palw-certify-binary-test");
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    let genesis = genesis_class_id();
    let width = misaka_palw_base0::classes::a16_graph_v5_row_v1().expect("projects").profile.n_ctx.to_string();

    for lane in ["attempt", "fp"] {
        let out = dir.join(format!("bind-{lane}.json"));
        let stdout = run(&["bind", "--n-ctx", &width, "--lane", lane], &out);
        assert_eq!(
            class_id_from(&stdout),
            genesis,
            "`palw-certify bind --n-ctx {width} --lane {lane}` bound a class genesis does not register.\n{stdout}"
        );
        // **The certificate it WROTE is the artifact of record, and it is decoded, not searched.**
        // An assertion on stdout alone passes on a build that prints the right id and serialises a
        // different one; a substring search over the bytes would not even find it, because the file
        // is borsh and the id is 64 raw bytes rather than the hex the message prints. So this
        // deserialises the object the chain would receive and reads `class_id` off it — the same
        // field `apply_object` looks up.
        assert!(out.exists(), "the {lane} lane printed success and wrote no certificate");
        let bytes = std::fs::read(&out).expect("the certificate is readable");
        let object: kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 =
            borsh::from_slice(&bytes).expect("the certificate is a borsh consensus object");
        match object {
            kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassLaneCertified { class_id, profile, .. } => {
                assert_eq!(class_id.to_string(), genesis, "the {lane} lane printed {genesis} and serialised class {class_id}");
                // And the object's own profile hashes to the class it names — the equality
                // `apply_object` enforces, checked here so a mismatch is caught on this machine
                // rather than by the chain refusing the object.
                assert_eq!(
                    profile.shape_profile_id(),
                    class_id,
                    "the certificate's profile does not hash to the class id it declares"
                );
            }
            other => panic!("the {lane} lane wrote a {other:?}, not a ClassLaneCertified"),
        }
    }
}

/// **A width no row spells fails to bind, and the binary says so rather than printing an id.**
///
/// The other half of the same property: matching can refuse and projecting cannot. n_ctx 16 is
/// spelled by three A16 rows, which are three different CLASSES at one width, so the operator has
/// to say which.
#[test]
fn the_binary_refuses_an_ambiguous_width_instead_of_picking() {
    let out = std::env::temp_dir().join("palw-certify-binary-test/should-not-exist.json");
    let _ = std::fs::remove_file(&out);
    let output = Command::new(env!("CARGO_BIN_EXE_palw-certify"))
        .args(["bind", "--n-ctx", "16", "--lane", "attempt", "--out"])
        .arg(&out)
        .output()
        .expect("palw-certify runs");
    assert!(!output.status.success(), "an ambiguous width must not exit 0");
    let all = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(all.contains("not one"), "the refusal must say the width names more than one row:\n{all}");
    assert!(all.contains("graph-v2"), "the refusal must name the rows so the operator can choose:\n{all}");
    assert!(!out.exists(), "a refused bind wrote a certificate anyway");
}

/// **The real file, through the binary.** The only version that catches a converter whose header
/// stopped saying what this build believes it writes.
#[test]
fn the_binary_binds_the_genesis_row_from_the_shipped_artifact() {
    let Ok(path) = std::env::var("MISAKA_PALW_ARTIFACT") else {
        println!(
            "SKIPPED: set MISAKA_PALW_ARTIFACT to the shipped .palwart to run the --artifact form. \
             No real header reached the binary in this run."
        );
        return;
    };
    let dir = std::env::temp_dir().join("palw-certify-binary-test");
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    let out = dir.join("bind-artifact.json");
    let stdout = run(&["bind", "--artifact", &path, "--lane", "attempt"], &out);
    assert_eq!(
        class_id_from(&stdout),
        genesis_class_id(),
        "`palw-certify bind --artifact {path}` bound a class genesis does not register.\n{stdout}"
    );
}
