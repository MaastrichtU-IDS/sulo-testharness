use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use sulo_testharness::golden::{GoldenOutcome, check_golden};
use sulo_testharness::load::load_file;
use sulo_testharness::report::{render, render_json, render_junit};
use sulo_testharness::suite::{DeferredCases, RunOptions, RunOutcome, aggregate_cases, run_suite};
use sulo_testharness::verdict::run_exit_code;

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

/// What to do with cases whose oracle of record is not this reasoner.
///
/// ONE flag with three values for the same reason `--format` is one
/// flag: `--include-deferred --only-deferred` would be a request the
/// program has to resolve by silently preferring one.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum Deferred {
    /// Name them, count them, do not run them. The default, and what
    /// spec line 746 asks for.
    Skip,
    /// Run them alongside everything else.
    Include,
    /// Run only them. The seam the phase 7 HermiT differential uses.
    Only,
}

impl From<Deferred> for DeferredCases {
    fn from(d: Deferred) -> Self {
        match d {
            Deferred::Skip => DeferredCases::Skip,
            Deferred::Include => DeferredCases::Include,
            Deferred::Only => DeferredCases::Only,
        }
    }
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
        /// What to do with cases tagged `oracle-hermit`, whose oracle
        /// of record is the CI differential rather than this
        /// reasoner. They are always named and counted in the report.
        #[arg(long, value_enum, default_value_t = Deferred::Skip)]
        deferred: Deferred,
        /// Exit 0 rather than 3 when the run holds an Indeterminate
        /// and no Fail (spec 5.4). Never suppresses a Fail, and the
        /// Indeterminates stay in the report either way.
        #[arg(long)]
        allow_indeterminate: bool,
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
            deferred,
            allow_indeterminate,
        } => {
            let outcome = run_suite(&RunOptions {
                suite: &suite,
                ontology: ontology.as_deref(),
                filter: filter.as_deref(),
                deferred: deferred.into(),
            });

            let (results, deferred) = match outcome {
                RunOutcome::Ran { results, deferred } => (results, deferred),
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
                Format::Text => render(&results, &deferred),
                Format::Json => render_json(&results, &deferred),
                Format::Junit => render_junit(&results, &deferred),
            };
            print!("{rendered}");
            // JSON has no trailing newline of its own, so a shell
            // redirect would otherwise write a file without one.
            if format == Format::Json {
                println!();
            }

            // Aggregated over EVERY case, not the last one, and
            // through the same precedence the per-case path uses.
            // `deferred` is deliberately not passed: a case nothing
            // was asked about cannot set an exit code.
            let verdict = aggregate_cases(&results);
            ExitCode::from(u8::try_from(run_exit_code(&verdict, allow_indeterminate)).unwrap_or(2))
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
