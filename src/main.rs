use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use sulo_testharness::golden::{GoldenOutcome, check_golden};
use sulo_testharness::load::load_file;
use sulo_testharness::report::{render, render_json, render_junit};
use sulo_testharness::suite::{RunOptions, RunOutcome, aggregate_cases, run_suite};
use sulo_testharness::verdict::exit_code;

#[derive(Parser)]
#[command(
    name = "sulo-testharness",
    about = "Regression harness for the SULO ontology"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Output formats.
///
/// ONE flag with three values, deliberately not two boolean flags:
/// `--json --junit` would otherwise be a request the program has to
/// resolve by silently preferring one, and a consumer who asked for
/// JUnit would quietly get JSON.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum Format {
    /// Human-readable report, the default.
    Text,
    /// JSON, carrying `rests_on_absence` and `baseline_loss` per case.
    Json,
    /// JUnit XML, for a CI test-report consumer.
    Junit,
}

#[derive(Subcommand)]
enum Command {
    /// Run a suite of case manifests against an ontology.
    Run {
        /// Directory of case manifests, walked recursively.
        #[arg(long)]
        suite: PathBuf,
        /// Ontology every case is checked against, unless the case
        /// names its own. Optional only because a case may name its
        /// own; a case that needs this and does not get it is a
        /// configuration error, not a silent skip.
        #[arg(long)]
        ontology: Option<PathBuf>,
        /// Run only cases whose manifest path contains this
        /// substring. Matching nothing is exit 2, never a green run.
        #[arg(long)]
        filter: Option<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
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
        Command::Run {
            suite,
            ontology,
            filter,
            format,
        } => {
            let outcome = run_suite(&RunOptions {
                suite: &suite,
                ontology: ontology.as_deref(),
                filter: filter.as_deref(),
            });

            let results = match outcome {
                RunOutcome::Ran(r) => r,
                // Exit 2: a harness or configuration error, which is
                // NOT a statement about the ontology. Printed to
                // stderr so a consumer redirecting stdout to a report
                // file still sees it, and so the report file is not
                // left holding half a document.
                RunOutcome::Config(msg) => {
                    eprintln!("error: {msg}");
                    return ExitCode::from(2);
                }
            };

            let rendered = match format {
                Format::Text => render(&results),
                Format::Json => render_json(&results),
                Format::Junit => render_junit(&results),
            };
            print!("{rendered}");

            // Aggregated over EVERY case, not the last one, and
            // through the same precedence the per-case path uses.
            let verdict = aggregate_cases(&results);
            ExitCode::from(u8::try_from(exit_code(&verdict)).unwrap_or(2))
        }
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
                GoldenOutcome::Error(m) => {
                    eprintln!("error: {m}");
                    ExitCode::from(2)
                }
            }
        }
    }
}
