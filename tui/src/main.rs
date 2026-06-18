use clap::Parser;

#[derive(Parser)]
#[command(name = "paypunk-tui", about = "Paypunk Terminal UI")]
struct Args {
    #[arg(short, long)]
    socket_path: Option<String>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    paypunk_tui::run_tui(args.socket_path).await
}
