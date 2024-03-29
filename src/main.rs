use std::fs;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

mod commands;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialises a Git repository
    Init,

    CatFile {
        #[arg(short)]
        pretty_print: bool,

        hash: String,
    },
}

fn main() -> Result<()> {
    println!("Logs from your program will appear here!");
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => {
            fs::create_dir_all(".git/objects").context("creating the git objects directory")?;
            fs::create_dir_all(".git/refs").context("creating the git refs directory")?;
            fs::write(".git/HEAD", "ref: refs/heads/main\n").context("writing HEAD file")?;
            println!("Initialized git directory");
        }
        Commands::CatFile { pretty_print, hash } => {
            println!("Args: {pretty_print:?}");
        }
    }

    Ok(())
}
