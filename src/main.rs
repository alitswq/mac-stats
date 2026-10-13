use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use mac_stats::ai::{self, AiMonitor, AiSnapshot};
use mac_stats::{DEVELOPER, RELEASE, Sampler, View, draw_with_ai, report};

const HELP: &str = "\
Live CPU, memory, disk, and network stats.

  mac-stats            open the terminal UI
  mac-stats --once     print one sample and exit
  mac-stats --version  show the release version and build number
  mac-stats --help     show this help and developer credit
  mac-stats --ai-once  print AI allowance and usage, then exit
  mac-stats --install-claude-feed  set up the Claude Code usage feed
  mac-stats --claude-statusline   export Claude status-line JSON from stdin

Keys: 5 essentials, 1 RAM, 2 CPU, 3 disk, 4 network, 6 AI.
      Tab or the arrow keys switch views. q quits.
";

fn help() -> String {
    format!("{RELEASE}\nDeveloped by {DEVELOPER}\n\n{HELP}")
}

fn main() -> io::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--help") | Some("-h") => {
            println!("{}", help());
            Ok(())
        }
        Some("--version") | Some("-V") => {
            println!("{RELEASE}");
            Ok(())
        }
        Some("--once") => print_once(),
        Some("--ai-once") => {
            let snapshot = ai::collect_once();
            println!("{}", ai::report(&snapshot, ai::now_unix()));
            Ok(())
        }
        Some("--claude-statusline") => {
            let output = ai::export_claude_reader(
                io::stdin().lock(),
                &ai::claude_usage_path()?,
                ai::now_unix(),
            )?;
            println!("{output}");
            Ok(())
        }
        Some("--install-claude-feed") => {
            let directory = std::env::var_os("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claude"))
                })
                .ok_or_else(|| io::Error::other("Claude settings directory unavailable"))?;
            ai::install_claude_feed(&directory.join("settings.json"), &std::env::current_exe()?)?;
            println!(
                "Claude Code usage feed configured. Restart Claude Code; quota data appears after its next response."
            );
            Ok(())
        }
        Some(other) => {
            eprintln!("unknown argument: {other}\n\n{}", help());
            std::process::exit(2);
        }
        None => ratatui::run(run),
    }
}

fn print_once() -> io::Result<()> {
    let mut sampler = Sampler::new();
    sampler.sample();
    std::thread::sleep(Duration::from_secs(1));
    let snapshot = sampler.sample();
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{}", report(&snapshot))?;
    Ok(())
}

fn run(terminal: &mut ratatui::DefaultTerminal) -> io::Result<()> {
    terminal.draw(|frame| {
        frame.render_widget("Reading system counters…", frame.area());
    })?;
    let mut sampler = Sampler::new();
    let mut snapshot = sampler.sample();
    let mut view = View::Essentials;
    let mut last_sample = Instant::now();
    let mut ai_monitor = None;
    let mut ai_snapshot = AiSnapshot::default();
    let mut ai_started = false;

    loop {
        if view == View::Ai && !ai_started {
            ai_started = true;
            match AiMonitor::start() {
                Ok(monitor) => ai_monitor = Some(monitor),
                Err(_) => {
                    ai_snapshot.codex.message =
                        Some("AI collector could not start. Try mac-stats --ai-once.".into());
                    ai_snapshot.claude.message = ai_snapshot.codex.message.clone();
                }
            }
        }
        if let Some(monitor) = &mut ai_monitor {
            ai_snapshot = monitor.snapshot();
        }
        terminal
            .draw(|frame| draw_with_ai(frame, &snapshot, view, &ai_snapshot, ai::now_unix()))?;
        let timeout = Duration::from_secs(1)
            .saturating_sub(last_sample.elapsed())
            .min(Duration::from_millis(200));
        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
            && handle_key(&mut view, key.code, key.modifiers)
        {
            return Ok(());
        }
        if last_sample.elapsed() >= Duration::from_secs(1) {
            snapshot = sampler.sample();
            last_sample = Instant::now();
        }
    }
}

fn handle_key(view: &mut View, code: KeyCode, modifiers: KeyModifiers) -> bool {
    if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c' | 'C')) {
        return true;
    }
    match code {
        KeyCode::Char('q') | KeyCode::Esc => true,
        KeyCode::Char(key) => {
            if let Some(next) = View::from_key(key) {
                *view = next;
            }
            false
        }
        KeyCode::Left | KeyCode::BackTab => {
            *view = view.previous();
            false
        }
        KeyCode::Right | KeyCode::Tab => {
            *view = view.next();
            false
        }
        _ => false,
    }
}
