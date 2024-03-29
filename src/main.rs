use std::fs;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod commands;
mod object;

use commands::catfile;
use object::{parse_object, GitObject};

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
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => {
            fs::create_dir_all(".git/objects").context("creating the git objects directory")?;
            fs::create_dir_all(".git/refs").context("creating the git refs directory")?;
            fs::write(".git/HEAD", "ref: refs/heads/main\n").context("writing HEAD file")?;
            println!("Initialized git directory");
        }
        Commands::CatFile {
            pretty_print: _,
            hash,
        } => {
            anyhow::ensure!(hash.len() == 40);
            let (gobj, content) = parse_object(hash).context("parsing git object")?;
            if gobj == GitObject::Invalid {
                anyhow::bail!("invalid git object");
            }

            catfile::read_object(gobj, content).context("cating file")?;
        }
    }

    Ok(())
}
