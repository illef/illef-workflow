use chrono::{Local, TimeZone, Utc};
use chrono::DateTime;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::tui::app::{App, Panel};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // layout: top (lists) | bottom (log) | status bar
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    // top: workflows | executions
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(vertical[0]);

    draw_workflows(frame, app, top_chunks[0]);
    draw_executions(frame, app, top_chunks[1]);
    draw_log(frame, app, vertical[1]);
    draw_status_bar(frame, app, vertical[2]);
}

fn draw_workflows(frame: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Workflows;
    let border_style = if is_active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let items: Vec<ListItem> = app
        .workflows
        .iter()
        .map(|wf| {
            let (status_icon, icon_style) = match wf.status.as_str() {
                "running" => ("●", Style::default().fg(Color::Green)),
                _ => ("○", Style::default().fg(Color::DarkGray)),
            };

            let countdown = if wf.next_run_at > 0 {
                let next: DateTime<Utc> = Utc.timestamp_opt(wf.next_run_at, 0).unwrap();
                format_countdown(next)
            } else {
                "-".to_string()
            };

            let line1 = Line::from(vec![
                Span::styled(format!("{} ", status_icon), icon_style),
                Span::styled(&wf.name, Style::default().add_modifier(Modifier::BOLD)),
            ]);
            let line2 = Line::from(vec![
                Span::raw("  "),
                Span::styled(&wf.cron, Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                Span::styled(countdown, Style::default().fg(Color::Cyan)),
            ]);

            ListItem::new(vec![line1, line2])
        })
        .collect();

    let mut state = ListState::default();
    state.select(if app.workflows.is_empty() {
        None
    } else {
        Some(app.selected_workflow)
    });

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Workflows ")
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_executions(frame: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Executions;
    let border_style = if is_active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let items: Vec<ListItem> = app
        .executions
        .iter()
        .map(|exec| {
            let (icon, color) = match exec.status.as_str() {
                "success" => ("✓", Color::Green),
                "failed" => ("✗", Color::Red),
                "running" => ("●", Color::Yellow),
                _ => ("?", Color::DarkGray),
            };

            let time = if exec.started_at > 0 {
                let dt = Utc.timestamp_opt(exec.started_at, 0).unwrap().with_timezone(&Local);
                dt.format("%m-%d %H:%M").to_string()
            } else {
                "unknown".to_string()
            };

            let line = Line::from(vec![
                Span::styled(format!("{} ", icon), Style::default().fg(color)),
                Span::raw(time),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default();
    state.select(if app.executions.is_empty() {
        None
    } else {
        Some(app.selected_execution)
    });

    let title = app
        .selected_workflow_name()
        .map(|n| format!(" {} - Executions ", n))
        .unwrap_or_else(|| " Executions ".to_string());

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_log(frame: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Log;
    let border_style = if is_active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let visible_height = area.height.saturating_sub(2) as usize;
    let start = if app.log_lines.len() > visible_height {
        app.log_scroll
            .saturating_sub(visible_height / 2)
            .min(app.log_lines.len().saturating_sub(visible_height))
    } else {
        0
    };

    let visible_lines: Vec<Line> = app
        .log_lines
        .iter()
        .skip(start)
        .take(visible_height)
        .map(|l| Line::from(Span::raw(l.as_str())))
        .collect();

    let log_title = if let Some(exec) = app.executions.get(app.selected_execution) {
        let dt = Utc.timestamp_opt(exec.started_at, 0).unwrap().with_timezone(&Local);
        format!(" Log - {} ", dt.format("%Y-%m-%d %H:%M"))
    } else {
        " Log ".to_string()
    };

    let paragraph = Paragraph::new(visible_lines)
        .block(
            Block::default()
                .title(log_title)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn format_countdown(next: DateTime<Utc>) -> String {
    let secs = (next - Utc::now()).num_seconds();
    if secs <= 0 {
        return "now".to_string();
    }
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("in {}h {}m", h, m)
    } else if m > 0 {
        format!("in {}m {}s", m, s)
    } else {
        format!("in {}s", s)
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let help = if app.status_message.is_empty() {
        "[←→] switch panel  [w] workflows  [↑↓] select  [r] run now  [q] quit"
    } else {
        &app.status_message
    };

    let paragraph = Paragraph::new(help).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(paragraph, area);
}
