use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod commands;
mod object;

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

    /// List the contents of a tree object
    LsTree {
        #[arg(long)]
        name_only: bool,

        tree_hash: String,
    },

    /// Write the contents of the staging area to disk
    WriteTree,

    /// Creates a commit object
    CommitTree {
        tree_hash: String,

        #[arg(short)]
        parent: Option<String>,

        #[arg(short)]
        message: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => commands::init::invoke().context("initialisation")?,
        Commands::CatFile {
            pretty_print: _,
            hash,
        } => commands::catfile::invoke(&hash).context("cat file invocation")?,

        Commands::HashObject { write, file } => {
            commands::hashobject::invoke(&file, write).context("hash object invocation")?
        }

        Commands::LsTree {
            name_only,
            tree_hash,
        } => commands::lstree::invoke(&tree_hash, name_only).context("lstree invocation")?,
        Commands::WriteTree => commands::writetree::invoke(".").context("write tree invocation")?,

        Commands::CommitTree {
            tree_hash,
            parent,
            message,
        } => commands::committree::invoke(tree_hash, parent, message)
            .context("commit tree invocation")?,
    }

    Ok(())
}
