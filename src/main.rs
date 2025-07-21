use {
    crate::{receiver::init_receiver, sender::init_sender},
    anyhow::Result,
    clap::{self, Parser, ValueEnum},
    tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt},
};

mod receiver;
mod sender;

#[derive(Parser)]
struct Args {
    #[clap(long, value_enum)]
    mode: Mode,
    #[clap(long)]
    address: String,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Mode {
    Sender,
    Receiver,
}

#[tokio::main]
async fn main() -> Result<()> {
    let stdout_layer = fmt::layer()
        .with_timer(fmt::time::UtcTime::rfc_3339()) // 2025-06-07T03:37:59Z
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .compact(); // concise one-liner
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(stdout_layer)
        .init();

    let args = Args::parse();
    match args.mode {
        Mode::Sender => init_sender(args.address.as_str(), 5000).await?,
        Mode::Receiver => init_receiver(args.address.as_str()).await?,
    }
    Ok(())
}
