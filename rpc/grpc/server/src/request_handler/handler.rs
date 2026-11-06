use super::{
    handler_trait::Handler,
    interface::{DynKaspadMethod, Interface},
};
use crate::{
    connection::{Connection, IncomingRoute},
    connection_handler::ServerContext,
    error::{GrpcServerError, GrpcServerResult},
};
use kaspa_core::debug;
use kaspa_core::memory_budget::MemoryLease;
use kaspa_grpc_core::{
    ops::KaspadPayloadOps,
    protowire::{KaspadRequest, KaspadResponse},
};

pub struct RequestHandler {
    rpc_op: KaspadPayloadOps,
    incoming_route: IncomingRoute,
    server_ctx: ServerContext,
    method: DynKaspadMethod,
    connection: Connection,
}

impl RequestHandler {
    pub fn new(
        rpc_op: KaspadPayloadOps,
        incoming_route: IncomingRoute,
        server_context: ServerContext,
        interface: &Interface,
        connection: Connection,
    ) -> Self {
        let method = interface.get_method(&rpc_op);
        Self { rpc_op, incoming_route, server_ctx: server_context, method, connection }
    }

    pub async fn handle_request(&self, request: KaspadRequest) -> GrpcServerResult<(KaspadResponse, Option<MemoryLease>)> {
        let id = request.id;
        if self.rpc_op == KaspadPayloadOps::GetUtxosByAddresses {
            use kaspa_grpc_core::protowire::{GetUtxosByAddressesResponseMessage, kaspad_request::Payload};
            let Some(Payload::GetUtxosByAddressesRequest(ref wire_request)) = request.payload else {
                return Err(GrpcServerError::InvalidRequestPayload);
            };
            let retained = match kaspa_rpc_core::GetUtxosByAddressesRequest::try_from(wire_request) {
                Ok(request) => self.server_ctx.core_service.get_utxos_by_addresses_retained_call(None, request).await,
                Err(error) => Err(error),
            };
            let (wire_response, conversion) = match retained {
                Ok(reply) => {
                    let (response, retained, conversion) = reply.into_parts();
                    kaspa_utils_tower::transport::convert_preallocated(response, retained, conversion, |response| {
                        GetUtxosByAddressesResponseMessage::from(Ok(response))
                    })
                }
                Err(error) => (GetUtxosByAddressesResponseMessage::from(error), None),
            };
            let mut response: KaspadResponse = wire_response.into();
            response.id = id;
            return Ok((response, conversion));
        }
        if self.rpc_op == KaspadPayloadOps::GetBlocks {
            use kaspa_grpc_core::protowire::{GetBlocksResponseMessage, kaspad_request::Payload};
            let Some(Payload::GetBlocksRequest(ref wire_request)) = request.payload else {
                return Err(GrpcServerError::InvalidRequestPayload);
            };
            let retained = match kaspa_rpc_core::GetBlocksRequest::try_from(wire_request) {
                Ok(request) => self.server_ctx.core_service.get_blocks_retained_call(None, request).await,
                Err(error) => Err(error),
            };
            let (wire_response, conversion) = match retained {
                Ok(reply) => {
                    let (response, retained, conversion) = reply.into_parts();
                    // At present only projected Rust IDs are admitted. No
                    // protobuf admission is claimed when conversion is None.
                    kaspa_utils_tower::transport::convert_preallocated(response, retained, conversion, |response| {
                        GetBlocksResponseMessage::from(Ok(response))
                    })
                }
                Err(error) => (GetBlocksResponseMessage::from(error), None),
            };
            let mut response: KaspadResponse = wire_response.into();
            response.id = id;
            return Ok((response, conversion));
        }
        let mut response = self.method.call(self.server_ctx.clone(), self.connection.clone(), request).await?;
        response.id = id;
        Ok((response, None))
    }
}

#[async_trait::async_trait]
impl Handler for RequestHandler {
    async fn start(&mut self) {
        debug!("GRPC, Starting request handler {:?} for client {}", self.rpc_op, self.connection);
        while let Ok(request) = self.incoming_route.recv().await {
            let response = self.handle_request(request).await;
            match response {
                Ok((response, conversion)) => {
                    let sent = if let Some(lease) = conversion {
                        self.connection.enqueue_preallocated(response, lease).await
                    } else {
                        self.connection.enqueue(response).await
                    };
                    if sent.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    debug!("GRPC, Request handling error {} for client {}", e, self.connection);
                }
            }
        }
        debug!("GRPC, Exiting request handler {:?} for client {}", self.rpc_op, self.connection);
    }
}
