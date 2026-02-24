mod cli;
mod communication;
mod messages;

use cli::Cli;
use log::info;
use messages::{Message, MessageKey, Module};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::get();
    cli.init_logger();

    info!("Connecting to {} @ {}", cli.port, cli.baudrate);
    let (rx, mut tx) = communication::open(&cli.port, cli.baudrate)?;

    let requests = vec![
        (Module::Blueberry, MessageKey::Id),
        (Module::Blueberry, MessageKey::Version),
        (Module::Blueberry, MessageKey::WhoseThere),
        (Module::Test, MessageKey::Test),
        // (Module::Blueberry, MessageKey::AppData), // get's the MCU stuck
    ];

    for request in &requests {
        info!("Sending request: {:?}", request);
        communication::send_all(&mut tx, &[Message::request_packet(request.0, request.1)?]).await?;
    }

    communication::recv_loop(rx).await
}
