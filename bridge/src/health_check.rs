/// Audit #4: bound concurrent health-probe handlers so a connection flood cannot exhaust
/// tasks/sockets on a public health endpoint. Generous for real monitors; over-cap
/// connections are dropped (closed) immediately rather than queued.
const MAX_HEALTH_CONNS: usize = 64;

pub(crate) fn spawn_health_check_server(health_port: String) {
    tokio::spawn(async move {
        use std::sync::Arc;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        use tokio::sync::Semaphore;

        if let Ok(listener) = TcpListener::bind(&health_port).await {
            tracing::info!("Health check server started on {}", health_port);
            let sem = Arc::new(Semaphore::new(MAX_HEALTH_CONNS));
            loop {
                if let Ok((mut stream, _)) = listener.accept().await {
                    // Bound concurrency: drop the connection immediately when at capacity.
                    let Ok(permit) = sem.clone().try_acquire_owned() else {
                        continue;
                    };
                    // Handle each probe on its own task with a read timeout, so a single client that
                    // connects and never sends cannot wedge the accept loop.
                    tokio::spawn(async move {
                        let _permit = permit; // released when the handler ends
                        let mut buffer = [0; 1024];
                        let read = tokio::time::timeout(std::time::Duration::from_secs(10), stream.read(&mut buffer)).await;
                        if matches!(read, Ok(Ok(_))) {
                            let response = "HTTP/1.1 200 OK\r\n\r\n";
                            let _ = stream.write_all(response.as_bytes()).await;
                        }
                    });
                }
            }
        }
    });
}
