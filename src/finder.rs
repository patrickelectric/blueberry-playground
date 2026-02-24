use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use log::{debug, info, warn};
use once_cell::sync::OnceCell;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio::time::timeout;
use tokio_serial::SerialPortBuilderExt;

use crate::communication;
use crate::messages::{Message, MessageKey, Module};

const PROBE_TIMEOUT: Duration = Duration::from_secs(1);
const PROBE_BAUDRATE: u32 = 115_200;

static INSTANCE: OnceCell<Finder> = OnceCell::new();

struct DeviceFound {
    source: String,
    messages: Vec<Message>,
}

pub struct Finder {
    // We should ignore responses from our own IP addresses
    local_ips: HashSet<Ipv4Addr>,
    broadcasts: Vec<(String, Ipv4Addr)>,
    serial_ports: Vec<String>,
    who_there_packet: Vec<u8>,
}

impl Finder {
    pub fn get() -> &'static Finder {
        INSTANCE.get_or_init(Self::new)
    }

    fn new() -> Self {
        let who_there_packet = Message::request_packet(Module::Blueberry, MessageKey::WhoseThere)
            .expect("Failed to build WhoseThere packet");

        let mut local_ips = HashSet::new();
        let mut broadcasts = Vec::new();

        match if_addrs::get_if_addrs() {
            Ok(ifaces) => {
                for iface in &ifaces {
                    if let if_addrs::IfAddr::V4(v4) = &iface.addr {
                        local_ips.insert(v4.ip);
                        if v4.ip.is_loopback() {
                            continue;
                        }
                        let bcast = broadcast_addr(v4.ip, v4.netmask);
                        if !broadcasts.iter().any(|(_, b)| *b == bcast) {
                            broadcasts.push((iface.name.clone(), bcast));
                        }
                    }
                }
            }
            Err(e) => warn!("Failed to enumerate network interfaces: {e}"),
        }

        let serial_ports = match tokio_serial::available_ports() {
            Ok(ports) => ports.into_iter().map(|p| p.port_name).collect(),
            Err(e) => {
                warn!("Failed to enumerate serial ports: {e}");
                Vec::new()
            }
        };

        Self {
            local_ips,
            broadcasts,
            serial_ports,
            who_there_packet,
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting device discovery...");

        let mut handles: Vec<tokio::task::JoinHandle<Vec<DeviceFound>>> = Vec::new();

        for port in &self.serial_ports {
            info!("Probing serial port: {port}");
            let port = port.clone();
            let packet = self.who_there_packet.clone();
            handles.push(tokio::spawn(
                async move { probe_serial(&port, &packet).await },
            ));
        }

        for (name, bcast) in &self.broadcasts {
            info!("Probing network {name} (broadcast {bcast})...");
            let bcast = *bcast;
            let packet = self.who_there_packet.clone();
            let locals = self.local_ips.clone();
            handles.push(tokio::spawn(async move {
                probe_udp_broadcast(bcast, &packet, &locals).await
            }));
        }

        let mut found = Vec::new();
        for handle in handles {
            found.extend(handle.await?);
        }

        if found.is_empty() {
            info!("No devices found");
        } else {
            info!("Found {} device(s):", found.len());
            for dev in &found {
                info!("  {}", dev.source);
                for msg in &dev.messages {
                    info!("    {msg:?}");
                }
            }
        }

        Ok(())
    }
}

fn broadcast_addr(ip: Ipv4Addr, netmask: Ipv4Addr) -> Ipv4Addr {
    let ip = u32::from(ip);
    let mask = u32::from(netmask);
    Ipv4Addr::from(ip | !mask)
}

async fn probe_serial(port: &str, packet: &[u8]) -> Vec<DeviceFound> {
    match probe_serial_inner(port, packet).await {
        Ok(devs) => devs,
        Err(e) => {
            debug!("Serial probe {port} failed: {e}");
            Vec::new()
        }
    }
}

async fn probe_serial_inner(
    port: &str,
    packet: &[u8],
) -> Result<Vec<DeviceFound>, Box<dyn std::error::Error + Send + Sync>> {
    debug!("Probing serial {port}");

    let stream = tokio_serial::new(port, PROBE_BAUDRATE).open_native_async()?;
    let (mut reader, mut writer) = tokio::io::split(stream);

    writer.write_all(packet).await?;
    writer.flush().await?;

    let mut buf = Vec::with_capacity(1024);
    let mut tmp = [0u8; 1024];

    let messages = match timeout(PROBE_TIMEOUT, async {
        let mut all = Vec::new();
        loop {
            let n = reader.read(&mut tmp).await?;
            if n == 0 {
                continue;
            }
            buf.extend_from_slice(&tmp[..n]);
            while let Some(msgs) = communication::extract_packet(&mut buf) {
                all.extend(msgs);
            }
            if !all.is_empty() {
                break;
            }
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(all)
    })
    .await
    {
        Ok(Ok(msgs)) => msgs,
        _ => return Ok(Vec::new()),
    };

    if messages.is_empty() {
        return Ok(Vec::new());
    }

    info!("Device found on serial port {port}");
    Ok(vec![DeviceFound {
        source: format!("Serial: {port}"),
        messages,
    }])
}

async fn probe_udp_broadcast(
    broadcast: Ipv4Addr,
    packet: &[u8],
    local_ips: &HashSet<Ipv4Addr>,
) -> Vec<DeviceFound> {
    match probe_udp_broadcast_inner(broadcast, packet, local_ips).await {
        Ok(devs) => devs,
        Err(e) => {
            debug!("UDP broadcast probe {broadcast} failed: {e}");
            Vec::new()
        }
    }
}

async fn probe_udp_broadcast_inner(
    broadcast: Ipv4Addr,
    packet: &[u8],
    local_ips: &HashSet<Ipv4Addr>,
) -> Result<Vec<DeviceFound>, Box<dyn std::error::Error + Send + Sync>> {
    debug!("Probing UDP broadcast {broadcast}");

    let socket = UdpSocket::bind("0.0.0.0:16962").await?; // This port is required
    socket.set_broadcast(true)?;

    let dest = SocketAddr::new(broadcast.into(), communication::DEFAULT_UDP_PORT);
    socket.send_to(packet, dest).await?;

    let mut by_source: HashMap<SocketAddr, Vec<u8>> = HashMap::new();
    let mut tmp = [0u8; 4096];

    let _ = timeout(PROBE_TIMEOUT, async {
        loop {
            let (n, from) = socket.recv_from(&mut tmp).await?;
            if n == 0 {
                continue;
            }
            if let SocketAddr::V4(v4) = from {
                if local_ips.contains(v4.ip()) {
                    debug!("Ignoring response from own address {from}");
                    continue;
                }
            }
            by_source
                .entry(from)
                .or_default()
                .extend_from_slice(&tmp[..n]);
        }
        #[allow(unreachable_code)]
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await;

    let mut found = Vec::new();
    for (addr, mut data) in by_source {
        let mut messages = Vec::new();
        while let Some(msgs) = communication::extract_packet(&mut data) {
            messages.extend(msgs);
        }
        if !messages.is_empty() {
            info!("Device found at {addr} (via broadcast {broadcast})");
            found.push(DeviceFound {
                source: format!("UDP: {addr} (via broadcast {broadcast})"),
                messages,
            });
        }
    }

    Ok(found)
}
