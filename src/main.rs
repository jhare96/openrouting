use openrouting::dsn;
use openrouting::router;
use openrouting::ses;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "openrouting", about = "PCB auto-router: accepts a .dsn file and outputs a .ses file")]
struct Args {
    /// Input DSN file
    input: PathBuf,
    /// Output SES file (defaults to input filename with .ses extension)
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();
    let input = &args.input;
    let output = args.output.unwrap_or_else(|| input.with_extension("ses"));

    let content = std::fs::read_to_string(input).expect("Failed to read input file");

    let design = dsn::parse_dsn(&content).expect("Failed to parse DSN file");

    let routing = router::route(&design);

    eprintln!(
        "Routed {} nets, {} unrouted",
        design.nets.len() - routing.unrouted.len(),
        routing.unrouted.len()
    );
    if !routing.unrouted.is_empty() {
        eprintln!("Unrouted nets: {}", routing.unrouted.join(", "));
    }

    ses::write_ses(&design, &routing, &output).expect("Failed to write SES file");
    eprintln!("Wrote SES file: {}", output.display());
}
