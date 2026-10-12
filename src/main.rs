use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use mac_stats::{Sampler, View, draw, report};

const HELP: &str = "\
mac-stats

Live CPU, memory, disk, and network stats.

  mac-stats          open the terminal UI
  mac-stats --once   print one sample and exit

Keys: 5 essentials, 1 RAM, 2 CPU, 3 disk, 4 network.
      Tab or the arrow keys switch views. q quits.
";

fn main() -> io::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--help") | Some("-h") => {
            println!("{HELP}");
            Ok(())
        }
        Some("--once") => print_once(),
        Some(other) => {
            eprintln!("unknown argument: {other}\n\n{HELP}");
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

    loop {
        terminal.draw(|frame| draw(frame, &snapshot, view))?;
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
