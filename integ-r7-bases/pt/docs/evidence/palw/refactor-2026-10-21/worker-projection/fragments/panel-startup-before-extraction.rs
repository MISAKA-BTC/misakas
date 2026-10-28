{
            // ADR-0084 Decision 2: the material when it fits the transport, the claim's answer
            // envelope when it does not — decided here, by the party holding the directory, so
            // the transport is never handed what it would refuse.
            // The service owns this gossip center through FlowContext. A strong callback owner
            // would retain the service, its model holdings and both databases after shutdown.
            let me = std::sync::Arc::downgrade(self);
            self.flow_context
                .palw_gossip()
                .set_material_resolver(std::sync::Arc::new(move |claim| me.upgrade()?.serve_material_or_answer(claim)));
            // The directory this node serves from, published to the wallet-facing RPC so a
            // submitter on this host stages where this panel reads (ADR-0084 Decision 5) — and
            // CREATED here, before it is named: a submitter refuses a directory that does not
            // exist on its host, and this one used to appear only with the first retained
            // material, so a fresh node named a directory nobody could stage into.
            if let Err(e) = std::fs::create_dir_all(&self.config.retention_dir) {
                warn!("[{PALW_PANEL}] cannot create the retention directory {}: {e}", self.config.retention_dir.display());
            }
            self.flow_context.palw_declare_retention_dir(self.config.retention_dir.clone());
        }