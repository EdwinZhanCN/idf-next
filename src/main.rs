use clap::Parser;

fn main() -> miette::Result<()> {
    idf_next::run(idf_next::cli::Cli::parse())
}
