use blueberry_serde::deserialize_packet;
use log::{debug, info, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::{SerialPortBuilderExt, SerialStream};

use crate::messages::Message;

const PREAMBLE: [u8; 4] = [0x42, 0x6C, 0x75, 0x65];

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn open(
    port: &str,
    baudrate: u32,
) -> std::result::Result<
    (
        tokio::io::ReadHalf<SerialStream>,
        tokio::io::WriteHalf<SerialStream>,
    ),
    Box<dyn std::error::Error>,
> {
    let stream = tokio_serial::new(port, baudrate).open_native_async()?;
    Ok(tokio::io::split(stream))
}

pub async fn send_all(
    tx: &mut tokio::io::WriteHalf<SerialStream>,
    packets: &[Vec<u8>],
) -> Result<()> {
    for packet in packets {
        tx.write_all(packet).await?;
        tx.flush().await?;
        debug!("TX packet ({} bytes)", packet.len());
    }
    Ok(())
}

pub async fn recv_loop(mut rx: tokio::io::ReadHalf<SerialStream>) -> Result<()> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 1024];

    let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let n = rx.read(&mut tmp).await?;
            if n == 0 {
                continue;
            }
            buf.extend_from_slice(&tmp[..n]);

            while let Some(messages) = extract_packet(&mut buf) {
                for msg in &messages {
                    info!("{msg:?}");
                }
            }
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
