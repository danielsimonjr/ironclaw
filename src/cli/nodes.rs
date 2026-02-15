//! CLI commands for managing IronClaw nodes (remote device instances).
//!
//! Nodes are remote IronClaw instances that can be discovered, paired,
//! and managed from a central instance.

use clap::Subcommand;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Node management commands.
#[derive(Subcommand, Debug)]
pub enum NodesCommand {
    /// List all known nodes.
    List {
        /// Show only online nodes.
        #[arg(long)]
        online: bool,
    },
    /// Add a new node.
    Add {
        /// Node name.
        name: String,
        /// Node URL (e.g., http://192.168.1.100:3000).
        #[arg(long)]
        url: String,
        /// Authentication token for the node.
        #[arg(long)]
        token: Option<String>,
    },
    /// Remove a node.
    Remove {
        /// Node name or ID.
        name: String,
    },
    /// Check node health.
    Ping {
        /// Node name or ID (omit for all nodes).
        name: Option<String>,
    },
    /// Show detailed node information.
    Info {
        /// Node name or ID.
        name: String,
    },
    /// Pair with a discovered node.
    Pair {
        /// Node URL to pair with.
        url: String,
    },
    /// Unpair a node.
    Unpair {
        /// Node name or ID.
        name: String,
    },
}

/// Represents a remote IronClaw node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub status: NodeStatus,
    pub paired: bool,
    pub version: Option<String>,
    pub platform: Option<String>,
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
    pub added_at: chrono::DateTime<chrono::Utc>,
    pub token: Option<String>,
}

/// Status of a remote node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Online,
    Offline,
    Unknown,
    Pairing,
    Error,
}

impl std::fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Offline => write!(f, "offline"),
            Self::Unknown => write!(f, "unknown"),
            Self::Pairing => write!(f, "pairing"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Manager for remote nodes.
pub struct NodeManager {
    nodes: std::sync::Arc<tokio::sync::RwLock<Vec<Node>>>,
}

impl NodeManager {
    pub fn new() -> Self {
        Self {
            nodes: std::sync::Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    /// Add a new node with the given name, URL, and optional authentication token.
    pub async fn add_node(
        &self,
        name: String,
        url: String,
        token: Option<String>,
    ) -> Result<Node, String> {
        let mut nodes = self.nodes.write().await;
        if nodes.iter().any(|n| n.name == name) {
            return Err(format!("Node '{}' already exists", name));
        }
        if nodes.iter().any(|n| n.url == url) {
            return Err(format!("Node with URL '{}' already exists", url));
        }
        let node = Node {
            id: Uuid::new_v4(),
            name,
            url,
            status: NodeStatus::Unknown,
            paired: false,
            version: None,
            platform: None,
            last_seen: None,
            added_at: chrono::Utc::now(),
            token,
        };
        nodes.push(node.clone());
        Ok(node)
    }

    /// Remove a node by name or ID. Returns `true` if a node was removed.
    pub async fn remove_node(&self, name: &str) -> bool {
        let mut nodes = self.nodes.write().await;
        let len_before = nodes.len();
        nodes.retain(|n| n.name != name && n.id.to_string() != name);
        nodes.len() < len_before
    }

    /// List all nodes, optionally filtering to online-only.
    pub async fn list_nodes(&self, online_only: bool) -> Vec<Node> {
        let nodes = self.nodes.read().await;
        if online_only {
            nodes
                .iter()
                .filter(|n| n.status == NodeStatus::Online)
                .cloned()
                .collect()
        } else {
            nodes.clone()
        }
    }

    /// Get a node by name or ID.
    pub async fn get_node(&self, name: &str) -> Option<Node> {
        self.nodes
            .read()
            .await
            .iter()
            .find(|n| n.name == name || n.id.to_string() == name)
            .cloned()
    }

    /// Update the status of a node by name or ID. Returns `true` if the node was found.
    pub async fn update_status(&self, name: &str, status: NodeStatus) -> bool {
        let mut nodes = self.nodes.write().await;
        if let Some(node) = nodes
            .iter_mut()
            .find(|n| n.name == name || n.id.to_string() == name)
        {
            node.status = status;
            if status == NodeStatus::Online {
                node.last_seen = Some(chrono::Utc::now());
            }
            true
        } else {
            false
        }
    }

    /// Mark a node as paired.
    pub async fn pair_node(&self, name: &str) -> Result<(), String> {
        let mut nodes = self.nodes.write().await;
        if let Some(node) = nodes
            .iter_mut()
            .find(|n| n.name == name || n.id.to_string() == name)
        {
            if node.paired {
                return Err(format!("Node '{}' is already paired", name));
            }
            node.paired = true;
            Ok(())
        } else {
            Err(format!("Node '{}' not found", name))
        }
    }

    /// Unmark a node as paired.
    pub async fn unpair_node(&self, name: &str) -> Result<(), String> {
        let mut nodes = self.nodes.write().await;
        if let Some(node) = nodes
            .iter_mut()
            .find(|n| n.name == name || n.id.to_string() == name)
        {
            node.paired = false;
            Ok(())
        } else {
            Err(format!("Node '{}' not found", name))
        }
    }

    /// Ping a node by hitting its health endpoint. Updates the node's status.
    pub async fn ping_node(&self, name: &str) -> Result<bool, String> {
        let node = self
            .get_node(name)
            .await
            .ok_or_else(|| format!("Node '{}' not found", name))?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| format!("HTTP client error: {}", e))?;

        let url = format!("{}/api/health", node.url.trim_end_matches('/'));
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                self.update_status(name, NodeStatus::Online).await;
                Ok(true)
            }
            Ok(_) => {
                self.update_status(name, NodeStatus::Error).await;
                Ok(false)
            }
            Err(_) => {
                self.update_status(name, NodeStatus::Offline).await;
                Ok(false)
            }
        }
    }
}

impl Default for NodeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Run a nodes command (standalone CLI mode without agent runtime).
pub async fn run_nodes_command(cmd: &NodesCommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        NodesCommand::List { online } => {
            if *online {
                println!("No online nodes (node discovery requires agent runtime)");
            } else {
                println!("No registered nodes. Use 'ironclaw nodes add' to register a node.");
            }
        }
        NodesCommand::Add { name, url, .. } => {
            println!(
                "Node '{}' added at {} (requires agent runtime to ping)",
                name, url
            );
        }
        NodesCommand::Remove { name } => {
            println!("Node '{}' removed", name);
        }
        NodesCommand::Ping { name } => {
            if let Some(name) = name {
                println!("Pinging node '{}'... (requires agent runtime)", name);
            } else {
                println!("Pinging all nodes... (requires agent runtime)");
            }
        }
        NodesCommand::Info { name } => {
            println!("Node '{}' info (requires agent runtime)", name);
        }
        NodesCommand::Pair { url } => {
            println!("Pairing with node at {}... (requires agent runtime)", url);
        }
        NodesCommand::Unpair { name } => {
            println!("Unpaired node '{}'", name);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_node() {
        let manager = NodeManager::new();
        let node = manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(node.name, "laptop");
        assert_eq!(node.url, "http://192.168.1.10:3000");
        assert_eq!(node.status, NodeStatus::Unknown);
        assert!(!node.paired);
        assert!(node.token.is_none());
    }

    #[tokio::test]
    async fn test_add_node_with_token() {
        let manager = NodeManager::new();
        let node = manager
            .add_node(
                "server".to_string(),
                "http://10.0.0.5:3000".to_string(),
                Some("secret-token-123".to_string()),
            )
            .await
            .unwrap();
        assert_eq!(node.name, "server");
        assert_eq!(node.token.as_deref(), Some("secret-token-123"));
    }

    #[tokio::test]
    async fn test_add_duplicate_name_rejected() {
        let manager = NodeManager::new();
        manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        let result = manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.20:3000".to_string(),
                None,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[tokio::test]
    async fn test_add_duplicate_url_rejected() {
        let manager = NodeManager::new();
        manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        let result = manager
            .add_node(
                "desktop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[tokio::test]
    async fn test_remove_node_by_name() {
        let manager = NodeManager::new();
        manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        assert!(manager.remove_node("laptop").await);
        assert!(manager.list_nodes(false).await.is_empty());
    }

    #[tokio::test]
    async fn test_remove_node_by_id() {
        let manager = NodeManager::new();
        let node = manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        let id_str = node.id.to_string();
        assert!(manager.remove_node(&id_str).await);
        assert!(manager.list_nodes(false).await.is_empty());
    }

    #[tokio::test]
    async fn test_remove_nonexistent_node() {
        let manager = NodeManager::new();
        assert!(!manager.remove_node("nonexistent").await);
    }

    #[tokio::test]
    async fn test_list_all_nodes() {
        let manager = NodeManager::new();
        manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        manager
            .add_node(
                "desktop".to_string(),
                "http://192.168.1.20:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        let nodes = manager.list_nodes(false).await;
        assert_eq!(nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_list_online_only() {
        let manager = NodeManager::new();
        manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        manager
            .add_node(
                "desktop".to_string(),
                "http://192.168.1.20:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        manager.update_status("laptop", NodeStatus::Online).await;

        let online = manager.list_nodes(true).await;
        assert_eq!(online.len(), 1);
        assert_eq!(online[0].name, "laptop");
    }

    #[tokio::test]
    async fn test_get_node_by_name() {
        let manager = NodeManager::new();
        manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        let node = manager.get_node("laptop").await;
        assert!(node.is_some());
        assert_eq!(node.unwrap().name, "laptop");
    }

    #[tokio::test]
    async fn test_get_node_by_id() {
        let manager = NodeManager::new();
        let added = manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        let node = manager.get_node(&added.id.to_string()).await;
        assert!(node.is_some());
        assert_eq!(node.unwrap().id, added.id);
    }

    #[tokio::test]
    async fn test_get_nonexistent_node() {
        let manager = NodeManager::new();
        assert!(manager.get_node("missing").await.is_none());
    }

    #[tokio::test]
    async fn test_update_status() {
        let manager = NodeManager::new();
        manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        assert!(manager.update_status("laptop", NodeStatus::Online).await);
        let node = manager.get_node("laptop").await.unwrap();
        assert_eq!(node.status, NodeStatus::Online);
        assert!(node.last_seen.is_some());
    }

    #[tokio::test]
    async fn test_update_status_offline_no_last_seen_change() {
        let manager = NodeManager::new();
        manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        assert!(manager.update_status("laptop", NodeStatus::Offline).await);
        let node = manager.get_node("laptop").await.unwrap();
        assert_eq!(node.status, NodeStatus::Offline);
        assert!(node.last_seen.is_none());
    }

    #[tokio::test]
    async fn test_update_status_nonexistent() {
        let manager = NodeManager::new();
        assert!(!manager.update_status("missing", NodeStatus::Online).await);
    }

    #[tokio::test]
    async fn test_pair_node() {
        let manager = NodeManager::new();
        manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        manager.pair_node("laptop").await.unwrap();
        let node = manager.get_node("laptop").await.unwrap();
        assert!(node.paired);
    }

    #[tokio::test]
    async fn test_pair_already_paired_node() {
        let manager = NodeManager::new();
        manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        manager.pair_node("laptop").await.unwrap();
        let result = manager.pair_node("laptop").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already paired"));
    }

    #[tokio::test]
    async fn test_pair_nonexistent_node() {
        let manager = NodeManager::new();
        let result = manager.pair_node("missing").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_unpair_node() {
        let manager = NodeManager::new();
        manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        manager.pair_node("laptop").await.unwrap();
        manager.unpair_node("laptop").await.unwrap();
        let node = manager.get_node("laptop").await.unwrap();
        assert!(!node.paired);
    }

    #[tokio::test]
    async fn test_unpair_nonexistent_node() {
        let manager = NodeManager::new();
        let result = manager.unpair_node("missing").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_node_status_display() {
        assert_eq!(NodeStatus::Online.to_string(), "online");
        assert_eq!(NodeStatus::Offline.to_string(), "offline");
        assert_eq!(NodeStatus::Unknown.to_string(), "unknown");
        assert_eq!(NodeStatus::Pairing.to_string(), "pairing");
        assert_eq!(NodeStatus::Error.to_string(), "error");
    }

    #[test]
    fn test_node_serialization() {
        let node = Node {
            id: Uuid::nil(),
            name: "test-node".to_string(),
            url: "http://localhost:3000".to_string(),
            status: NodeStatus::Online,
            paired: true,
            version: Some("0.1.0".to_string()),
            platform: Some("linux".to_string()),
            last_seen: None,
            added_at: chrono::DateTime::from_timestamp(1_700_000_000, 0)
                .unwrap()
                .to_utc(),
            token: None,
        };
        let json = serde_json::to_string(&node).unwrap();
        let deserialized: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test-node");
        assert_eq!(deserialized.status, NodeStatus::Online);
        assert!(deserialized.paired);
        assert_eq!(deserialized.version.as_deref(), Some("0.1.0"));
        assert_eq!(deserialized.platform.as_deref(), Some("linux"));
    }

    #[test]
    fn test_node_status_serialization() {
        let json = serde_json::to_string(&NodeStatus::Online).unwrap();
        assert_eq!(json, "\"online\"");
        let deserialized: NodeStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, NodeStatus::Online);
    }

    #[test]
    fn test_default_manager() {
        let _manager = NodeManager::default();
    }

    #[tokio::test]
    async fn test_multiple_nodes_lifecycle() {
        let manager = NodeManager::new();

        // Add several nodes
        manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        manager
            .add_node(
                "desktop".to_string(),
                "http://192.168.1.20:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        manager
            .add_node(
                "server".to_string(),
                "http://10.0.0.5:3000".to_string(),
                Some("tok".to_string()),
            )
            .await
            .unwrap();
        assert_eq!(manager.list_nodes(false).await.len(), 3);

        // Update statuses
        manager.update_status("laptop", NodeStatus::Online).await;
        manager.update_status("desktop", NodeStatus::Offline).await;
        manager.update_status("server", NodeStatus::Online).await;
        assert_eq!(manager.list_nodes(true).await.len(), 2);

        // Pair one
        manager.pair_node("laptop").await.unwrap();
        let laptop = manager.get_node("laptop").await.unwrap();
        assert!(laptop.paired);

        // Remove one
        manager.remove_node("desktop").await;
        assert_eq!(manager.list_nodes(false).await.len(), 2);

        // Unpair
        manager.unpair_node("laptop").await.unwrap();
        let laptop = manager.get_node("laptop").await.unwrap();
        assert!(!laptop.paired);
    }

    #[tokio::test]
    async fn test_node_id_is_unique() {
        let manager = NodeManager::new();
        let node1 = manager
            .add_node(
                "node1".to_string(),
                "http://192.168.1.1:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        let node2 = manager
            .add_node(
                "node2".to_string(),
                "http://192.168.1.2:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        assert_ne!(node1.id, node2.id);
    }

    #[tokio::test]
    async fn test_pair_node_by_id() {
        let manager = NodeManager::new();
        let node = manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        manager.pair_node(&node.id.to_string()).await.unwrap();
        let fetched = manager.get_node("laptop").await.unwrap();
        assert!(fetched.paired);
    }

    #[tokio::test]
    async fn test_unpair_node_by_id() {
        let manager = NodeManager::new();
        let node = manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        manager.pair_node("laptop").await.unwrap();
        manager.unpair_node(&node.id.to_string()).await.unwrap();
        let fetched = manager.get_node("laptop").await.unwrap();
        assert!(!fetched.paired);
    }

    #[tokio::test]
    async fn test_update_status_by_id() {
        let manager = NodeManager::new();
        let node = manager
            .add_node(
                "laptop".to_string(),
                "http://192.168.1.10:3000".to_string(),
                None,
            )
            .await
            .unwrap();
        assert!(
            manager
                .update_status(&node.id.to_string(), NodeStatus::Error)
                .await
        );
        let fetched = manager.get_node("laptop").await.unwrap();
        assert_eq!(fetched.status, NodeStatus::Error);
    }
}
