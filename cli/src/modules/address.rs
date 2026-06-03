use crate::imports::*;

#[derive(Default, Handler)]
#[help("Show or generate a new address for the current wallet account")]
pub struct Address;

impl Address {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        if argv.is_empty() {
            let address = ctx.account().await?.receive_address()?.to_string();
            tprintln!(ctx, "\n{address}\n");
        } else {
            let op = argv.first().unwrap();
            match op.as_str() {
                // kaspa-pq PQ-only (ADR-0019 §14): generating a *new* derived address
                // is a classical (secp256k1, `as_derivation_capable`) operation. A
                // single-key ML-DSA account has one fixed address (receive == change),
                // so there is no new address to generate.
                #[cfg(feature = "legacy-secp256k1")]
                "new" => {
                    let account = ctx.wallet().account()?.as_derivation_capable()?;
                    let ident = account.name_with_id();
                    let new_address = account.new_receive_address().await?;
                    tprintln!(ctx, "Generating new address for account {}", style(ident).cyan());
                    tprintln!(ctx, "{}", style(new_address).blue());
                }
                #[cfg(not(feature = "legacy-secp256k1"))]
                "new" => {
                    tprintln!(ctx, "ML-DSA accounts use a single fixed address; there is no new address to generate");
                }
                v => {
                    tprintln!(ctx, "unknown command: '{v}'\r\n");
                    return self.display_help(ctx, argv).await;
                }
            }
        }

        Ok(())
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        ctx.term().help(&[("address [new]", "Show current or generate a new account address")], None)?;

        Ok(())
    }
}
