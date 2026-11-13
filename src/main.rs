use clap::Parser;

fn main() -> miette::Result<()> {
    let cli = idf_next::cli::Cli::try_parse().unwrap_or_else(|e| idf_next::banner::exit(e));
    idf_next::run(cli)
}
