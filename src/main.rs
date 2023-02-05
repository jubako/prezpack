mod arx_creator;
mod create;
mod jim_creator;

use clap::Parser;
use create::{Creator, Entry};
use jubako as jbk;
use std::path::PathBuf;

#[derive(Parser)]
#[clap(name = "revealpack")]
#[clap(author, version, about, long_about=None)]
struct Cli {
    #[clap(short, long, action=clap::ArgAction::Count)]
    verbose: u8,

    // Input
    #[clap(value_parser)]
    infiles: Vec<PathBuf>,

    // Archive name to create
    #[clap(short, long, value_parser)]
    outfile: PathBuf,

    #[clap(short, long, value_parser)]
    main_entry: PathBuf,
}

fn main() -> jbk::Result<()> {
    let args = Cli::parse();

    if args.verbose > 0 {
        println!("Creating archive {:?}", args.outfile);
        println!("With files {:?}", args.infiles);
    }

    let mut creator = Creator::new(&args.outfile, args.main_entry)?;

    let root_parent = jbk::Vow::new(0.into());
    for infile in args.infiles {
        creator.push_back(Entry::new(infile, root_parent.bind())?);
    }

    creator.run(args.outfile)
}
