//! `docs` CLI subcommand — `list | show | browse | serve | ask`.
//!
//! Wires the [`crate::DocsBrowser`] / [`crate::DocsServer`] library
//! APIs to the user-facing CLI. Each invocation parses its own
//! arguments via clap (the [`Command`] trait gives no
//! `clap::ArgMatches` to subcommands), reads the doc tree out of
//! `rtb_app::App::assets` under `docs/`, and dispatches.
//!
//! # Lint exception
//!
//! `linkme::distributed_slice` emits `#[link_section]` which Rust
//! 1.95+ flags under `unsafe_code`. Allowed at module level — no
//! hand-rolled `unsafe` blocks anywhere in the module.

#![allow(unsafe_code)]

use std::ffi::OsString;
use std::net::SocketAddr;

use async_trait::async_trait;
use clap::{Parser, Subcommand};
use linkme::distributed_slice;
use miette::{miette, IntoDiagnostic};
use rtb_app::app::App;
use rtb_app::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb_app::features::Feature;

use crate::loader::load_docs;
use crate::render::{to_html_document, to_plain_text};
use crate::server::DocsServer;

/// The default doc-tree root inside the asset overlay.
const DEFAULT_ROOT: &str = "docs";

/// The `docs` subcommand.
pub struct DocsCmd;

#[async_trait]
impl Command for DocsCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "docs",
            about: "Browse the embedded documentation",
            aliases: &[],
            feature: Some(Feature::Docs),
        };
        &SPEC
    }

    /// `docs` owns its inner clap subtree (`list / show / browse /
    /// serve / ask`). Opt into the framework's passthrough so the
    /// inner parser handles `--help` + arg validation.
    fn subcommand_passthrough(&self) -> bool {
        true
    }

    async fn run(&self, app: App) -> miette::Result<()> {
        // The `Command` trait runs `async fn(App)` — args were
        // consumed by rtb-cli's clap tree at the top level. Re-read
        // `std::env::args_os()` and skip past the binary + the
        // matched `docs` token to get the docs-specific tail.
        let mut args: Vec<OsString> = std::env::args_os().collect();
        if args.len() >= 2 {
            args.drain(..2);
        }
        // clap expects a leading arg0; synthesise one.
        args.insert(0, OsString::from("docs"));
        let cli = match DocsCli::try_parse_from(args) {
            Ok(cli) => cli,
            Err(e) => {
                // `--help` / `--version` aren't errors — clap formats
                // them as `Err` so the caller can print and exit. Mirror
                // that here: print the formatted output and return Ok.
                use clap::error::ErrorKind;
                if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                    print!("{e}");
                    return Ok(());
                }
                return Err(miette!("{e}"));
            }
        };
        match cli.command {
            DocsSub::List(opts) => run_list(&app, &opts),
            DocsSub::Show(opts) => run_show(&app, &opts),
            DocsSub::Browse(opts) => run_browse(&app, &opts),
            DocsSub::Serve(opts) => run_serve(app, opts).await,
            DocsSub::Ask(opts) => run_ask(&app, &opts),
        }
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_docs() -> Box<dyn Command> {
    Box::new(DocsCmd)
}

// ---------------------------------------------------------------------
// clap surface
// ---------------------------------------------------------------------

#[derive(Debug, Parser)]
#[command(name = "docs", about = "Browse the embedded documentation")]
struct DocsCli {
    #[command(subcommand)]
    command: DocsSub,
}

#[derive(Debug, Subcommand)]
enum DocsSub {
    /// Print every page in the doc tree.
    List(ListOpts),
    /// Render a single page to stdout.
    Show(ShowOpts),
    /// Open the interactive TUI browser (`q` to quit).
    Browse(BrowseOpts),
    /// Start the loopback HTTP server (Ctrl+C to stop).
    Serve(ServeOpts),
    /// AI Q&A — gated on the `ai` Cargo feature.
    Ask(AskOpts),
}

#[derive(Debug, clap::Args)]
struct ListOpts {
    /// Doc-tree root inside the asset overlay.
    #[arg(long, default_value = DEFAULT_ROOT)]
    root: String,
}

#[derive(Debug, clap::Args)]
struct ShowOpts {
    /// Page path under the doc-tree root, e.g. `intro.md`.
    path: String,
    /// Render as plain text (default) or HTML.
    #[arg(long, value_parser = ["plain", "html"], default_value = "plain")]
    format: String,
    #[arg(long, default_value = DEFAULT_ROOT)]
    root: String,
}

#[derive(Debug, clap::Args)]
struct BrowseOpts {
    #[arg(long, default_value = DEFAULT_ROOT)]
    root: String,
}

#[derive(Debug, clap::Args)]
struct ServeOpts {
    /// Bind address. `127.0.0.1:0` (default) picks a free port.
    #[arg(long, default_value = "127.0.0.1:0")]
    bind: SocketAddr,
    #[arg(long, default_value = DEFAULT_ROOT)]
    root: String,
}

#[derive(Debug, clap::Args)]
struct AskOpts {
    /// Question to put to the configured AI provider.
    #[arg(trailing_var_arg = true)]
    question: Vec<String>,
}

// ---------------------------------------------------------------------
// Subcommand bodies
// ---------------------------------------------------------------------

fn run_list(app: &App, opts: &ListOpts) -> miette::Result<()> {
    let (index, _pages) = load_docs(&app.assets, &opts.root).into_diagnostic()?;
    println!("{}", index.title);
    for section in &index.sections {
        println!("\n# {}", section.title);
        for page in &section.pages {
            println!("  {} — {}", page.path, page.title);
        }
    }
    Ok(())
}

fn run_show(app: &App, opts: &ShowOpts) -> miette::Result<()> {
    let (_index, pages) = load_docs(&app.assets, &opts.root).into_diagnostic()?;
    let body = pages
        .get(&opts.path)
        .ok_or_else(|| miette!("page not found in docs tree: {}", opts.path))?;
    match opts.format.as_str() {
        "html" => println!("{}", to_html_document(&opts.path, body)),
        // "plain" is the default (validated by clap's value_parser).
        _ => println!("{}", to_plain_text(body)),
    }
    Ok(())
}

fn run_browse(app: &App, opts: &BrowseOpts) -> miette::Result<()> {
    let (index, pages) = load_docs(&app.assets, &opts.root).into_diagnostic()?;
    let mut browser = crate::DocsBrowser::new(index, pages).into_diagnostic()?;
    run_event_loop(&mut browser).into_diagnostic()
}

async fn run_serve(app: App, opts: ServeOpts) -> miette::Result<()> {
    let (index, pages) = load_docs(&app.assets, &opts.root).into_diagnostic()?;
    let server = DocsServer::new(index, pages).into_diagnostic()?;
    let cancel = app.shutdown.child_token();
    let (bound_tx, bound_rx) = tokio::sync::oneshot::channel();
    let serve_task = tokio::spawn(server.run(opts.bind, bound_tx, cancel));
    if let Ok(addr) = bound_rx.await {
        println!("docs server listening on http://{addr}");
        println!("press Ctrl+C to stop");
    }
    match serve_task.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(miette!("docs server: {e}")),
        Err(e) => Err(miette!("docs server task panicked: {e}")),
    }
}

fn run_ask(_app: &App, _opts: &AskOpts) -> miette::Result<()> {
    // The trait seam lives in `crate::ai` behind the `ai` feature.
    // The CLI surface stays unconditional so users discover the
    // command; when the feature is off, fail with a clear pointer.
    Err(crate::error::DocsError::AiDisabled.into())
}

// ---------------------------------------------------------------------
// TUI event loop
// ---------------------------------------------------------------------

fn run_event_loop(browser: &mut crate::DocsBrowser) -> Result<(), crate::error::DocsError> {
    use crossterm::event::{self, Event, KeyEventKind};
    use crossterm::terminal;
    use crossterm::ExecutableCommand;
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;

    terminal::enable_raw_mode().map_err(|e| crate::error::DocsError::Terminal(e.to_string()))?;
    let mut stdout = std::io::stdout();
    stdout
        .execute(terminal::EnterAlternateScreen)
        .map_err(|e| crate::error::DocsError::Terminal(e.to_string()))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| crate::error::DocsError::Terminal(e.to_string()))?;

    let result = (|| -> Result<(), crate::error::DocsError> {
        loop {
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    let buf = frame.buffer_mut();
                    browser.render(area, buf);
                })
                .map_err(|e| crate::error::DocsError::Terminal(e.to_string()))?;
            if browser.quit_requested() {
                return Ok(());
            }
            // 100ms tick keeps the loop responsive even if the user
            // never presses a key.
            if event::poll(std::time::Duration::from_millis(100))
                .map_err(|e| crate::error::DocsError::Terminal(e.to_string()))?
            {
                if let Event::Key(k) =
                    event::read().map_err(|e| crate::error::DocsError::Terminal(e.to_string()))?
                {
                    if k.kind == KeyEventKind::Press {
                        if let Some(code) = map_key(k.code) {
                            browser.handle_key(code);
                        }
                    }
                }
            }
        }
    })();

    let _ = std::io::stdout().execute(terminal::LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();
    result
}

const fn map_key(code: crossterm::event::KeyCode) -> Option<crate::browser::KeyCode> {
    use crossterm::event::KeyCode as Ck;
    Some(match code {
        Ck::Char(c) => crate::browser::KeyCode::Char(c),
        Ck::Up => crate::browser::KeyCode::Up,
        Ck::Down => crate::browser::KeyCode::Down,
        Ck::Enter => crate::browser::KeyCode::Enter,
        Ck::Tab => crate::browser::KeyCode::Tab,
        Ck::Backspace => crate::browser::KeyCode::Backspace,
        Ck::Esc => crate::browser::KeyCode::Esc,
        _ => return None,
    })
}
