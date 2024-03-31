use std::{fs, io::Write};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod commands;
mod object;

use object::{GitObject, GitObjectType};

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

    /// Parse a blob
    CatFile {
        #[arg(short)]
        pretty_print: bool,

        hash: String,
    },

    /// Write a blob to disk
    HashObject {
        #[arg(short)]
        write: bool,

        file: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
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
            let object = GitObject::load_blob(&hash).context("loading git object from hash")?;
            match &object.obj_type {
                GitObjectType::Blob => {
                    let mut stdout = std::io::stdout();
                    stdout
                        .write_all(&object.content)
                        .context("writing blob to stdout")?
                }
                _ => anyhow::bail!("we don't support the rest"),
            }
        }

        Commands::HashObject { write, file } => {
            let object = GitObject::create_blob(&file).context("creating git object")?;
            if write {
                object.write().context("writing git object")?;
            }

            println!("{}", object.hash);
        }
    }

    Ok(())
}
