use clap::Parser;
use once_cell::sync::OnceCell;

static INSTANCE: OnceCell<Cli> = OnceCell::new();

#[derive(Parser, Debug)]
#[command(name = "potato", about = "Blueberry protocol serial tool")]
pub struct Cli {
    /// Serial port path
    #[arg(short, long, default_value = "/dev/ttyACM0")]
    pub port: String,

    /// Baud rate
    #[arg(short, long, default_value_t = 115_200)]
    pub baudrate: u32,

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
