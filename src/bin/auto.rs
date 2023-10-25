use jubako as jbk;

use clap::{Args, Parser, Subcommand};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[clap(name = "prezpack")]
#[clap(author, version, about, long_about=None)]
struct Cli {
    #[clap(short, long, action=clap::ArgAction::Count)]
    verbose: u8,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[clap(arg_required_else_help = true)]
    Serve(Serve),

    #[clap(arg_required_else_help = true)]
    Extract(Extract),

    #[clap(arg_required_else_help = true)]
    Mount(Mount),
}

#[derive(Args)]
struct Extract {
    #[clap(value_parser)]
    outdir: PathBuf,
}

#[derive(Args)]
struct Mount {
    #[clap(value_parser)]
    mountdir: PathBuf,
}

#[derive(Args)]
struct Serve {
    #[clap(value_parser)]
    address: String,
}

fn main() -> ExitCode {
    match env::current_exe() {
        Ok(exe_path) => match run(exe_path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => match e.error {
                jbk::ErrorKind::NotAJbk => {
                    eprintln!("Impossible to locate a Jim archive in the executable.");
                    eprintln!("This binary is not intented to be directly used, you must put a Jim archive at its end.");
                    ExitCode::FAILURE
                }
                _ => {
                    eprintln!("Error: {e}");
                    ExitCode::FAILURE
                }
            },
        },
        Err(e) => {
            eprintln!("failed to get current exe path: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(jbk_path: PathBuf) -> jbk::Result<()> {
    let args = Cli::parse();

    match args.command {
        Commands::Extract(cmd) => {
            if args.verbose > 0 {
                println!("Extract archive {jbk_path:?} in {:?}", cmd.outdir);
            }

            arx::extract(&jbk_path, &cmd.outdir, Default::default(), false)
        }

        Commands::Mount(cmd) => {
            if args.verbose > 0 {
                println!("Mount archive {jbk_path:?} in {:?}", cmd.mountdir);
            }

            let arx = arx::Arx::new(&jbk_path)?;
            let arxfs = arx::ArxFs::new(arx)?;
            arxfs.mount(jbk_path.to_str().unwrap().to_string(), &cmd.mountdir)
        }

        Commands::Serve(cmd) => {
            if args.verbose > 0 {
                println!("Serve archive {jbk_path:?} at {:?}", cmd.address,);
            }
            let server = waj::Server::new(jbk_path)?;
            server.serve(&cmd.address)
        }
    }
}
