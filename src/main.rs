mod cli;
mod communication;
mod finder;
mod messages;

use cli::Cli;
use finder::Finder;
use log::info;
use messages::{Message, MessageKey, Module};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::get();
    cli.init_logger();

    if cli.finder {
        return Finder::get().run().await;
    }

    let mut conn = if let Some(ref ip) = cli.ip {
        communication::Connection::open_udp(ip).await?
    } else {
        communication::Connection::open_serial(&cli.port, cli.baudrate)?
    };

    let requests = vec![
        (Module::Blueberry, MessageKey::Id),
        (Module::Blueberry, MessageKey::Version),
        (Module::Blueberry, MessageKey::WhoseThere),
        (Module::Test, MessageKey::Test),
        // (Module::Blueberry, MessageKey::AppData), // Restarts the MCU
    ];

    for request in &requests {
        info!("Sending request: {:?}", request);
        conn.send_all(&[Message::request_packet(request.0, request.1)?])
            .await?;
    }

    conn.recv_loop().await
}
