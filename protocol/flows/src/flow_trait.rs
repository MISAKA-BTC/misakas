use kaspa_core::{debug, warn};
use kaspa_p2p_lib::{Router, common::ProtocolError};
use kaspa_utils::any::type_name_short;
use std::sync::Arc;

#[async_trait::async_trait]
pub trait Flow
where
    Self: 'static + Send + Sync,
{
    fn name(&self) -> &'static str {
        type_name_short::<Self>()
    }

    fn router(&self) -> Option<Arc<Router>>;

    async fn start(&mut self) -> Result<(), ProtocolError>;

    fn launch(mut self: Box<Self>) {
        tokio::spawn(async move {
            let res = self.start().await;
            if let Err(err) = res
                && let Some(router) = self.router()
            {
                router.try_sending_reject_message(&err).await;
                // Always tear down the connection (any flow error closes the whole peer router).
                let _ = router.close().await;
                // A ConnectionClosed error is a teardown SYMPTOM, not a root cause: the peer — or
                // another flow on this same connection (e.g. the IBD flow ending and dropping the
                // relay job channel, which surfaces in HandleRelayInvsFlow as ConnectionClosed) —
                // closed the connection first, and this flow merely observed it on its next
                // send/recv. Logging that as "flow error ... disconnecting" makes a secondary
                // symptom look like the fault. So route it to debug; the flow that hit the REAL
                // error logs that at warn (e.g. IbdFlow's "IBD with peer X completed with
                // error: ..."). Genuine protocol faults still warn here.
                if err.is_connection_closed_error() {
                    debug!("{} flow ended: {} (peer {} connection already closing)", self.name(), err, router);
                } else {
                    warn!("{} flow error: {}, disconnecting from peer {}.", self.name(), err, router);
                }
            }
        });
    }
}
