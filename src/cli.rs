use clap::Parser;
use once_cell::sync::OnceCell;

static INSTANCE: OnceCell<Cli> = OnceCell::new();

#[derive(Parser, Debug)]
#[command(name = "playground", about = "Blueberry protocol playground")]
pub struct Cli {
    /// Serial port path
    #[arg(short, long, default_value = "/dev/ttyACM0")]
    pub port: String,

    /// Baud rate (only with --port)
    #[arg(short, long, default_value_t = 115_200, requires = "port")]
    pub baudrate: u32,

    /// Connect via UDP to a device at this IP address (e.g. 192.168.31.28 or 192.168.31.28:16962)
    #[arg(short, long, conflicts_with_all = ["port", "baudrate"])]
    pub ip: Option<String>,

    /// Discover all Blueberry devices on serial ports and local networks
    #[arg(short, long, conflicts_with_all = ["port", "baudrate", "ip"])]
    pub finder: bool,

    /// Enable verbose/debug output
    #[arg(short, long)]
    pub verbose: bool,
}

impl Cli {
    pub fn get() -> &'static Cli {
        INSTANCE.get_or_init(|| Cli::parse())
    }

    pub fn init_logger(&self) {
        let level = if self.verbose { "debug" } else { "info" };
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(level)).init();
    }
}
