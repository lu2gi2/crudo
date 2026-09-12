use crossterm::cursor::Show;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::stdout;
use tui_crudo::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal safely with panic hook for robust cleanup
    setup_panic_hook();
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        crossterm::style::Print("\x1b[?1003l")
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize CRUDO application with compiled character-based logo
    let use_demo = std::env::args().any(|a| a == "--demo") || std::env::var("CRUDO_DEMO").is_ok();
    let backend: Option<tui_crudo::SharedBackend> = if use_demo {
        Some(std::sync::Arc::new(tui_crudo::DemoBackend::new()))
    } else {
        None
    };
    let mut app = App::new(backend, "");

    // Run main event loop
    let app_result = app.run(&mut terminal).await;

    // Terminal restoration
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        Show
    )?;
    terminal.show_cursor()?;

    if let Err(err) = app_result {
        eprintln!("CRUDO application encountered an error: {err}");
    }

    Ok(())
}

fn setup_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            std::io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            Show
        );
        default_hook(panic_info);
    }));
}
