//! Unified HTTP server for all webhook routes.
//!
//! Composes route fragments from HttpChannel, WASM channel router, etc.
//! into a single axum server. Channels define routes but never spawn servers.

use std::net::SocketAddr;

use axum::Router;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::error::ChannelError;

/// Configuration for the unified webhook server.
pub struct WebhookServerConfig {
    /// Address to bind the server to.
    pub addr: SocketAddr,
}

/// A single HTTP server that hosts all webhook routes.
///
/// Channels contribute route fragments via `add_routes()`, then a single
/// `start()` call binds the listener and spawns the server task.
pub struct WebhookServer {
    config: WebhookServerConfig,
    routes: Vec<Router>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl WebhookServer {
    /// Create a new webhook server with the given bind address.
    pub fn new(config: WebhookServerConfig) -> Self {
        Self {
            config,
            routes: Vec::new(),
            shutdown_tx: None,
            handle: None,
        }
    }

    /// Accumulate a route fragment. Each fragment should already have its
    /// state applied via `.with_state()`.
    pub fn add_routes(&mut self, router: Router) {
        self.routes.push(router);
    }

    /// Bind the listener, merge all route fragments, and spawn the server.
    pub async fn start(&mut self) -> Result<(), ChannelError> {
        let mut app = Router::new();
        for fragment in self.routes.drain(..) {
            app = app.merge(fragment);
        }

        let listener = tokio::net::TcpListener::bind(self.config.addr)
            .await
            .map_err(|e| ChannelError::StartupFailed {
                name: "webhook_server".to_string(),
                reason: format!("Failed to bind to {}: {}", self.config.addr, e),
            })?;

        tracing::info!("Webhook server listening on {}", self.config.addr);

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let handle = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                    tracing::info!("Webhook server shutting down");
                })
                .await
            {
                tracing::error!("Webhook server error: {}", e);
            }
        });

        self.handle = Some(handle);
        Ok(())
    }

    /// Signal graceful shutdown and wait for the server task to finish.
    pub async fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto_config() -> WebhookServerConfig {
        WebhookServerConfig {
            addr: "127.0.0.1:0".parse().unwrap(),
        }
    }

    #[test]
    fn new_creates_server_and_accepts_routes() {
        let mut server = WebhookServer::new(auto_config());
        // Should not panic — server is usable immediately after new().
        server.add_routes(Router::new());
    }

    #[test]
    fn add_routes_multiple_times() {
        let mut server = WebhookServer::new(auto_config());
        server.add_routes(Router::new());
        server.add_routes(Router::new());
        server.add_routes(Router::new());
        // Three fragments accumulated without error.
    }

    #[tokio::test]
    async fn start_and_shutdown_lifecycle() {
        let mut server = WebhookServer::new(auto_config());
        server.add_routes(Router::new());
        server.start().await.expect("server should start on port 0");
        assert!(server.handle.is_some());
        assert!(server.shutdown_tx.is_some());
        server.shutdown().await;
        assert!(server.handle.is_none());
        assert!(server.shutdown_tx.is_none());
    }

    #[tokio::test]
    async fn start_on_occupied_port_returns_error() {
        // Bind a port first so it's occupied.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap();
        let occupied_addr = listener.local_addr().unwrap();

        let config = WebhookServerConfig {
            addr: occupied_addr,
        };
        let mut server = WebhookServer::new(config);
        let result = server.start().await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ChannelError::StartupFailed { name, reason } => {
                assert_eq!(name, "webhook_server");
                assert!(reason.contains("Failed to bind"));
            }
            other => panic!("expected StartupFailed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn shutdown_when_not_started_is_noop() {
        let mut server = WebhookServer::new(auto_config());
        // Should not panic.
        server.shutdown().await;
    }
}
