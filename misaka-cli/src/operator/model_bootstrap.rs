//! Prepare actual signed bootstrap objects from a retained registration and file.
use crate::keys::KeySource;
use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use misaka_palw_sdk::bootstrap_check::{
    BootstrapSnapshotPinsV1, CompleteBootstrapArtifactV1, CompleteBootstrapCostsV1, authenticate_bootstrap_snapshot_v1,
};
use misaka_palw_sdk::onboarding_chain::EnvelopeSigner;
use std::{io::Read, path::Path, path::PathBuf, str::FromStr, sync::OnceLock};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum Stage {
    ArtifactBind,
    KernelRegister,
    KernelBind,
    Commit,
}

pub(crate) struct PrepareArgs {
    pub stage: Stage,
    pub registration: PathBuf,
    pub artifact: PathBuf,
    pub max_positions: u32,
    pub bond: String,
    pub key: KeySource,
    pub out: PathBuf,
    pub maximum_binding_reservation_sompi: u64,
    pub maximum_check_fee_sompi: u64,
    pub ledger_root: Option<String>,
    pub aux_root: Option<String>,
    pub tip_daa: Option<u64>,
    pub commitment: Option<PathBuf>,
}

fn bad(message: impl Into<String>) -> CliError {
    CliError::new(exit::CONFIG, message.into())
}

fn bounded_borsh<T: borsh::BorshDeserialize>(path: &Path, maximum: u64) -> Result<T, CliError> {
    let file = std::fs::File::open(path).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", path.display())))?;
    if file.metadata().map_err(|e| bad(e.to_string()))?.len() > maximum {
        return Err(bad(format!("{} exceeds the {maximum}-byte preparation limit", path.display())));
    }
    let mut bytes = Vec::new();
    file.take(maximum + 1).read_to_end(&mut bytes).map_err(|e| bad(e.to_string()))?;
    if bytes.len() as u64 > maximum {
        return Err(bad("the local object file grew beyond its preparation limit"));
    }
    borsh::from_slice(&bytes).map_err(|e| bad(format!("{}: {e}", path.display())))
}

// Semantic/file/clock/cost checks happen inside the SDK before it invokes this
// signer. Key loading remains lazy so a refused preparation cannot touch a key.
struct Signer<'a> {
    source: &'a KeySource,
    loaded: OnceLock<Result<kaspa_pq_validator_core::ValidatorKey, String>>,
}
impl Signer<'_> {
    fn key(&self) -> Result<&kaspa_pq_validator_core::ValidatorKey, String> {
        self.loaded.get_or_init(|| self.source.load_key().map_err(|e| e.msg)).as_ref().map_err(Clone::clone)
    }
}
impl EnvelopeSigner for Signer<'_> {
    fn public_key(&self) -> Vec<u8> {
        self.key().map(|k| k.public_key().to_vec()).unwrap_or_default()
    }
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        Ok(self.key()?.sign_with_context(message, context).to_vec())
    }
}

pub(crate) async fn prepare(ctx: &Ctx, args: PrepareArgs) -> CliResult {
    let costs = CompleteBootstrapCostsV1 {
        maximum_binding_reservation_sompi: args.maximum_binding_reservation_sompi,
        maximum_check_fee_sompi: args.maximum_check_fee_sompi,
    };
    costs.check().map_err(|e| bad(e.to_string()))?;
    let (txid, index) = args.bond.split_once(':').ok_or_else(|| bad("bond must be <txid>:<index>"))?;
    let bond = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        txid.parse().map_err(|_| bad("invalid bond transaction id"))?,
        index.parse().map_err(|_| bad("invalid bond index"))?,
    ));
    let network = kaspa_consensus_core::network::NetworkId::from_str(&ctx.network).map_err(|e| bad(e.to_string()))?;
    let params = crate::wallet::chain_params(ctx, network)?.0;
    let domain = misaka_palw_sdk::onboarding_chain::network_domain_v1(&params);
    let rooted = if matches!(args.stage, Stage::ArtifactBind) {
        if args.ledger_root.is_some() || args.aux_root.is_some() || args.tip_daa.is_some() {
            return Err(bad(
                "artifact-bind prepares a local statement; use kernel-register or later stages for authenticated node reads",
            ));
        }
        None
    } else {
        let needed = || bad("this bootstrap stage requires --ledger-root, --aux-root and --tip-daa from one trusted chain view");
        let pins = BootstrapSnapshotPinsV1 {
            ledger_root: args.ledger_root.as_deref().ok_or_else(needed)?.parse().map_err(|_| bad("invalid --ledger-root"))?,
            aux_root: args.aux_root.as_deref().ok_or_else(needed)?.parse().map_err(|_| bad("invalid --aux-root"))?,
            tip_daa: args.tip_daa.ok_or_else(needed)?,
        };
        let nv = crate::wallet::connect(ctx).await?;
        let (snapshot, rows) = super::model_complete::kernel_snapshot(&nv).await?;
        // Authenticate before the artifact's exhaustive execution and key load.
        authenticate_bootstrap_snapshot_v1(&snapshot, rows.clone(), pins, domain).map_err(|e| bad(e.to_string()))?;
        Some((snapshot, rows, pins))
    };
    let commitment = match (args.stage, args.commitment.as_deref()) {
        (Stage::Commit, Some(path)) => Some(bounded_borsh::<misaka_palw_sdk::bootstrap_check::ConformanceCommitmentV1>(path, 4096)?),
        (Stage::Commit, None) => {
            return Err(bad("commit requires --commitment-borsh with the caller's source/implementation/scope/resource statement"));
        }
        (_, Some(_)) => return Err(bad("--commitment-borsh belongs to the commit stage")),
        _ => None,
    };
    let registration = bounded_borsh(&args.registration, 2 << 20)?;
    let artifact = CompleteBootstrapArtifactV1::from_registration_file(&registration, &args.artifact, args.max_positions, costs)
        .map_err(|e| bad(e.to_string()))?;
    if artifact.registrant() != bond {
        return Err(bad("--bond differs from the retained registration's registrant"));
    }
    let signer = Signer { source: &args.key, loaded: OnceLock::new() };
    let object = match args.stage {
        Stage::ArtifactBind => artifact.signed_artifact_binding(domain, bond, &signer),
        Stage::KernelRegister => {
            let (snapshot, rows, pins) = rooted.expect("rooted stages were checked above");
            artifact.signed_kernel_registration(&snapshot, rows, pins, domain, bond, &signer)
        }
        Stage::KernelBind => {
            let (snapshot, rows, pins) = rooted.expect("rooted stages were checked above");
            artifact.signed_kernel_binding(&snapshot, rows, pins, domain, bond, &signer)
        }
        Stage::Commit => {
            let (snapshot, rows, pins) = rooted.expect("rooted stages were checked above");
            artifact.signed_conformance_commitment(
                &snapshot,
                rows,
                pins,
                params.genesis.hash,
                domain,
                commitment.expect("commit metadata was checked above"),
                bond,
                &signer,
            )
        }
    }
    .map_err(|e| bad(e.to_string()))?;
    let bytes = borsh::to_vec(&object).map_err(|e| bad(e.to_string()))?;
    std::fs::write(&args.out, &bytes).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", args.out.display())))?;
    if ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::json!({
                "schema":"misaka.palw.onboard.bootstrap-prepare.v1", "stage":format!("{:?}",args.stage),
                "verification_scope":if matches!(args.stage,Stage::ArtifactBind){"local-file-statement"}else{"caller-pinned-bootstrap-snapshot"},
                "class":artifact.class().to_string(), "artifact_root":artifact.artifact_root().to_string(),
                "kernel_class":artifact.kernel_class().to_string(), "kernel_param_root":artifact.kernel_param_root().to_string(),
                "ledger_root":args.ledger_root,"aux_root":args.aux_root,"tip_daa":args.tip_daa,
                "inputs_reexecuted":artifact.domain.inputs,"artifact_bytes":artifact.domain.artifact_bytes,"work":artifact.domain.work,
                "binding_reservation_sompi":kaspa_consensus_core::palw_onboarding_v1::PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1.to_string(),
                "complete_check_fee_sompi":kaspa_consensus_core::palw_opv_bootstrap_v1::PALW_COMPLETE_CHECK_FEE_SOMPI_V1.to_string(),
                "maximum_binding_reservation_sompi":args.maximum_binding_reservation_sompi.to_string(),
                "maximum_check_fee_sompi":args.maximum_check_fee_sompi.to_string(),
                "out":args.out.display().to_string(),"bytes":bytes.len(),"submitted":false,"native_registration_inclusion_verified":false,
                "mature_useful_work_measured":false,"trained_model_verified":false,"full_rfc12_security_approval":false,
            })
        );
    } else {
        println!("wrote {} ({} bytes), {:?}, class {}", args.out.display(), bytes.len(), args.stage, artifact.class());
        println!("submit with misaka palw submit-object --object {}", args.out.display());
        println!(
            "binding reservation {} sompi; complete-check bond fee {} sompi; carrier fees are checked at submission",
            kaspa_consensus_core::palw_onboarding_v1::PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
            kaspa_consensus_core::palw_opv_bootstrap_v1::PALW_COMPLETE_CHECK_FEE_SOMPI_V1
        );
    }
    Ok(())
}
