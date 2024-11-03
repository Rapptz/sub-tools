use clap::Parser;
use sub_tools::cli::{Cli, Subcommands};

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();

    match args.command {
        Subcommands::Convert(convert_args) => convert_args.run()?,
        Subcommands::Info(info_args) => info_args.run()?,
        Subcommands::Shift(shift_args) => shift_args.run()?,
        Subcommands::Cleanup(cleanup_args) => cleanup_args.run()?,
    }

    Ok(())
}
