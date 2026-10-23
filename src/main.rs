//! The `allthechanges` command line: parses arguments, calls the library,
//! prints the report on stdout and maps the outcome to an exit code.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use allthechanges::changelog::{self, Release};
use allthechanges::report::{GatherReport, QueryReport};
use allthechanges::{Error, git, scan};
use clap::{Parser, Subcommand};

const AFTER_HELP: &str = "\
Exit codes:
  0  done; for query, no problems and (with --require) at least one fragment
  1  the check failed: lint problems, --require with no fragment, gather
     refused (problems, nothing to gather, version already present)
  2  usage error
  3  operational error: unreadable file, git failed, both changelog.d and
     .changelog.d present, CHANGELOG.md not UTF-8 or with CRLF line endings

stdout carries the report and nothing else (with --json, one JSON document
and a newline); stderr carries diagnostics only.";

/// Changelog fragments: one file per change under changelog.d/, so parallel
/// branches never conflict on CHANGELOG.md.
#[derive(Debug, Parser)]
#[command(name = "allthechanges", version, after_help = AFTER_HELP)]
struct Cli {
    /// The fragment directory [default: changelog.d, else .changelog.d]
    #[arg(long, global = true, value_name = "PATH")]
    dir: Option<PathBuf>,

    /// Print the report as one JSON document
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Lint and list the fragments, or those added in a git range
    ///
    /// Fragments are named [<YYYY-MM-DD>.]<slug>.<section>.md, where section
    /// is added, changed, deprecated, removed, fixed or security, and hold
    /// one or more Markdown list items ("- ..."). A missing fragment
    /// directory holds no fragments.
    Query {
        /// List only the fragments git reports as added in this range
        /// (A..B, A...B or a single ref), read from the working tree
        #[arg(long, value_name = "GIT-RANGE")]
        range: Option<String>,

        /// Fail unless at least one fragment is listed
        #[arg(long)]
        require: bool,
    },

    /// Move every fragment into a new release section of CHANGELOG.md
    ///
    /// Lints first and writes nothing on any problem. The section,
    /// "## [<version>] - <date>", goes after an [Unreleased] section if
    /// there is one, else before the first release, else at the end; nothing
    /// else in the file is changed. The gathered fragments are then deleted.
    /// No git operation is run.
    Gather {
        /// The release, written verbatim (no whitespace, '[' or ']')
        #[arg(long, value_name = "VERSION", value_parser = changelog::parse_version)]
        version: String,

        /// The release date [default: today, local time]
        #[arg(long, value_name = "YYYY-MM-DD", value_parser = changelog::parse_date)]
        date: Option<String>,

        /// The changelog to write; created if absent
        #[arg(long, value_name = "PATH", default_value = "CHANGELOG.md")]
        changelog: PathBuf,

        /// Print the section that would be written; write and delete nothing
        #[arg(long)]
        dry_run: bool,
    },
}

/// The process exit codes; one meaning each. Usage errors exit 2 from
/// clap's own handling.
#[derive(Debug, Clone, Copy)]
enum Exit {
    Done = 0,
    CheckFailed = 1,
    Operational = 3,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        ExitCode::from(exit as u8)
    }
}

fn main() -> ExitCode {
    // clap prints help and version on stdout with exit 0, and usage errors on
    // stderr with exit 2.
    let cli = Cli::parse();
    match run(&cli) {
        Ok(exit) => exit.into(),
        Err(e) => {
            eprintln!("allthechanges: {e}");
            Exit::Operational.into()
        }
    }
}

/// Runs the command and prints its report. An `Err` from the command comes
/// before anything is printed.
fn run(cli: &Cli) -> Result<Exit, Error> {
    let (report, ok) = match &cli.command {
        Command::Query { range, require } => {
            let dir = scan::locate(Path::new(""), cli.dir.as_deref())?;
            let scanned = match (&dir, range) {
                (None, None) => scan::Scan::default(),
                (None, Some(range)) => git::scan_range_defaults(range)?,
                (Some(dir), None) => scan::scan(dir)?,
                (Some(dir), Some(range)) => git::scan_range(dir, range)?,
            };
            let report = QueryReport::new(dir.as_deref(), range.as_deref(), *require, &scanned);
            let text = if cli.json {
                report.json()
            } else {
                report.human()
            };
            (text, report.ok)
        }
        Command::Gather {
            version,
            date,
            changelog,
            dry_run,
        } => {
            let dir = scan::locate(Path::new(""), cli.dir.as_deref())?;
            let scanned = match &dir {
                None => scan::Scan::default(),
                Some(dir) => scan::scan(dir)?,
            };
            let date = date.clone().unwrap_or_else(changelog::today);
            let release = Release {
                changelog,
                version,
                date: &date,
                dry_run: *dry_run,
            };
            let outcome = changelog::gather(&scanned, &release)?;
            let report = GatherReport::new(&release, &scanned, &outcome);
            let text = if cli.json {
                report.json()
            } else {
                report.human()
            };
            (text, report.ok)
        }
    };
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(report.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(Error::io(Path::new("<stdout>")))?;
    Ok(if ok { Exit::Done } else { Exit::CheckFailed })
}
