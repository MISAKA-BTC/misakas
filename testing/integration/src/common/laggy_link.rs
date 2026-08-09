//! A TCP link that takes its time, so p2p tests stop running at loopback speed.
//!
//! Every end-to-end test so far connected two daemons over localhost, where a message arrives
//! before the sender has finished thinking about it. That hides exactly the class of defect this
//! work has been full of: a reservation redeemed before the handoff is broadcast, a summary reply
//! racing the request that provoked it, a flow reaching a select at the one moment nothing is
//! queued. Those either cannot happen at loopback latency or happen so rarely that one green run
//! means nothing.
//!
//! So the daemons are connected through here instead. Each direction is forwarded with a delay
//! drawn fresh per chunk, which both slows delivery and lets the two directions reorder relative to
//! each other — the part a fixed delay would not reproduce.
//!
//! Deliberately in the test harness, not the node. Injecting latency into the product to test the
//! product is how you end up shipping the test harness.

use rand::{Rng, thread_rng};
use std::{ops::Range, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
};

/// Round-trip latency worth testing against: a WAN hop between two continents, not a LAN.
///
/// The upper end matters more than the average. A link that is always slow is just a slow link; a
/// link whose delay varies is what reorders arrivals and surfaces ordering assumptions.
pub const WAN_DELAY: Range<Duration> = Duration::from_millis(25)..Duration::from_millis(120);

/// A listening proxy. Connections to `local_port` are forwarded to the target with delay.
pub struct LaggyLink {
    pub local_port: u16,
    shutdown: Arc<Notify>,
}

impl LaggyLink {
    /// Start forwarding to `target_port` on localhost.
    pub async fn spawn(target_port: u16, delay: Range<Duration>) -> Self {
        // Port 0: let the OS choose, so repeated runs in one process cannot collide.
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind laggy link");
        let local_port = listener.local_addr().expect("laggy link addr").port();
        let shutdown = Arc::new(Notify::new());

        let stop = shutdown.clone();
        tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    r = listener.accept() => r,
                    _ = stop.notified() => return,
                };
                let Ok((inbound, _)) = accepted else { continue };
                let delay = delay.clone();
                tokio::spawn(async move {
                    // A failure to reach the target is a closed connection, which is what a real
                    // network does too — nothing to report.
                    let Ok(outbound) = TcpStream::connect(("127.0.0.1", target_port)).await else { return };
                    let (ri, wi) = inbound.into_split();
                    let (ro, wo) = outbound.into_split();
                    let a = tokio::spawn(pump(ri, wo, delay.clone()));
                    let b = tokio::spawn(pump(ro, wi, delay));
                    let _ = tokio::join!(a, b);
                });
            }
        });

        Self { local_port, shutdown }
    }
}

impl Drop for LaggyLink {
    fn drop(&mut self) {
        self.shutdown.notify_waiters();
    }
}

/// Forward one direction, pausing a fresh random interval before each chunk.
///
/// Per chunk rather than per connection: the delay has to keep applying for the whole transfer, or
/// a long IBD would pay it once and then run at loopback speed for the part that matters.
async fn pump<R, W>(mut from: R, mut to: W, delay: Range<Duration>)
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut buf = vec![0u8; 32 * 1024];
    loop {
        let n = match from.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let wait = { thread_rng().gen_range(delay.start..delay.end) };
        tokio::time::sleep(wait).await;
        if to.write_all(&buf[..n]).await.is_err() {
            break;
        }
    }
    let _ = to.shutdown().await;
}
