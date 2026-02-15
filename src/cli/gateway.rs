//! Gateway management CLI commands.
//!
//! Start, stop, and check status of the web gateway.

use std::path::PathBuf;

use clap::Subcommand;

#[derive(Subcommand, Debug, Clone)]
pub enum GatewayCommand {
    /// Start the web gateway
    Start {
        /// Port to listen on
        #[arg(short, long, default_value = "3000")]
        port: u16,

        /// Run in background (daemonize)
        #[arg(short, long)]
        daemon: bool,
    },

    /// Stop the running gateway
    Stop,

    /// Show gateway status
    Status,
}

/// Run a gateway command.
pub async fn run_gateway_command(cmd: GatewayCommand) -> anyhow::Result<()> {
    match cmd {
        GatewayCommand::Start { port, daemon } => start_gateway(port, daemon).await,
        GatewayCommand::Stop => stop_gateway().await,
        GatewayCommand::Status => gateway_status().await,
    }
}

async fn start_gateway(port: u16, daemon: bool) -> anyhow::Result<()> {
    let pid_file = pid_file_path();

    // Check if already running
    if let Some(pid) = read_pid(&pid_file) {
        if is_process_running(pid) {
            anyhow::bail!(
                "Gateway already running (PID: {}). Use 'ironclaw gateway stop' first.",
                pid
            );
        }
        // Stale PID file, remove it
        let _ = std::fs::remove_file(&pid_file);
    }

    if daemon {
        println!("Starting gateway on port {} in background...", port);
        // For daemon mode, we'd fork. In practice, use systemd or similar.
        // For now, just record the intent.
        println!("Note: Use systemd/launchd for proper daemonization.");
        println!("  systemd: ironclaw gateway start --port {}", port);
        println!("  Or run: ironclaw run --gateway-port {} &", port);
    } else {
        // Write PID file
        write_pid(&pid_file)?;
        println!("Gateway starting on port {}...", port);
        println!("PID file: {}", pid_file.display());
        println!("Use 'ironclaw gateway stop' to stop the gateway.");

        // The actual gateway startup happens in the main agent loop.
        // This command just validates and sets up the PID file.
        println!("\nTo start the full agent with gateway, run:");
        println!("  GATEWAY_ENABLED=true GATEWAY_PORT={} ironclaw run", port);
    }

    Ok(())
}

async fn stop_gateway() -> anyhow::Result<()> {
    let pid_file = pid_file_path();

    if let Some(pid) = read_pid(&pid_file) {
        if is_process_running(pid) {
            // Send SIGTERM
            #[cfg(unix)]
            {
                use std::process::Command;
                let status = Command::new("kill")
                    .arg("-TERM")
                    .arg(pid.to_string())
                    .status();

                match status {
                    Ok(s) if s.success() => {
                        println!("Sent SIGTERM to gateway (PID: {})", pid);
                    }
                    _ => {
                        println!("Failed to stop gateway (PID: {})", pid);
                    }
                }
            }

            #[cfg(not(unix))]
            {
                println!("Cannot stop gateway on this platform (PID: {})", pid);
                println!("Please stop the process manually.");
            }
        } else {
            println!("Gateway not running (stale PID file).");
        }

        let _ = std::fs::remove_file(&pid_file);
    } else {
        println!("No gateway PID file found. Gateway may not be running.");
    }

    Ok(())
}

async fn gateway_status() -> anyhow::Result<()> {
    let pid_file = pid_file_path();

    println!("Gateway Status");
    println!("==============\n");

    // Check PID
    if let Some(pid) = read_pid(&pid_file) {
        if is_process_running(pid) {
            println!("  Status: Running (PID: {})", pid);
        } else {
            println!("  Status: Not running (stale PID file)");
        }
    } else {
        println!("  Status: Not running");
    }

    // Check configuration
    let gateway_enabled = std::env::var("GATEWAY_ENABLED")
        .map(|v| v == "true")
        .unwrap_or(false);
    let gateway_port = std::env::var("GATEWAY_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(3000);
    let gateway_token = std::env::var("GATEWAY_AUTH_TOKEN").is_ok();

    println!("  Enabled: {}", gateway_enabled);
    println!("  Port: {}", gateway_port);
    println!(
        "  Auth Token: {}",
        if gateway_token { "set" } else { "not set" }
    );
    println!("  PID File: {}", pid_file.display());

    // Try to ping the gateway
    if gateway_enabled {
        let url = format!("http://127.0.0.1:{}/health", gateway_port);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()?;

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                println!("  Health: OK (reachable at {})", url);
            }
            Ok(resp) => {
                println!("  Health: Degraded (status {})", resp.status());
            }
            Err(_) => {
                println!("  Health: Unreachable");
            }
        }
    }

    Ok(())
}

fn pid_file_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ironclaw")
        .join("gateway.pid")
}

fn read_pid(path: &PathBuf) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn write_pid(path: &PathBuf) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, std::process::id().to_string())?;
    Ok(())
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use std::process::Command;
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .is_ok_and(|s| s.success())
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}
