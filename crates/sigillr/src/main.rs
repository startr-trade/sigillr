/// Sigillr: a public identity registry with nothing personal behind it.
#[derive(clap::Parser)]
#[command(name = "sigillr", version, about, long_about = None, after_help = PLANNED)]
struct Cli {}

const PLANNED: &str = "Commands arrive in later releases: serve (run an instance), \
lookup (check a key or certificate), index rebuild (rebuild the index from the registry).";

fn main() {
    let _ = <Cli as clap::Parser>::parse();
}
