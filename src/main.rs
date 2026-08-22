use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sulo_testharness::golden::{GoldenOutcome, check_golden};
use sulo_testharness::load::load_file;

#[derive(Parser)]
#[command(
    name = "sulo-testharness",
    about = "Regression harness for the SULO ontology"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compare the inferred closure against a golden file.
    Golden {
        #[arg(long)]
        ontology: PathBuf,
        #[arg(long)]
        golden: PathBuf,
        /// Re-baseline instead of comparing.
        #[arg(long)]
        accept_golden: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Golden {
            ontology,
            golden,
            accept_golden,
        } => {
            let loaded = match load_file(&ontology) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::from(2);
                }
            };

            for l in &loaded.loss {
                eprintln!("warning: axiom loss: {l}");
            }

            match check_golden(&loaded.ontology, &golden, accept_golden) {
                GoldenOutcome::Match => {
                    println!("golden closure matches");
                    ExitCode::SUCCESS
                }
                GoldenOutcome::Rebaselined => {
                    println!("golden closure written to {}", golden.display());
                    ExitCode::SUCCESS
                }
                GoldenOutcome::Drift(d) => {
                    println!("golden closure drift:\n{d}");
                    ExitCode::from(4)
                }
                GoldenOutcome::RebaselineRequired(m) => {
                    println!("re-baseline required: {m}");
                    ExitCode::from(4)
                }
            }
        }
    }
}
