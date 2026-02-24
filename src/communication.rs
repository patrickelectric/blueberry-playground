use blueberry_serde::deserialize_packet;
use log::{debug, info, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio_serial::SerialPortBuilderExt;

use crate::messages::Message;

const PREAMBLE: [u8; 4] = [0x42, 0x6C, 0x75, 0x65];
const DEFAULT_UDP_PORT: u16 = 16962;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

enum Transport {
    Serial {
        reader: tokio::io::ReadHalf<tokio_serial::SerialStream>,
        writer: tokio::io::WriteHalf<tokio_serial::SerialStream>,
    },
    Udp(UdpSocket),
}

pub struct Connection {
    transport: Transport,
}

impl Connection {
    pub fn open_serial(port: &str, baudrate: u32) -> Result<Self> {
        info!("Connecting via serial to {port} @ {baudrate}");
        let stream = tokio_serial::new(port, baudrate).open_native_async()?;
        let (reader, writer) = tokio::io::split(stream);
        Ok(Self {
            transport: Transport::Serial { reader, writer },
        })
    }

    pub async fn open_udp(addr: &str) -> Result<Self> {
        let addr = if addr.contains(':') {
            addr.to_string()
        } else {
            format!("{addr}:{DEFAULT_UDP_PORT}")
        };
        info!("Connecting via UDP to {addr}");
        let socket = UdpSocket::bind("0.0.0.0:16962").await?; // This port is required
        socket.connect(&addr).await?;
        Ok(Self {
            transport: Transport::Udp(socket),
        })
    }

    pub async fn send_all(&mut self, packets: &[Vec<u8>]) -> Result<()> {
        match &mut self.transport {
            Transport::Serial { writer, .. } => {
                for packet in packets {
                    writer.write_all(packet).await?;
                    writer.flush().await?;
                    debug!("TX packet ({} bytes)", packet.len());
                }
            }
            Transport::Udp(socket) => {
                for packet in packets {
                    socket.send(packet).await?;
                    debug!("TX packet ({} bytes)", packet.len());
                }
            }
        }
        Ok(())
    }

    pub async fn recv_loop(self) -> Result<()> {
        let mut buf = Vec::with_capacity(4096);
        let mut tmp = [0u8; 1024];

        let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            match self.transport {
                Transport::Serial { mut reader, .. } => loop {
                    let n = reader.read(&mut tmp).await?;
                    if n == 0 {
                        continue;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    drain_packets(&mut buf);
                },
                Transport::Udp(socket) => loop {
                    let n = socket.recv(&mut tmp).await?;
                    if n == 0 {
                        continue;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    drain_packets(&mut buf);
                },
            }
            #[allow(unreachable_code)]
            Ok::<(), Box<dyn std::error::Error>>(())
        })
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => {
                info!("Receive loop finished (5s timeout)");
                Ok(())
            }
        }
    }
}

fn drain_packets(buf: &mut Vec<u8>) {
    while let Some(messages) = extract_packet(buf) {
        for msg in &messages {
            info!("{msg:?}");
        }
    }
}

/// Try to extract the next complete packet from `buf`.
/// Consumes the bytes on success, or discards garbage bytes before the next preamble.
/// Returns `None` when no complete packet is available yet.
fn extract_packet(buf: &mut Vec<u8>) -> Option<Vec<Message>> {
    let start = buf.windows(4).position(|w| w == PREAMBLE)?;

    if start > 0 {
        buf.drain(..start);
    }

    if buf.len() < 8 {
        return None;
    }

    let length_words = u16::from_le_bytes([buf[4], buf[5]]) as usize;
    let packet_len = length_words * 4;

    if packet_len < 8 || buf.len() < packet_len {
        return None;
    }

    match deserialize_packet(&buf[..packet_len]) {
        Ok((_hdr, raw_messages)) => {
            let messages = raw_messages.iter().map(|r| Message::from_raw(r)).collect();
            buf.drain(..packet_len);
            Some(messages)
        }
        Err(e) => {
            warn!("Parse error: {e}, skipping");
            buf.drain(..4);
            None
        }
    }
}
