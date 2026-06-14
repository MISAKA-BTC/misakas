pub(crate) fn spawn_health_check_server(health_port: String) {
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        if let Ok(listener) = TcpListener::bind(&health_port).await {
            tracing::info!("Health check server started on {}", health_port);
            loop {
                if let Ok((mut stream, _)) = listener.accept().await {
                    // Handle each probe on its own task with a read timeout, so a single client that
                    // connects and never sends cannot wedge the accept loop.
                    tokio::spawn(async move {
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
