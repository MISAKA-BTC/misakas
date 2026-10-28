use super::rpc_core_mock::RpcCoreMock;
use crate::{adaptor::Adaptor, manager::Manager};
use kaspa_core::info;
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::scope::{NewBlockTemplateScope, Scope};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_utils::networking::{ContextualNetAddress, NetAddress};
use std::sync::Arc;

#[tokio::test]
async fn test_client_server_sanity_check() {
    kaspa_core::log::try_init_logger("info, kaspa_grpc_core=trace, kaspa_grpc_server=trace, kaspa_grpc_client=trace");

    // Create and start a fake core service
    let rpc_core_service = Arc::new(RpcCoreMock::new());
    rpc_core_service.start();

    // Create and start the server
    let server = create_server(rpc_core_service.clone());
    assert!(!server.has_connections(), "server should have no client when just started");

    let client = create_client(server.serve_address()).await;
    assert_connections_eventually(|| server.active_connections().len(), 1, "the client failed to connect to the server").await;
    assert!(client.handle_message_id() && client.handle_stop_notify(), "the client failed to collect server features");

    // Stop the fake service
    rpc_core_service.join().await;

    // Stop the server
    assert!(server.stop().await.is_ok(), "error stopping the server");

    assert!(client.disconnect().await.is_ok(), "client failed to disconnect");
    drop(client);

    drop(server);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}

#[tokio::test]
async fn test_client_server_connections() {
    enum ClosingEnd {
        Client,
        Server,
    }

    struct Test {
        name: &'static str,
        ends: Vec<ClosingEnd>,
        terminate_clients: bool,
    }

    impl Test {
        async fn execute(&self) {
            info!("=================================================================================");
            info!("{}", self.name);

            // Create and start a fake core service
            let rpc_core_service = Arc::new(RpcCoreMock::new());
            rpc_core_service.start();

            // Create and start the server
            let server = create_server(rpc_core_service.clone());
            assert!(!server.has_connections(), "server should have no client when just started");

            // Create clients
            let mut clients = Vec::with_capacity(self.ends.len());
            for _ in 0..self.ends.len() {
                clients.push(create_client(server.serve_address()).await);
            }
            assert_connections_eventually(
                || server.active_connections().len(),
                self.ends.len(),
                "one or more clients failed to connect to the server",
            )
            .await;

            // Disconnect clients
            let mut clients_left: usize = self.ends.len();
            for (i, closing) in self.ends.iter().enumerate() {
                match *closing {
                    ClosingEnd::Client => {
                        assert!(clients[i].disconnect().await.is_ok(), "client {} failed to disconnect", i);
                        clients_left -= 1;
                    }
                    ClosingEnd::Server => {}
                }
            }
            if clients_left < self.ends.len() {
                assert_connections_eventually(
                    || server.active_connections().len(),
                    clients_left,
                    &format!("server should have {} client(s) left connected", clients_left),
                )
                .await;
            }

            // Terminate connections server-side
            if self.terminate_clients {
                server.terminate_all_connections().await;
                assert_connections_eventually(|| server.active_connections().len(), 0, "server should have no more clients").await;
                for (i, client) in clients.iter().enumerate() {
                    assert!(!client.is_connected(), "server failed to disconnect client {}", i);
                }
                assert!(!server.has_connections(), "server should have no more clients");
            }

            // Stop the fake service
            rpc_core_service.join().await;

            // Stop the server
            assert!(server.stop().await.is_ok(), "error stopping the server");

            // Check final state
            if !self.terminate_clients {
                assert_connections_eventually(|| server.active_connections().len(), 0, "server should have no more clients").await;
                for (i, client) in clients.iter().enumerate() {
                    assert!(!client.is_connected(), "server failed to disconnect client {}", i);
                }
                assert!(!server.has_connections(), "server should have no more clients");
            }

            // Terminate the server
            drop(server);
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }
    }

    let tests = vec![
        Test {
            name: "3 clients connecting and disconnecting themselves",
            ends: vec![ClosingEnd::Client, ClosingEnd::Client, ClosingEnd::Client],
            terminate_clients: false,
        },
        Test {
            name: "3 clients connecting and server disconnecting them",
            ends: vec![ClosingEnd::Server, ClosingEnd::Server, ClosingEnd::Server],
            terminate_clients: true,
        },
        Test {
            name: "3 clients connecting, 1 disconnecting itself, server shutting down",
            ends: vec![ClosingEnd::Client, ClosingEnd::Server, ClosingEnd::Client],
            terminate_clients: false,
        },
    ];

    kaspa_core::log::try_init_logger("info, kaspa_grpc_core=trace, kaspa_grpc_server=trace, kaspa_grpc_client=trace");
    for test in tests {
        test.execute().await;
    }

    // Wait for server termination (just for logging properly)
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}

#[tokio::test]
async fn test_client_server_notifications() {
    kaspa_core::log::try_init_logger("info, kaspa_grpc_core=trace, kaspa_grpc_server=trace, kaspa_grpc_client=trace");

    // Create and start a fake core service
    let rpc_core_service = Arc::new(RpcCoreMock::new());
    rpc_core_service.start();

    // Create and start the server
    let server = create_server(rpc_core_service.clone());

    // Connect 2 clients
    let client1 = create_client(server.serve_address()).await;
    let client2 = create_client(server.serve_address()).await;

    // Subscribe both clients to NewBlockTemplate notifications
    assert!(client1.start_notify(0, Scope::NewBlockTemplate(NewBlockTemplateScope::default())).await.is_ok());
    assert!(client2.start_notify(0, Scope::NewBlockTemplate(NewBlockTemplateScope::default())).await.is_ok());

    // Let core send a notification
    assert!(rpc_core_service.notify_new_block_template().is_ok());
    rpc_core_service.notify_complete().await;

    // Make sure each client receives the notification
    assert!(client1.notification_channel_receiver().recv().await.is_ok());
    assert!(client2.notification_channel_receiver().recv().await.is_ok());

    // Disconnect the first client but keep the other
    assert!(client1.disconnect().await.is_ok(), "client failed to disconnect");
    drop(client1);

    // Stop the fake service
    rpc_core_service.join().await;

    // Stop the server
    assert!(server.stop().await.is_ok(), "error stopping the server");
    drop(server);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}

fn create_server(core_service: Arc<RpcCoreMock>) -> Arc<Adaptor> {
    let manager = Manager::new(128);
    Adaptor::server(
        get_free_net_address(),
        1,
        manager,
        core_service.clone(),
        core_service.core_notifier(),
        core_service.subscription_context(),
        3,
        Default::default(),
    )
}

async fn create_client(server_address: NetAddress) -> GrpcClient {
    let server_url = format!("grpc://localhost:{}", server_address.port);
    // Under heavy parallel test load the very first connect can transiently fail (the server's
    // accept loop hasn't drained the listen backlog yet). Retry a few times before giving up so the
    // test is deterministic instead of flaking on a single tight attempt.
    let mut last_err = None;
    for _ in 0..20 {
        match GrpcClient::connect(server_url.clone()).await {
            Ok(client) => return client,
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
    }
    panic!("failed to connect grpc client to {server_url}: {:?}", last_err.unwrap());
}

/// Polls `f` until it returns the `expected` connection count or a ~2s deadline elapses, then
/// asserts. Connection-state changes (connect / disconnect / server-side termination) propagate
/// asynchronously across the client task, the server manager, and tokio's scheduler; a single fixed
/// sleep is racy under parallel load, whereas polling converges as soon as the state settles.
async fn assert_connections_eventually(mut f: impl FnMut() -> usize, expected: usize, msg: &str) {
    for _ in 0..200 {
        if f() == expected {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(f(), expected, "{msg}");
}

fn get_free_net_address() -> NetAddress {
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    drop(socket);
    ContextualNetAddress::unspecified().normalize(port)
}
