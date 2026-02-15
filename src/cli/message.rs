//! Message sending CLI commands.

use clap::Subcommand;

#[derive(Subcommand, Debug, Clone)]
pub enum MessageCommand {
    /// Send a message to a channel
    Send {
        /// Message content
        content: String,

        /// Target channel (e.g., "telegram", "slack", "webchat")
        #[arg(short, long, default_value = "default")]
        channel: String,

        /// Thread/conversation ID
        #[arg(short, long)]
        thread: Option<String>,

        /// Recipient (for DM channels)
        #[arg(short, long)]
        to: Option<String>,
    },
}

/// Run a message command.
pub async fn run_message_command(cmd: MessageCommand) -> anyhow::Result<()> {
    match cmd {
        MessageCommand::Send {
            content,
            channel,
            thread,
            to,
        } => send_message(&content, &channel, thread.as_deref(), to.as_deref()).await,
    }
}

async fn send_message(
    content: &str,
    channel: &str,
    thread: Option<&str>,
    to: Option<&str>,
) -> anyhow::Result<()> {
    // For the web gateway channel, we can use the HTTP API
    if channel == "webchat" || channel == "web" || channel == "default" {
        let gateway_port = std::env::var("GATEWAY_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(3000);

        let gateway_token = std::env::var("GATEWAY_AUTH_TOKEN").ok();

        let client = reqwest::Client::new();
        let mut request = client
            .post(format!("http://127.0.0.1:{}/api/send", gateway_port))
            .json(&serde_json::json!({
                "content": content,
                "thread_id": thread,
            }));

        if let Some(token) = gateway_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.send().await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to send message (is the gateway running on port {}?): {}",
                gateway_port,
                e
            )
        })?;

        if response.status().is_success() {
            println!("Message sent to {} channel.", channel);
            if let Ok(body) = response.json::<serde_json::Value>().await {
                if let Some(id) = body.get("message_id") {
                    println!("  Message ID: {}", id);
                }
            }
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to send message: {} {}", status, body);
        }
    } else {
        println!(
            "Sending to channel '{}' (recipient: {}):",
            channel,
            to.unwrap_or("default")
        );
        println!("  {}", content);
        println!("\nNote: Direct channel sending requires the agent to be running.");
        println!("Use the web gateway for programmatic message sending.");
    }

    Ok(())
}
