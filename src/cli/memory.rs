//! Memory/workspace CLI commands.
//!
//! Exposes the workspace system for direct CLI use without starting the agent.

use std::io::Read;
use std::sync::Arc;

use clap::Subcommand;

use crate::workspace::{ConnectionType, EmbeddingProvider, ProfileType, SearchConfig, Workspace};

/// Run a memory command using the Database trait (works with any backend).
pub async fn run_memory_command_with_db(
    cmd: MemoryCommand,
    db: std::sync::Arc<dyn crate::db::Database>,
    embeddings: Option<Arc<dyn EmbeddingProvider>>,
) -> anyhow::Result<()> {
    let mut workspace = Workspace::new_with_db("default", db);
    if let Some(emb) = embeddings {
        workspace = workspace.with_embeddings(emb);
    }

    match cmd {
        MemoryCommand::Search { query, limit } => search(&workspace, &query, limit).await,
        MemoryCommand::Read { path } => read(&workspace, &path).await,
        MemoryCommand::Write {
            path,
            content,
            append,
        } => write(&workspace, &path, content, append).await,
        MemoryCommand::Tree { path, depth } => tree(&workspace, &path, depth).await,
        MemoryCommand::Status => status(&workspace).await,
        MemoryCommand::Spaces { action } => spaces(&workspace, action).await,
        MemoryCommand::Profile { action } => profile(&workspace, action).await,
        MemoryCommand::Connect { action } => connect(&workspace, action).await,
    }
}

#[derive(Subcommand, Debug, Clone)]
pub enum MemoryCommand {
    /// Search workspace memory (hybrid full-text + semantic)
    Search {
        /// Search query
        query: String,

        /// Maximum number of results
        #[arg(short, long, default_value = "5")]
        limit: usize,
    },

    /// Read a file from the workspace
    Read {
        /// File path (e.g., "MEMORY.md", "daily/2024-01-15.md")
        path: String,
    },

    /// Write content to a workspace file
    Write {
        /// File path (e.g., "notes/idea.md")
        path: String,

        /// Content to write (omit to read from stdin)
        content: Option<String>,

        /// Append instead of overwrite
        #[arg(short, long)]
        append: bool,
    },

    /// Show workspace directory tree
    Tree {
        /// Root path to start from
        #[arg(default_value = "")]
        path: String,

        /// Maximum depth to traverse
        #[arg(short, long, default_value = "3")]
        depth: usize,
    },

    /// Show workspace status (document count, index health)
    Status,

    /// Manage memory spaces (named collections)
    Spaces {
        #[command(subcommand)]
        action: SpaceAction,
    },

    /// Manage user profile facts
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },

    /// Manage memory connections (knowledge graph)
    Connect {
        #[command(subcommand)]
        action: ConnectAction,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum SpaceAction {
    /// Create a new space
    Create {
        /// Space name
        name: String,
        /// Space description
        #[arg(short, long, default_value = "")]
        description: String,
    },
    /// List all spaces
    List,
    /// Add a document to a space
    Add {
        /// Space name
        name: String,
        /// Document path
        path: String,
    },
    /// Remove a document from a space
    Remove {
        /// Space name
        name: String,
        /// Document path
        path: String,
    },
    /// List documents in a space
    Contents {
        /// Space name
        name: String,
    },
    /// Delete a space
    Delete {
        /// Space name
        name: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ProfileAction {
    /// Set a profile fact
    Set {
        /// Fact key (e.g., "name", "location")
        key: String,
        /// Fact value
        value: String,
        /// Profile type: static or dynamic
        #[arg(short = 't', long, default_value = "static")]
        profile_type: String,
    },
    /// Show all profile facts
    Get,
    /// Delete a profile fact
    Delete {
        /// Fact key to delete
        key: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConnectAction {
    /// Create a connection between documents
    Create {
        /// Source document path
        source: String,
        /// Target document path
        target: String,
        /// Connection type: updates, extends, derives
        #[arg(short = 't', long, default_value = "extends")]
        connection_type: String,
    },
    /// List connections for a document
    List {
        /// Document path
        path: String,
    },
}

/// Run a memory command (PostgreSQL backend).
#[cfg(feature = "postgres")]
pub async fn run_memory_command(
    cmd: MemoryCommand,
    pool: deadpool_postgres::Pool,
    embeddings: Option<Arc<dyn EmbeddingProvider>>,
) -> anyhow::Result<()> {
    let mut workspace = Workspace::new("default", pool);
    if let Some(emb) = embeddings {
        workspace = workspace.with_embeddings(emb);
    }

    match cmd {
        MemoryCommand::Search { query, limit } => search(&workspace, &query, limit).await,
        MemoryCommand::Read { path } => read(&workspace, &path).await,
        MemoryCommand::Write {
            path,
            content,
            append,
        } => write(&workspace, &path, content, append).await,
        MemoryCommand::Tree { path, depth } => tree(&workspace, &path, depth).await,
        MemoryCommand::Status => status(&workspace).await,
        MemoryCommand::Spaces { action } => spaces(&workspace, action).await,
        MemoryCommand::Profile { action } => profile(&workspace, action).await,
        MemoryCommand::Connect { action } => connect(&workspace, action).await,
    }
}

async fn search(workspace: &Workspace, query: &str, limit: usize) -> anyhow::Result<()> {
    let config = SearchConfig::default().with_limit(limit.min(50));
    let results = workspace.search_with_config(query, config).await?;

    if results.is_empty() {
        println!("No results found for: {}", query);
        return Ok(());
    }

    println!("Found {} result(s) for \"{}\":\n", results.len(), query);

    for (i, result) in results.iter().enumerate() {
        let score_bar = score_indicator(result.score);
        println!("{}. [{}] (score: {:.3})", i + 1, score_bar, result.score);

        // Show a content preview (first 200 chars)
        let preview = truncate_content(&result.content, 200);
        for line in preview.lines() {
            println!("   {}", line);
        }
        println!();
    }

    Ok(())
}

async fn read(workspace: &Workspace, path: &str) -> anyhow::Result<()> {
    match workspace.read(path).await {
        Ok(doc) => {
            println!("{}", doc.content);
        }
        Err(crate::error::WorkspaceError::DocumentNotFound { .. }) => {
            anyhow::bail!("File not found: {}", path);
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

async fn write(
    workspace: &Workspace,
    path: &str,
    content: Option<String>,
    append: bool,
) -> anyhow::Result<()> {
    let content = match content {
        Some(c) => c,
        None => {
            // Read from stdin
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf
        }
    };

    if append {
        workspace.append(path, &content).await?;
        println!("Appended to {}", path);
    } else {
        workspace.write(path, &content).await?;
        println!("Wrote to {}", path);
    }

    Ok(())
}

async fn tree(workspace: &Workspace, path: &str, max_depth: usize) -> anyhow::Result<()> {
    let root = if path.is_empty() { "." } else { path };
    println!("{}/", root);
    print_tree(workspace, path, "", max_depth, 0).await?;
    Ok(())
}

async fn print_tree(
    workspace: &Workspace,
    path: &str,
    prefix: &str,
    max_depth: usize,
    current_depth: usize,
) -> anyhow::Result<()> {
    if current_depth >= max_depth {
        return Ok(());
    }

    let entries = workspace.list(path).await?;
    let count = entries.len();

    for (i, entry) in entries.iter().enumerate() {
        let is_last = i == count - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last { "    " } else { "│   " };

        if entry.is_directory {
            println!("{}{}{}/", prefix, connector, entry.name());
            Box::pin(print_tree(
                workspace,
                &entry.path,
                &format!("{}{}", prefix, child_prefix),
                max_depth,
                current_depth + 1,
            ))
            .await?;
        } else {
            println!("{}{}{}", prefix, connector, entry.name());
        }
    }

    Ok(())
}

async fn status(workspace: &Workspace) -> anyhow::Result<()> {
    let all_paths = workspace.list_all().await?;
    let file_count = all_paths.len();

    // Count directories by collecting unique parent paths
    let mut dirs: std::collections::HashSet<String> = std::collections::HashSet::new();
    for path in &all_paths {
        if let Some(parent) = path.rsplit_once('/') {
            dirs.insert(parent.0.to_string());
        }
    }

    println!("Workspace Status");
    println!("  User:        {}", workspace.user_id());
    println!("  Files:       {}", file_count);
    println!("  Directories: {}", dirs.len());

    // Check key files
    let key_files = [
        "MEMORY.md",
        "HEARTBEAT.md",
        "IDENTITY.md",
        "SOUL.md",
        "AGENTS.md",
        "USER.md",
    ];
    println!("\n  Identity files:");
    for path in &key_files {
        let exists = workspace.exists(path).await.unwrap_or(false);
        let marker = if exists { "+" } else { "-" };
        println!("    [{}] {}", marker, path);
    }

    Ok(())
}

async fn spaces(workspace: &Workspace, action: SpaceAction) -> anyhow::Result<()> {
    match action {
        SpaceAction::Create { name, description } => {
            workspace.create_space(&name, &description).await?;
            println!("Created space: {}", name);
        }
        SpaceAction::List => {
            let spaces = workspace.list_spaces().await?;
            if spaces.is_empty() {
                println!("No spaces found.");
            } else {
                println!("{} space(s):\n", spaces.len());
                for s in &spaces {
                    let desc = if s.description.is_empty() {
                        "(no description)".to_string()
                    } else {
                        s.description.clone()
                    };
                    println!("  {} - {}", s.name, desc);
                }
            }
        }
        SpaceAction::Add { name, path } => {
            workspace.add_to_space(&name, &path).await?;
            println!("Added {} to space {}", path, name);
        }
        SpaceAction::Remove { name, path } => {
            workspace.remove_from_space(&name, &path).await?;
            println!("Removed {} from space {}", path, name);
        }
        SpaceAction::Contents { name } => {
            let docs = workspace.list_space_documents(&name).await?;
            if docs.is_empty() {
                println!("Space '{}' is empty.", name);
            } else {
                println!("Space '{}' ({} documents):\n", name, docs.len());
                for d in &docs {
                    println!("  {} ({} words)", d.path, d.word_count());
                }
            }
        }
        SpaceAction::Delete { name } => {
            workspace.delete_space(&name).await?;
            println!("Deleted space: {}", name);
        }
    }
    Ok(())
}

async fn profile(workspace: &Workspace, action: ProfileAction) -> anyhow::Result<()> {
    match action {
        ProfileAction::Set {
            key,
            value,
            profile_type,
        } => {
            let pt = if profile_type == "dynamic" {
                ProfileType::Dynamic
            } else {
                ProfileType::Static
            };
            workspace.set_profile_fact(pt, &key, &value, "cli").await?;
            println!("Set profile fact: {} = {}", key, value);
        }
        ProfileAction::Get => {
            let facts = workspace.get_profile().await?;
            if facts.is_empty() {
                println!("No profile facts found.");
            } else {
                println!("{} profile fact(s):\n", facts.len());
                for f in &facts {
                    println!(
                        "  [{}] {} = {} (confidence: {:.0}%, source: {})",
                        f.profile_type,
                        f.key,
                        f.value,
                        f.confidence * 100.0,
                        f.source
                    );
                }
            }
        }
        ProfileAction::Delete { key } => {
            workspace.delete_profile_fact(&key).await?;
            println!("Deleted profile fact: {}", key);
        }
    }
    Ok(())
}

async fn connect(workspace: &Workspace, action: ConnectAction) -> anyhow::Result<()> {
    match action {
        ConnectAction::Create {
            source,
            target,
            connection_type,
        } => {
            let ct = ConnectionType::from_str_loose(&connection_type)
                .ok_or_else(|| anyhow::anyhow!("Invalid connection type: {}", connection_type))?;
            let conn = workspace.connect(&source, &target, ct).await?;
            println!(
                "Created connection: {} --[{}]--> {} (id: {})",
                source, connection_type, target, conn.id
            );
        }
        ConnectAction::List { path } => {
            let doc = workspace.read(&path).await?;
            let connections = workspace.get_connections(doc.id).await?;
            if connections.is_empty() {
                println!("No connections for: {}", path);
            } else {
                println!("{} connection(s) for '{}':\n", connections.len(), path);
                for c in &connections {
                    let direction = if c.source_id == doc.id {
                        format!("--> {}", c.target_id)
                    } else {
                        format!("<-- {}", c.source_id)
                    };
                    println!(
                        "  [{}] {} (strength: {:.2})",
                        c.connection_type, direction, c.strength
                    );
                }
            }
        }
    }
    Ok(())
}

fn truncate_content(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

fn score_indicator(score: f32) -> &'static str {
    if score > 0.8_f32 {
        "=====>"
    } else if score > 0.5_f32 {
        "====>"
    } else if score > 0.3_f32 {
        "===>"
    } else if score > 0.1_f32 {
        "==>"
    } else {
        "=>"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_indicator() {
        assert_eq!(score_indicator(0.9_f32), "=====>");
        assert_eq!(score_indicator(0.6_f32), "====>");
        assert_eq!(score_indicator(0.4_f32), "===>");
        assert_eq!(score_indicator(0.2_f32), "==>");
        assert_eq!(score_indicator(0.05_f32), "=>");
    }

    #[test]
    fn test_truncate_content() {
        assert_eq!(truncate_content("hello", 10), "hello");
        assert_eq!(truncate_content("hello world", 5), "hello...");
    }
}
