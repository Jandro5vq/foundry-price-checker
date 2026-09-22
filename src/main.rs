mod api;
mod app;
mod arena;
mod export;
mod matching;
mod model;
mod openrouter;
mod options;
mod ui;

use std::io::stdout;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::Terminal;

use app::App;

#[derive(Parser, Debug)]
#[command(
    name = "foundry-price-tui",
    about = "Tiny TUI to search, check and sort Azure Foundry model prices"
)]
struct Cli {
    /// Azure region to price (defaults to spaincentral).
    #[arg(long)]
    region: Option<String>,
    /// Currency code (defaults to EUR).
    #[arg(long)]
    currency: Option<String>,
    /// serviceName to include (repeatable). Defaults to all curated services.
    #[arg(long = "service")]
    services: Vec<String>,
    /// Print the price table to stdout and exit (no TUI).
    #[arg(long)]
    dump: bool,
    /// Do not fetch LMArena intelligence (Elo) ratings.
    #[arg(long)]
    no_arena: bool,
    /// Do not fetch context window / capabilities from OpenRouter.
    #[arg(long)]
    no_caps: bool,
}

struct TerminalGuard;

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen);
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let region = cli
        .region
        .unwrap_or_else(|| options::DEFAULT_REGION.to_string());
    let currency = cli
        .currency
        .unwrap_or_else(|| options::DEFAULT_CURRENCY.to_string());
    let services = if cli.services.is_empty() {
        options::SERVICES.iter().map(|s| s.to_string()).collect()
    } else {
        cli.services.clone()
    };

    if cli.dump {
        return dump(&region, &currency, &services, !cli.no_arena, !cli.no_caps);
    }

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen);
        original_hook(info);
    }));

    let _guard = TerminalGuard::new()?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(region, currency, cli.services, !cli.no_arena, !cli.no_caps);

    let (tx, rx) = mpsc::channel();
    app.refresh(&tx);

    run(&mut terminal, &mut app, &rx, &tx)
}

fn dump(
    region: &str,
    currency: &str,
    services: &[String],
    arena_enabled: bool,
    caps_enabled: bool,
) -> Result<()> {
    let items = api::fetch_items(region, currency, services, &|_, _| {})?;
    let (mut rows, other) = model::build_rows(&items, None);
    if arena_enabled {
        match arena::fetch_blocking() {
            Ok(entries) => {
                let index = arena::ArenaIndex::new(entries);
                eprintln!("arena: {} overall models", index.len());
                for row in &mut rows {
                    row.arena = index.lookup(&row.model, &row.product);
                }
            }
            Err(e) => eprintln!("arena unavailable: {e:#}"),
        }
    }
    if caps_enabled {
        match openrouter::fetch_blocking() {
            Ok(entries) => {
                let index = openrouter::CapsIndex::new(entries);
                eprintln!("caps: {} models", index.len());
                for row in &mut rows {
                    row.caps = index.lookup(&row.model, &row.product);
                }
            }
            Err(e) => eprintln!("capabilities unavailable: {e:#}"),
        }
    }
    println!(
        "model,developer,deployment,mode,input_per_1M,cached_input_per_1M,output_per_1M,elo,context_window,capabilities,caps_source,arena_model,product"
    );
    for r in &rows {
        let flags = r
            .caps
            .as_ref()
            .map(|c| {
                let mut f = String::new();
                for (on, ch) in [
                    (c.vision, 'V'),
                    (c.tools, 'T'),
                    (c.reasoning, 'R'),
                    (c.audio, 'A'),
                    (c.json, 'J'),
                ] {
                    if on {
                        f.push(ch);
                    }
                }
                f
            })
            .unwrap_or_default();
        println!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{}",
            r.model,
            r.developer,
            r.deployment,
            r.mode,
            export::fmt_value(r.input),
            export::fmt_value(r.cached),
            export::fmt_value(r.output),
            r.arena
                .as_ref()
                .map(|a| format!("{:.0}", a.rating))
                .unwrap_or_default(),
            r.caps
                .as_ref()
                .map(|c| c.context.to_string())
                .unwrap_or_default(),
            flags,
            r.caps
                .as_ref()
                .map(|c| c.source.clone())
                .unwrap_or_default(),
            r.arena.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
            r.product
        );
    }
    eprintln!(
        "{} items, {} rows, {} skipped",
        items.len(),
        rows.len(),
        other
    );
    Ok(())
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    rx: &mpsc::Receiver<api::FetchMsg>,
    tx: &mpsc::Sender<api::FetchMsg>,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key, tx),
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollDown => app.on_scroll(1),
                    MouseEventKind::ScrollUp => app.on_scroll(-1),
                    _ => {}
                },
                _ => {}
            }
        }

        while let Ok(msg) = rx.try_recv() {
            app.on_fetch(msg);
        }

        app.tick = app.tick.wrapping_add(1);
        if app.should_quit {
            break;
        }
    }
    Ok(())
}
