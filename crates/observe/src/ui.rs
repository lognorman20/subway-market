//! Slim two-panel TUI — upcoming trains and history.

use std::io::{stdout, Stdout};
use std::time::Duration;

use anyhow::Context;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use db::queries::TrackedTrainRow;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::AsyncPgConnection;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::{Frame, Terminal};
use tracing::warn;

use crate::queries;
use crate::format::{format_age, format_diff, format_ny_short, format_train};

const DEFAULT_LIMIT: i64 = 200;
const TICK_MS: u64 = 2_000;

struct AppState {
    upcoming: Vec<TrackedTrainRow>,
    history: Vec<TrackedTrainRow>,
    load_error: Option<String>,
}

pub async fn run(pool: Pool<AsyncPgConnection>) -> anyhow::Result<()> {
    enable_raw_mode().context("enable raw mode")?;
    stdout().execute(EnterAlternateScreen).context("alt screen")?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend).context("terminal")?;

    let result = run_loop(&mut terminal, &pool).await;

    disable_raw_mode().context("disable raw mode")?;
    stdout()
        .execute(LeaveAlternateScreen)
        .context("leave alt screen")?;
    terminal.show_cursor().context("show cursor")?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<Stdout>>,
    pool: &Pool<AsyncPgConnection>,
) -> anyhow::Result<()> {
    let mut state = refresh(pool).await;

    loop {
        terminal.draw(|f| render(f, &state))?;

        if event::poll(Duration::from_millis(TICK_MS))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    break;
                }
            }
        } else {
            state = refresh(pool).await;
        }
    }

    Ok(())
}

async fn refresh(pool: &Pool<AsyncPgConnection>) -> AppState {
    let mut load_error = None;
    let mut upcoming = Vec::new();
    let mut history = Vec::new();

    match pool.get().await {
        Ok(mut conn) => {
            match queries::load_upcoming(&mut conn, DEFAULT_LIMIT).await {
                Ok(rows) => upcoming = rows,
                Err(err) => {
                    warn!(error = %err, "load_upcoming failed");
                    load_error = Some(format!("upcoming: {err}"));
                }
            }

            match queries::load_history(&mut conn, DEFAULT_LIMIT).await {
                Ok(rows) => history = rows,
                Err(err) => {
                    warn!(error = %err, "load_history failed");
                    let msg = format!("history: {err}");
                    load_error = Some(match load_error {
                        Some(prev) => format!("{prev}; {msg}"),
                        None => msg,
                    });
                }
            }
        }
        Err(err) => {
            warn!(error = %err, "postgres pool get failed");
            load_error = Some(format!("db pool: {err}"));
        }
    }

    AppState {
        upcoming,
        history,
        load_error,
    }
}

fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if state.load_error.is_some() { 3 } else { 2 }),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);

    render_header(frame, chunks[0], state);
    render_upcoming(frame, chunks[1], &state.upcoming);
    render_history(frame, chunks[2], &state.history);
}

fn render_header(frame: &mut Frame, area: Rect, state: &AppState) {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!(
                "{} upcoming · {} history · refresh {}s · q quit",
                state.upcoming.len(),
                state.history.len(),
                TICK_MS / 1000,
            ),
            Style::default().fg(Color::Gray),
        ),
    ])];

    if let Some(err) = &state.load_error {
        lines.push(Line::from(Span::styled(err.clone(), Style::default().fg(Color::Red))));
    }

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" OBSERVE ")),
        area,
    );
}

fn render_upcoming(frame: &mut Frame, area: Rect, rows: &[TrackedTrainRow]) {
    let header = Row::new(vec!["Route", "Train", "Target", "ETA", "Rem", "Phase", "Age"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let table_rows: Vec<Row> = rows
        .iter()
        .map(|r| {
            Row::new(vec![
                Cell::from(r.route_id.clone()),
                Cell::from(format_train(&r.route_id, r.nyct_train_id.as_deref())),
                Cell::from(r.target_stop_id.clone().unwrap_or_else(|| "—".into())),
                Cell::from(format_ny_short(r.expected_arrival_unix)),
                Cell::from(
                    r.remaining_stops
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "—".into()),
                ),
                Cell::from(r.phase.clone()),
                Cell::from(format_age(r.updated_at)),
            ])
        })
        .collect();

    let table = Table::new(
        table_rows,
        [
            Constraint::Length(6),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(4),
            Constraint::Length(10),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" UPCOMING (watching / frozen) "),
    );

    frame.render_widget(table, area);
}

fn render_history(frame: &mut Frame, area: Rect, rows: &[TrackedTrainRow]) {
    let header = Row::new(vec!["Route", "Train", "Target", "Diff", "Phase", "Age"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let table_rows: Vec<Row> = rows
        .iter()
        .map(|r| {
            let diff_style = match r.diff_seconds {
                Some(s) if s >= 30 => Style::default().fg(Color::Red),
                Some(s) if s <= -30 => Style::default().fg(Color::Cyan),
                _ => Style::default(),
            };
            Row::new(vec![
                Cell::from(r.route_id.clone()),
                Cell::from(format_train(&r.route_id, r.nyct_train_id.as_deref())),
                Cell::from(r.target_stop_id.clone().unwrap_or_else(|| "—".into())),
                Cell::from(Span::styled(
                    format_diff(r.diff_seconds),
                    diff_style,
                )),
                Cell::from(r.phase.clone()),
                Cell::from(format_age(r.updated_at)),
            ])
        })
        .collect();

    let table = Table::new(
        table_rows,
        [
            Constraint::Length(6),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" HISTORY (arrived / acted / diff) "),
    );

    frame.render_widget(table, area);
}
