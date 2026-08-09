//! Tabbed read-only TUI — trains, markets, trades tape, agent×market PnL.

use std::io::{stdout, Stdout};
use std::time::Duration;

use anyhow::Context;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use db::models::{AgentMarketPnlRow, MarketRow, TradeRow};
use db::queries::TrackedTrainRow;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::AsyncPgConnection;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::{Frame, Terminal};
use tracing::warn;

use crate::format::{
    format_age, format_diff, format_id, format_ny_clock, format_ny_short, format_shares,
    format_train, format_usdc_micro, format_wad, format_wad_opt,
};
use crate::queries;

const DEFAULT_LIMIT: i64 = 200;
const TICK_MS: u64 = 2_000;
const ID_COL: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Trains,
    Markets,
    Trades,
    Pnl,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Trains, Tab::Markets, Tab::Trades, Tab::Pnl];

    fn label(self) -> &'static str {
        match self {
            Self::Trains => "1 trains",
            Self::Markets => "2 markets",
            Self::Trades => "3 trades",
            Self::Pnl => "4 pnl",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Trains => Self::Markets,
            Self::Markets => Self::Trades,
            Self::Trades => Self::Pnl,
            Self::Pnl => Self::Trains,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Trains => Self::Pnl,
            Self::Markets => Self::Trains,
            Self::Trades => Self::Markets,
            Self::Pnl => Self::Trades,
        }
    }

    fn from_digit(c: char) -> Option<Self> {
        match c {
            '1' => Some(Self::Trains),
            '2' => Some(Self::Markets),
            '3' => Some(Self::Trades),
            '4' => Some(Self::Pnl),
            _ => None,
        }
    }
}

struct AppState {
    tab: Tab,
    upcoming: Vec<TrackedTrainRow>,
    history: Vec<TrackedTrainRow>,
    markets: Vec<MarketRow>,
    trades: Vec<TradeRow>,
    pnl: Vec<AgentMarketPnlRow>,
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
    let mut state = refresh(pool, Tab::Trains).await;

    loop {
        terminal.draw(|f| render(f, &state))?;

        if event::poll(Duration::from_millis(TICK_MS))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                        state.tab = state.tab.next();
                    }
                    KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                        state.tab = state.tab.prev();
                    }
                    KeyCode::Char(c) => {
                        if let Some(tab) = Tab::from_digit(c) {
                            state.tab = tab;
                        }
                    }
                    _ => {}
                }
            }
        } else {
            state = refresh(pool, state.tab).await;
        }
    }

    Ok(())
}

async fn refresh(pool: &Pool<AsyncPgConnection>, tab: Tab) -> AppState {
    let mut load_error = None;
    let mut upcoming = Vec::new();
    let mut history = Vec::new();
    let mut markets = Vec::new();
    let mut trades = Vec::new();
    let mut pnl = Vec::new();

    match pool.get().await {
        Ok(mut conn) => {
            push_err(
                &mut load_error,
                "upcoming",
                queries::load_upcoming(&mut conn, DEFAULT_LIMIT)
                    .await
                    .map(|rows| upcoming = rows),
            );
            push_err(
                &mut load_error,
                "history",
                queries::load_history(&mut conn, DEFAULT_LIMIT)
                    .await
                    .map(|rows| history = rows),
            );
            push_err(
                &mut load_error,
                "markets",
                queries::load_markets(&mut conn, DEFAULT_LIMIT)
                    .await
                    .map(|rows| markets = rows),
            );
            push_err(
                &mut load_error,
                "trades",
                queries::load_trades(&mut conn, DEFAULT_LIMIT)
                    .await
                    .map(|rows| trades = rows),
            );
            push_err(
                &mut load_error,
                "pnl",
                queries::load_agent_market_pnl(&mut conn, DEFAULT_LIMIT)
                    .await
                    .map(|rows| pnl = rows),
            );
        }
        Err(err) => {
            warn!(error = %err, "postgres pool get failed");
            load_error = Some(format!("db pool: {err}"));
        }
    }

    AppState {
        tab,
        upcoming,
        history,
        markets,
        trades,
        pnl,
        load_error,
    }
}

fn push_err<T, E: std::fmt::Display>(
    load_error: &mut Option<String>,
    label: &str,
    result: Result<T, E>,
) {
    if let Err(err) = result {
        warn!(error = %err, "{label} query failed");
        let msg = format!("{label}: {err}");
        *load_error = Some(match load_error.take() {
            Some(prev) => format!("{prev}; {msg}"),
            None => msg,
        });
    }
}

fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if state.load_error.is_some() { 3 } else { 2 }),
            Constraint::Min(3),
        ])
        .split(area);

    render_header(frame, chunks[0], state);

    match state.tab {
        Tab::Trains => render_trains(frame, chunks[1], state),
        Tab::Markets => render_markets(frame, chunks[1], &state.markets),
        Tab::Trades => render_trades(frame, chunks[1], &state.trades),
        Tab::Pnl => render_pnl(frame, chunks[1], &state.pnl),
    }
}

fn render_header(frame: &mut Frame, area: Rect, state: &AppState) {
    let tabs: Vec<Span> = Tab::ALL
        .iter()
        .flat_map(|tab| {
            let style = if *tab == state.tab {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            [
                Span::styled(tab.label(), style),
                Span::raw("  "),
            ]
        })
        .collect();

    let counts = match state.tab {
        Tab::Trains => format!(
            "{} upcoming · {} history",
            state.upcoming.len(),
            state.history.len()
        ),
        Tab::Markets => format!("{} markets", state.markets.len()),
        Tab::Trades => format!("{} trades", state.trades.len()),
        Tab::Pnl => format!("{} pnl rows", state.pnl.len()),
    };

    let mut lines = vec![Line::from({
        let mut spans = tabs;
        spans.push(Span::styled(
            format!("· {counts} · refresh {}s · tab/1-4 · q quit", TICK_MS / 1000),
            Style::default().fg(Color::DarkGray),
        ));
        spans
    })];

    if let Some(err) = &state.load_error {
        lines.push(Line::from(Span::styled(
            err.clone(),
            Style::default().fg(Color::Red),
        )));
    }

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" OBSERVE ")),
        area,
    );
}

fn render_trains(frame: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    render_upcoming(frame, chunks[0], &state.upcoming);
    render_history(frame, chunks[1], &state.history);
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
                Cell::from(Span::styled(format_diff(r.diff_seconds), diff_style)),
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

fn render_markets(frame: &mut Frame, area: Rect, rows: &[MarketRow]) {
    let header = Row::new(vec![
        "Market", "Route", "Stop", "Outcome", "ETA", "qEarly", "qLate", "Opened",
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let table_rows: Vec<Row> = rows
        .iter()
        .map(|r| {
            let outcome_style = match r.outcome.as_str() {
                "early" => Style::default().fg(Color::Cyan),
                "late" => Style::default().fg(Color::Red),
                _ => Style::default().fg(Color::Yellow),
            };
            Row::new(vec![
                Cell::from(format_id(&r.market_id, ID_COL)),
                Cell::from(r.route_id.clone()),
                Cell::from(r.stop_id.clone()),
                Cell::from(Span::styled(r.outcome.clone(), outcome_style)),
                Cell::from(format_ny_short(Some(r.frozen_eta_unix))),
                Cell::from(format_shares(&r.q_early)),
                Cell::from(format_shares(&r.q_late)),
                Cell::from(format_ny_clock(r.opened_at)),
            ])
        })
        .collect();

    let table = Table::new(
        table_rows,
        [
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" MARKETS (qty micro-USDC → USDC) "),
    );

    frame.render_widget(table, area);
}

fn render_trades(frame: &mut Frame, area: Rect, rows: &[TradeRow]) {
    let header = Row::new(vec![
        "Filled", "Market", "Agent", "Side", "Act", "Shares", "USDC", "Price",
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let table_rows: Vec<Row> = rows
        .iter()
        .map(|r| {
            let side_style = match r.side.as_str() {
                "early" => Style::default().fg(Color::Cyan),
                "late" => Style::default().fg(Color::Red),
                _ => Style::default(),
            };
            Row::new(vec![
                Cell::from(format_ny_clock(r.filled_at)),
                Cell::from(format_id(&r.market_id, ID_COL)),
                Cell::from(format_id(&r.agent_id, ID_COL)),
                Cell::from(Span::styled(r.side.clone(), side_style)),
                Cell::from(r.action.clone()),
                Cell::from(format_shares(&r.shares)),
                Cell::from(format_usdc_micro(r.usdc_amount)),
                Cell::from(format_wad(&r.price_wad)),
            ])
        })
        .collect();

    let table = Table::new(
        table_rows,
        [
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" TRADES TAPE (USDC micro · price WAD) "),
    );

    frame.render_widget(table, area);
}

fn render_pnl(frame: &mut Frame, area: Rect, rows: &[AgentMarketPnlRow]) {
    let header = Row::new(vec![
        "Agent", "Market", "Early", "Late", "Cost", "Real", "Unreal", "Total", "Res", "Age",
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let table_rows: Vec<Row> = rows
        .iter()
        .map(|r| {
            let total_style = if r.total_pnl_usdc > 0 {
                Style::default().fg(Color::Cyan)
            } else if r.total_pnl_usdc < 0 {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(format_id(&r.agent_id, ID_COL)),
                Cell::from(format_id(&r.market_id, ID_COL)),
                Cell::from(format_shares(&r.early_shares)),
                Cell::from(format_shares(&r.late_shares)),
                Cell::from(format_usdc_micro(r.cost_basis_usdc)),
                Cell::from(format_usdc_micro(r.realized_pnl_usdc)),
                Cell::from(format_usdc_micro(r.unrealized_pnl_usdc)),
                Cell::from(Span::styled(
                    format_usdc_micro(r.total_pnl_usdc),
                    total_style,
                )),
                Cell::from(if r.resolved { "yes" } else { "no" }),
                Cell::from(format_age(r.updated_at)),
            ])
        })
        .collect();

    let table = Table::new(
        table_rows,
        [
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(9),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(4),
            Constraint::Length(5),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(
                " AGENT×MARKET PnL (mark {}) ",
                rows.first()
                    .map(|r| format_wad_opt(r.last_mark_wad.as_deref()))
                    .unwrap_or_else(|| "—".into())
            )),
    );

    frame.render_widget(table, area);
}
