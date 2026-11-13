pub mod clients {
    pub fn with_queue() -> tiri_core::shared::client::ClientRef {
        crate::initialize();
        tiri_core::server_client::queue_test_clients::with_queue()
    }
}
