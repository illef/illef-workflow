pub mod app;
pub mod client;
pub mod ui;

use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::time::interval;

use crate::tui::app::{App, Panel};

pub async fn run() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
    let mut app = App::new().await?;

    app.refresh_workflows().await?;
    if !app.workflows.is_empty() {
        app.refresh_executions().await?;
        if !app.executions.is_empty() {
            app.select_execution(0);
        }
    }

    let mut refresh_interval = interval(Duration::from_secs(5));
    let mut log_poll_interval = interval(Duration::from_millis(500));

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        tokio::select! {
            _ = refresh_interval.tick() => {
                let _ = app.refresh_workflows().await;
                let _ = app.refresh_executions().await;
            }
            _ = log_poll_interval.tick() => {
                app.poll_log_updates();
            }
            _ = tokio::task::spawn_blocking(|| {
                event::poll(Duration::from_millis(100))
            }) => {
                if event::poll(Duration::from_millis(0))? {
                    if let Event::Key(key) = event::read()? {
                        handle_key(&mut app, key.code, key.modifiers).await?;
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

async fn handle_key(app: &mut App, key: KeyCode, _modifiers: KeyModifiers) -> Result<()> {
    match key {
        KeyCode::Char('q') => {
            app.should_quit = true;
        }
        KeyCode::Char('w') => {
            app.active_panel = Panel::Workflows;
        }
        KeyCode::Tab => {
            app.active_panel = match app.active_panel {
                Panel::Workflows => Panel::Executions,
                Panel::Executions => Panel::Log,
                Panel::Log => Panel::Workflows,
            };
        }
        KeyCode::Up | KeyCode::Char('k') => match app.active_panel {
            Panel::Workflows => {
                app.move_workflow_up();
                let _ = app.refresh_executions().await;
                if !app.executions.is_empty() {
                    app.select_execution(0);
                }
            }
            Panel::Executions => {
                app.move_execution_up();
            }
            Panel::Log => {
                app.scroll_log_up();
            }
        },
        KeyCode::Down | KeyCode::Char('j') => match app.active_panel {
            Panel::Workflows => {
                app.move_workflow_down();
                let _ = app.refresh_executions().await;
                if !app.executions.is_empty() {
                    app.select_execution(0);
                }
            }
            Panel::Executions => {
                app.move_execution_down();
            }
            Panel::Log => {
                app.scroll_log_down();
            }
        },
        KeyCode::Right => match app.active_panel {
            Panel::Workflows => {
                let _ = app.refresh_executions().await;
                app.active_panel = Panel::Executions;
            }
            _ => {}
        },
        KeyCode::Left => match app.active_panel {
            Panel::Executions => {
                app.active_panel = Panel::Workflows;
            }
            _ => {}
        },
        KeyCode::Enter => match app.active_panel {
            Panel::Workflows => {
                let _ = app.refresh_executions().await;
                app.active_panel = Panel::Executions;
            }
            Panel::Executions => {
                let idx = app.selected_execution;
                app.select_execution(idx);
                app.active_panel = Panel::Log;
            }
            Panel::Log => {}
        },
        KeyCode::Char('r') => {
            app.status_message = String::new();
            if let Err(e) = app.trigger_selected_workflow().await {
                app.status_message = format!("Error: {}", e);
            }
        }
        _ => {}
    }
    Ok(())
}
