//! mDNS/Bonjour service advertisement for gateway discovery.
//!
//! Advertises the IronClaw gateway on the local network using multicast DNS
//! (mDNS), allowing companion apps and other devices to discover it
//! automatically via Bonjour/Avahi.
//!
//! Service type: `_ironclaw._tcp.local.`

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

/// mDNS multicast address and port.
const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;

/// Service type for IronClaw gateway discovery.
const SERVICE_TYPE: &str = "_ironclaw._tcp.local.";

/// mDNS service advertiser for gateway discovery.
pub struct MdnsAdvertiser {
    /// Shutdown signal sender.
    shutdown_tx: Option<watch::Sender<bool>>,
    /// Service info being advertised.
    info: ServiceInfo,
}

/// Information about the advertised service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Instance name (e.g., "IronClaw on hostname").
    pub instance_name: String,
    /// Service type (always "_ironclaw._tcp.local.").
    pub service_type: String,
    /// Hostname of the machine.
    pub hostname: String,
    /// Port the gateway is listening on.
    pub port: u16,
    /// Optional TXT record key-value pairs.
    pub txt_records: std::collections::HashMap<String, String>,
}

impl MdnsAdvertiser {
    /// Create a new mDNS advertiser for the gateway.
    pub fn new(gateway_port: u16, auth_required: bool) -> Self {
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "ironclaw".to_string());

        let instance_name = format!("IronClaw on {}", hostname);

        let mut txt_records = std::collections::HashMap::new();
        txt_records.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
        txt_records.insert("auth".to_string(), auth_required.to_string());
        txt_records.insert("path".to_string(), "/api".to_string());

        Self {
            shutdown_tx: None,
            info: ServiceInfo {
                instance_name,
                service_type: SERVICE_TYPE.to_string(),
                hostname,
                port: gateway_port,
                txt_records,
            },
        }
    }

    /// Get the service info.
    pub fn info(&self) -> &ServiceInfo {
        &self.info
    }

    /// Start advertising the service on the local network.
    ///
    /// Spawns a background task that responds to mDNS queries.
    /// Returns immediately; call `stop()` to cease advertising.
    pub fn start(&mut self) -> Result<(), String> {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        self.shutdown_tx = Some(shutdown_tx);

        let info = self.info.clone();

        tokio::spawn(async move {
            if let Err(e) = run_mdns_responder(info, shutdown_rx).await {
                tracing::warn!("mDNS advertiser stopped: {}", e);
            }
        });

        tracing::info!(
            "mDNS: advertising '{}' on port {}",
            self.info.instance_name,
            self.info.port
        );

        Ok(())
    }

    /// Stop advertising.
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
            tracing::info!("mDNS: stopped advertising");
        }
    }

    /// Check if currently advertising.
    pub fn is_running(&self) -> bool {
        self.shutdown_tx.is_some()
    }
}

impl Drop for MdnsAdvertiser {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Build a minimal mDNS response packet for our service.
///
/// This is a simplified DNS response that announces our service.
/// Real mDNS implementations use more complex packet formats, but this
/// is sufficient for basic discovery by companion apps.
fn build_mdns_response(info: &ServiceInfo, transaction_id: u16) -> Vec<u8> {
    let mut packet = Vec::new();

    // DNS Header
    packet.extend_from_slice(&transaction_id.to_be_bytes()); // Transaction ID
    packet.extend_from_slice(&[0x84, 0x00]); // Flags: Response, Authoritative
    packet.extend_from_slice(&[0x00, 0x00]); // Questions: 0
    packet.extend_from_slice(&[0x00, 0x01]); // Answer RRs: 1
    packet.extend_from_slice(&[0x00, 0x00]); // Authority RRs: 0
    packet.extend_from_slice(&[0x00, 0x01]); // Additional RRs: 1

    // Answer: SRV record pointing to our host:port
    // Name: _ironclaw._tcp.local.
    for label in SERVICE_TYPE.trim_end_matches('.').split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0x00); // End of name

    packet.extend_from_slice(&[0x00, 0x21]); // Type: SRV
    packet.extend_from_slice(&[0x80, 0x01]); // Class: IN, cache flush
    packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x78]); // TTL: 120 seconds
    // RDATA: priority(2) + weight(2) + port(2) + target
    let hostname_labels: Vec<&str> = info.hostname.split('.').collect();
    let mut rdata = Vec::new();
    rdata.extend_from_slice(&[0x00, 0x00]); // Priority: 0
    rdata.extend_from_slice(&[0x00, 0x00]); // Weight: 0
    rdata.extend_from_slice(&info.port.to_be_bytes()); // Port
    for label in &hostname_labels {
        rdata.push(label.len() as u8);
        rdata.extend_from_slice(label.as_bytes());
    }
    rdata.push(5); // "local" label
    rdata.extend_from_slice(b"local");
    rdata.push(0x00); // End of name
    packet.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
    packet.extend_from_slice(&rdata);

    // Additional: TXT record with service metadata
    for label in SERVICE_TYPE.trim_end_matches('.').split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0x00);

    packet.extend_from_slice(&[0x00, 0x10]); // Type: TXT
    packet.extend_from_slice(&[0x80, 0x01]); // Class: IN, cache flush
    packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x78]); // TTL: 120

    let mut txt_data = Vec::new();
    for (key, value) in &info.txt_records {
        let entry = format!("{}={}", key, value);
        txt_data.push(entry.len() as u8);
        txt_data.extend_from_slice(entry.as_bytes());
    }
    packet.extend_from_slice(&(txt_data.len() as u16).to_be_bytes());
    packet.extend_from_slice(&txt_data);

    packet
}

/// Run the mDNS responder loop.
async fn run_mdns_responder(
    info: ServiceInfo,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<(), String> {
    let socket = UdpSocket::bind(("0.0.0.0", MDNS_PORT))
        .or_else(|_| UdpSocket::bind(("0.0.0.0", 0)))
        .map_err(|e| format!("Failed to bind mDNS socket: {}", e))?;

    socket
        .join_multicast_v4(&MDNS_ADDR, &Ipv4Addr::UNSPECIFIED)
        .map_err(|e| format!("Failed to join multicast group: {}", e))?;

    socket
        .set_nonblocking(true)
        .map_err(|e| format!("Failed to set non-blocking: {}", e))?;

    let socket = Arc::new(socket);

    // Send initial announcement
    let announcement = build_mdns_response(&info, 0);
    let mdns_dest = SocketAddr::new(MDNS_ADDR.into(), MDNS_PORT);
    let _ = socket.send_to(&announcement, mdns_dest);

    let mut buf = [0u8; 4096];
    let mut re_announce_interval = tokio::time::interval(Duration::from_secs(60));

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
            }
            _ = re_announce_interval.tick() => {
                // Periodic re-announcement
                let packet = build_mdns_response(&info, 0);
                let _ = socket.send_to(&packet, mdns_dest);
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                // Check for incoming queries
                match socket.recv_from(&mut buf) {
                    Ok((len, src)) => {
                        if len >= 12 {
                            // Check if this is a query for our service type
                            let flags = u16::from_be_bytes([buf[2], buf[3]]);
                            let is_query = flags & 0x8000 == 0;
                            if is_query {
                                // Check if the query contains our service type
                                let packet_data = &buf[..len];
                                if contains_service_query(packet_data) {
                                    let transaction_id = u16::from_be_bytes([buf[0], buf[1]]);
                                    let response = build_mdns_response(&info, transaction_id);
                                    let _ = socket.send_to(&response, src);
                                }
                            }
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // No data available
                    }
                    Err(_) => {
                        // Ignore other errors
                    }
                }
            }
        }
    }

    // Send goodbye (TTL=0)
    let _ = socket.leave_multicast_v4(&MDNS_ADDR, &Ipv4Addr::UNSPECIFIED);
    Ok(())
}

/// Check if an mDNS query packet contains a query for our service type.
fn contains_service_query(packet: &[u8]) -> bool {
    // Simple heuristic: check if "_ironclaw" appears in the packet
    let needle = b"_ironclaw";
    packet.windows(needle.len()).any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_info_creation() {
        let advertiser = MdnsAdvertiser::new(3000, true);
        let info = advertiser.info();
        assert!(info.instance_name.starts_with("IronClaw on"));
        assert_eq!(info.service_type, "_ironclaw._tcp.local.");
        assert_eq!(info.port, 3000);
        assert!(info.txt_records.contains_key("version"));
        assert_eq!(info.txt_records.get("auth").unwrap(), "true");
    }

    #[test]
    fn test_build_mdns_response() {
        let info = ServiceInfo {
            instance_name: "Test".to_string(),
            service_type: SERVICE_TYPE.to_string(),
            hostname: "testhost".to_string(),
            port: 3000,
            txt_records: std::collections::HashMap::new(),
        };
        let packet = build_mdns_response(&info, 42);
        // Should be a valid DNS response
        assert!(packet.len() > 12);
        // Check transaction ID
        assert_eq!(u16::from_be_bytes([packet[0], packet[1]]), 42);
        // Check flags indicate response
        assert!(packet[2] & 0x80 != 0);
    }

    #[test]
    fn test_contains_service_query() {
        let mut packet = vec![0u8; 12]; // DNS header
        packet.push(9);
        packet.extend_from_slice(b"_ironclaw");
        assert!(contains_service_query(&packet));

        let empty_packet = vec![0u8; 20];
        assert!(!contains_service_query(&empty_packet));
    }

    #[test]
    fn test_is_running() {
        let advertiser = MdnsAdvertiser::new(3000, false);
        assert!(!advertiser.is_running());
    }
}
