use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
// The whole module, not its `render`/`render_json`, which collide by
// name with `report`'s. Qualified at the two call sites instead, so a
// reader sees which report is being built.
use sulo_testharness::differential;
use sulo_testharness::differential::{
    DifferentialOptions, DifferentialOutcome, differential_exit_code, run_differential,
};
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

/// Output formats for the `differential` subcommand.
///
/// No JUnit here, deliberately. A differential run makes one claim
/// ("the two reasoners agree"), which is not a set of test cases, and
/// rendering it as one would invite a consumer to read a Divergence as
/// an ordinary test failure. It is neither: it is a statement about a
/// REASONER, not about SULO.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum DiffFormat {
    /// Human-readable report, the default.
    Text,
    /// JSON, one object per question.
    Json,
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
    /// Run only them, under the pinned reasoner. Not what the
    /// `differential` subcommand uses; that one includes every case.
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
    /// Cross-check every absence-resting answer against HermiT.
    ///
    /// Needs a JVM and a ROBOT jar, which is why it is its own
    /// subcommand rather than a flag on `run`: neither may leak into
    /// the default or local path (spec 5.3).
    Differential {
        /// Directory of case manifests, walked recursively.
        #[arg(long)]
        suite: PathBuf,
        /// The ontology both reasoners are asked about. Required, not
        /// optional as it is on `run`.
        #[arg(long)]
        ontology: PathBuf,
        /// Path to a ROBOT jar (measured against 1.9.7).
        #[arg(long)]
        robot: PathBuf,
        /// Run only cases whose manifest path contains this
        /// substring. Matching nothing is exit 2, never a green run.
        #[arg(long)]
        filter: Option<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = DiffFormat::Text)]
        format: DiffFormat,
        /// Where probe ontologies and ROBOT's own output are kept, one
        /// directory per question. Defaults to a directory under the
        /// system temp directory. A divergence is only actionable if
        /// the reader can open the probe that produced it, so these
        /// are never deleted.
        #[arg(long)]
        workdir: Option<PathBuf>,
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
        Command::Differential {
            suite,
            ontology,
            robot,
            filter,
            format,
            workdir,
        } => {
            let workdir = workdir.unwrap_or_else(|| {
                std::env::temp_dir().join(format!(
                    "sulo-testharness-differential-{}",
                    std::process::id()
                ))
            });
            let opts = DifferentialOptions {
                suite: &suite,
                ontology: &ontology,
                robot: &robot,
                filter: filter.as_deref(),
                workdir: &workdir,
            };

            let asked = match run_differential(&opts) {
                DifferentialOutcome::Ran(a) => a,
                // Exit 2: a harness or configuration error, which is
                // NOT a statement about either reasoner. Same route,
                // and the same stderr, as `run`'s.
                DifferentialOutcome::Config(msg) => {
                    eprintln!("error: {msg}");
                    return ExitCode::from(2);
                }
            };

            match format {
                DiffFormat::Text => print!("{}", differential::render(&asked, &opts)),
                DiffFormat::Json => println!("{}", differential::render_json(&asked, &opts)),
            }

            // 5 on any divergence, 3 on any question that could not be
            // answered, 0 only when every question was asked AND every
            // answer matched.
            ExitCode::from(u8::try_from(differential_exit_code(&asked)).unwrap_or(2))
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
