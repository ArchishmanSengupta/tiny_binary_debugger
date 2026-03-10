use crate::stats::TraceStats;
use crate::storage::{TraceDb, TraceEntry};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{prelude::*, widgets::*};
use std::collections::HashMap;
use std::io::stdout;
use std::time::Duration;

enum Mode {
    Normal,
    Search(String),
    Help,
}

struct App {
    trace: Vec<TraceEntry>,
    current: usize,
    list_state: ListState,
    prev_regs: Option<HashMap<String, serde_json::Value>>,
    mode: Mode,
    stats: TraceStats,
    arch: String,
    base_sp: Option<u64>,
    should_quit: bool,
    filtered_indices: Vec<usize>,
    trace_file: String,
}

impl App {
    fn new(db: &TraceDb, trace_file: &str) -> Self {
        let trace = db.get_all();
        let stats = TraceStats::analyze(db);

        let arch = if !trace.is_empty() {
            let regs: serde_json::Value = serde_json::from_str(&trace[0].regs).unwrap_or_default();
            if regs.get("rax").is_some() {
                "x86_64".to_string()
            } else {
                "arm64".to_string()
            }
        } else {
            "unknown".to_string()
        };

        let filtered_indices: Vec<usize> = (0..trace.len()).collect();
        let mut list_state = ListState::default();
        if !trace.is_empty() {
            list_state.select(Some(0));
        }

        let base_sp = if !trace.is_empty() {
            let regs: serde_json::Value = serde_json::from_str(&trace[0].regs).unwrap_or_default();
            regs.get("sp").or(regs.get("rsp")).and_then(|v| v.as_u64())
        } else {
            None
        };

        Self {
            trace,
            current: 0,
            list_state,
            prev_regs: None,
            mode: Mode::Normal,
            stats,
            arch,
            base_sp,
            should_quit: false,
            filtered_indices,
            trace_file: trace_file.to_string(),
        }
    }

    fn select(&mut self, idx: usize) {
        if idx >= self.trace.len() {
            return;
        }
        // Save current regs as previous for diff highlighting
        if self.current < self.trace.len() {
            if let Ok(regs) =
                serde_json::from_str::<serde_json::Value>(&self.trace[self.current].regs)
            {
                if let Some(obj) = regs.as_object() {
                    self.prev_regs = Some(obj.clone().into_iter().collect());
                }
            }
        }
        self.current = idx;
        // Sync list selection
        if let Some(pos) = self.filtered_indices.iter().position(|&i| i == idx) {
            self.list_state.select(Some(pos));
        }
    }

    fn step(&mut self, delta: i64) {
        let new = (self.current as i64 + delta)
            .max(0)
            .min(self.trace.len() as i64 - 1) as usize;
        self.select(new);
    }

    fn find_next(&mut self, what: &str) {
        for i in (self.current + 1)..self.trace.len() {
            if self.matches_filter(&self.trace[i], what) {
                self.select(i);
                return;
            }
        }
    }

    fn find_prev(&mut self, what: &str) {
        if self.current == 0 {
            return;
        }
        for i in (0..self.current).rev() {
            if self.matches_filter(&self.trace[i], what) {
                self.select(i);
                return;
            }
        }
    }

    fn matches_filter(&self, entry: &TraceEntry, what: &str) -> bool {
        match what {
            "call" => entry.insn_text.contains("CALL"),
            "ret" => entry.insn_text.contains("RETURN"),
            "mem" => !entry.mem_changes.is_empty(),
            _ => false,
        }
    }

    fn apply_search(&mut self, query: &str) {
        let q = query.to_lowercase();
        self.filtered_indices = if q.is_empty() {
            (0..self.trace.len()).collect()
        } else {
            self.trace
                .iter()
                .enumerate()
                .filter(|(_, e)| e.insn_text.to_lowercase().contains(&q))
                .map(|(i, _)| i)
                .collect()
        };
        if let Some(&first) = self.filtered_indices.first() {
            self.select(first);
        }
    }

    fn current_entry(&self) -> Option<&TraceEntry> {
        self.trace.get(self.current)
    }

    fn current_regs(&self) -> HashMap<String, serde_json::Value> {
        self.current_entry()
            .and_then(|e| serde_json::from_str::<serde_json::Value>(&e.regs).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
            .into_iter()
            .collect()
    }
}

pub fn run(trace_file: &str) -> Result<(), String> {
    let db = TraceDb::load(trace_file).map_err(|e| format!("Failed to load trace: {}", e))?;

    if db.count() == 0 {
        return Err("Trace file is empty".to_string());
    }

    let mut app = App::new(&db, trace_file);

    enable_raw_mode().map_err(|e| e.to_string())?;
    stdout()
        .execute(EnterAlternateScreen)
        .map_err(|e| e.to_string())?;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;

    let result = main_loop(&mut terminal, &mut app);

    disable_raw_mode().map_err(|e| e.to_string())?;
    stdout()
        .execute(LeaveAlternateScreen)
        .map_err(|e| e.to_string())?;

    result
}

fn main_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<(), String> {
    loop {
        terminal.draw(|f| ui(f, app)).map_err(|e| e.to_string())?;

        if event::poll(Duration::from_millis(50)).map_err(|e| e.to_string())? {
            if let Event::Key(key) = event::read().map_err(|e| e.to_string())? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match &app.mode {
                    Mode::Normal => handle_normal_key(app, key.code),
                    Mode::Search(_) => handle_search_key(app, key.code),
                    Mode::Help => {
                        app.mode = Mode::Normal;
                    }
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_normal_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('h') | KeyCode::Left => app.step(-1),
        KeyCode::Char('l') | KeyCode::Right => app.step(1),
        KeyCode::Char('j') | KeyCode::Down => app.step(1),
        KeyCode::Char('k') | KeyCode::Up => app.step(-1),
        KeyCode::Char('g') | KeyCode::Home => app.select(0),
        KeyCode::Char('G') | KeyCode::End => {
            app.select(app.trace.len().saturating_sub(1));
        }
        KeyCode::Char('c') => app.find_next("call"),
        KeyCode::Char('C') => app.find_prev("call"),
        KeyCode::Char('r') => app.find_next("ret"),
        KeyCode::Char('R') => app.find_prev("ret"),
        KeyCode::Char('m') => app.find_next("mem"),
        KeyCode::Char('M') => app.find_prev("mem"),
        KeyCode::Char('/') => app.mode = Mode::Search(String::new()),
        KeyCode::Char('?') => app.mode = Mode::Help,
        KeyCode::PageDown => app.step(50),
        KeyCode::PageUp => app.step(-50),
        _ => {}
    }
}

fn handle_search_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            // Cancel search, restore full list
            app.filtered_indices = (0..app.trace.len()).collect();
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            app.mode = Mode::Normal;
        }
        KeyCode::Char(c) => {
            if let Mode::Search(ref mut q) = app.mode {
                q.push(c);
                let query = q.clone();
                app.apply_search(&query);
            }
        }
        KeyCode::Backspace => {
            if let Mode::Search(ref mut q) = app.mode {
                q.pop();
                let query = q.clone();
                app.apply_search(&query);
            }
        }
        _ => {}
    }
}

// ──────────────────────────── rendering ────────────────────────────

fn ui(f: &mut Frame, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(f.area());

    render_header(f, app, outer[0]);
    render_main(f, app, outer[1]);
    render_footer(f, app, outer[2]);

    if let Mode::Help = app.mode {
        render_help(f);
    }
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let pct = if !app.trace.is_empty() {
        (app.current * 100) / app.trace.len()
    } else {
        0
    };

    let text = Line::from(vec![
        Span::styled(
            " TDB ",
            Style::default().fg(Color::Black).bg(Color::White).bold(),
        ),
        Span::raw("  "),
        Span::styled(&app.trace_file, Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(
            format!("{} steps", app.trace.len()),
            Style::default().fg(Color::White),
        ),
        Span::styled("  |  ", Style::default().fg(Color::DarkGray)),
        Span::styled(app.arch.to_uppercase(), Style::default().fg(Color::Cyan)),
        Span::styled("  |  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("Step {}/{}", app.current, app.trace.len()),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!("  ({}%)", pct),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled("  |  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} calls", app.stats.call_count),
            Style::default().fg(Color::Blue),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} mem", app.stats.mem_change_count),
            Style::default().fg(Color::Yellow),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let header = Paragraph::new(text).block(block);
    f.render_widget(header, area);
}

fn render_main(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(35),
            Constraint::Percentage(35),
        ])
        .split(area);

    render_timeline(f, app, cols[0]);
    render_detail(f, app, cols[1]);
    render_right(f, app, cols[2]);
}

fn render_timeline(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .filtered_indices
        .iter()
        .map(|&i| {
            let e = &app.trace[i];
            let is_call = e.insn_text.contains("CALL");
            let is_ret = e.insn_text.contains("RETURN");
            let has_mem = !e.mem_changes.is_empty();

            let marker = if is_call {
                ">"
            } else if is_ret {
                "<"
            } else if has_mem {
                "*"
            } else {
                " "
            };

            let style = if i == app.current {
                Style::default().fg(Color::White).bg(Color::DarkGray)
            } else if is_call {
                Style::default().fg(Color::Blue)
            } else if is_ret {
                Style::default().fg(Color::Red)
            } else if has_mem {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Gray)
            };

            // Truncate instruction text to fit
            let max_insn = 22;
            let insn = &e.insn_text;
            let insn_short = if insn.len() > max_insn {
                format!("{}..", &insn[..max_insn - 2])
            } else {
                insn.clone()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", marker), style),
                Span::styled(format!("{:>6} ", e.step), style.fg(Color::DarkGray)),
                Span::styled(format!("{:>12x} ", e.pc), style.fg(Color::DarkGray)),
                Span::styled(insn_short, style),
            ]))
        })
        .collect();

    let title = format!(
        " Timeline ({}{}) ",
        app.filtered_indices.len(),
        if app.filtered_indices.len() != app.trace.len() {
            format!("/{}", app.trace.len())
        } else {
            String::new()
        }
    );

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(Style::default().fg(Color::White).bg(Color::DarkGray).bold())
        .highlight_symbol("> ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_detail(f: &mut Frame, app: &App, area: Rect) {
    let entry = match app.current_entry() {
        Some(e) => e,
        None => {
            let empty = Paragraph::new("  No trace data loaded")
                .style(Style::default().fg(Color::DarkGray))
                .block(
                    Block::default()
                        .title(" Instruction ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray)),
                );
            f.render_widget(empty, area);
            return;
        }
    };

    let regs: serde_json::Value = serde_json::from_str(&entry.regs).unwrap_or_default();
    let sp = regs
        .get("sp")
        .or(regs.get("rsp"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let delta = app.base_sp.map(|b| sp as i64 - b as i64).unwrap_or(0);

    let insn_type = if entry.insn_text.contains("CALL") {
        ("CALL", Color::Blue)
    } else if entry.insn_text.contains("RETURN") {
        ("RETURN", Color::Red)
    } else {
        let m = entry.insn_text.split_whitespace().next().unwrap_or("");
        if m.starts_with('b') || m.starts_with('j') {
            ("BRANCH", Color::Magenta)
        } else if m.contains("str") || m.contains("st") || m == "push" {
            ("STORE", Color::Yellow)
        } else if m.contains("ldr") || m.contains("ld") || m == "pop" {
            ("LOAD", Color::Cyan)
        } else {
            ("", Color::DarkGray)
        }
    };

    let delta_color = if delta < 0 {
        Color::Red
    } else if delta > 0 {
        Color::Green
    } else {
        Color::DarkGray
    };

    let bytes_str = entry
        .insn_bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ");

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  PC   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("0x{:016X}", entry.pc),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("  {}", entry.insn_text),
            Style::default().fg(Color::White).bold(),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Bytes  ", Style::default().fg(Color::DarkGray)),
            Span::styled(bytes_str, Style::default().fg(Color::Gray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Step   ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}", entry.step), Style::default().fg(Color::White)),
            Span::styled(
                format!("  ({}/{})", app.current + 1, app.trace.len()),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled("  SP     ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("0x{:X}", sp), Style::default().fg(Color::White)),
            Span::styled(
                format!("  ({}{})", if delta >= 0 { "+" } else { "" }, delta),
                Style::default().fg(delta_color),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Type   ", Style::default().fg(Color::DarkGray)),
            Span::styled(insn_type.0, Style::default().fg(insn_type.1)),
        ]),
    ];

    let block = Block::default()
        .title(" Instruction ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let detail = Paragraph::new(lines).block(block);
    f.render_widget(detail, area);
}

fn render_right(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(area);

    render_registers(f, app, chunks[0]);
    render_memory(f, app, chunks[1]);
}

fn render_registers(f: &mut Frame, app: &App, area: Rect) {
    let regs = app.current_regs();
    let prev = app.prev_regs.as_ref();

    // Sort registers for consistent display
    let mut names: Vec<&String> = regs.keys().collect();
    names.sort_by_key(|a| reg_sort_key(a));

    let mut lines: Vec<Line> = Vec::new();
    for name in &names {
        if let Some(val) = regs.get(*name) {
            let v = val.as_u64().unwrap_or(0);
            let changed = prev
                .and_then(|p| p.get(*name))
                .map(|pv| pv != val)
                .unwrap_or(false);

            let val_style = if changed {
                Style::default().fg(Color::Green).bold()
            } else {
                Style::default().fg(Color::Gray)
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {:>4} ", name),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("0x{:016X}", v), val_style),
            ]));
        }
    }

    let block = Block::default()
        .title(format!(" Registers ({}) ", regs.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let widget = Paragraph::new(lines).block(block);
    f.render_widget(widget, area);
}

fn render_memory(f: &mut Frame, app: &App, area: Rect) {
    let entry = app.current_entry();
    let mut lines: Vec<Line> = Vec::new();

    if let Some(entry) = entry {
        if entry.mem_changes.is_empty() {
            lines.push(Line::from(Span::styled(
                "  no changes",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for c in &entry.mem_changes {
                let ch = if c.new_val >= 0x20 && c.new_val <= 0x7E {
                    c.new_val as char
                } else {
                    '.'
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!(" 0x{:012X} ", c.addr),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        format!("{:02X}", c.old_val),
                        Style::default().fg(Color::Red),
                    ),
                    Span::styled(" -> ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{:02X}", c.new_val),
                        Style::default().fg(Color::Green),
                    ),
                    Span::styled(format!(" '{}'", ch), Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            "  no data",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let count = entry.map(|e| e.mem_changes.len()).unwrap_or(0);
    let block = Block::default()
        .title(format!(" Memory ({}) ", count))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let widget = Paragraph::new(lines).block(block);
    f.render_widget(widget, area);
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let content = match &app.mode {
        Mode::Normal => Line::from(vec![
            Span::styled(" h", Style::default().fg(Color::Cyan)),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled("l", Style::default().fg(Color::Cyan)),
            Span::styled(" step  ", Style::default().fg(Color::DarkGray)),
            Span::styled("g", Style::default().fg(Color::Cyan)),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled("G", Style::default().fg(Color::Cyan)),
            Span::styled(" first/last  ", Style::default().fg(Color::DarkGray)),
            Span::styled("c", Style::default().fg(Color::Cyan)),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled("C", Style::default().fg(Color::Cyan)),
            Span::styled(" call  ", Style::default().fg(Color::DarkGray)),
            Span::styled("r", Style::default().fg(Color::Cyan)),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled("R", Style::default().fg(Color::Cyan)),
            Span::styled(" ret  ", Style::default().fg(Color::DarkGray)),
            Span::styled("m", Style::default().fg(Color::Cyan)),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled("M", Style::default().fg(Color::Cyan)),
            Span::styled(" mem  ", Style::default().fg(Color::DarkGray)),
            Span::styled("/", Style::default().fg(Color::Cyan)),
            Span::styled(" search  ", Style::default().fg(Color::DarkGray)),
            Span::styled("?", Style::default().fg(Color::Cyan)),
            Span::styled(" help  ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::styled(" quit", Style::default().fg(Color::DarkGray)),
        ]),
        Mode::Search(query) => Line::from(vec![
            Span::styled(" Search: ", Style::default().fg(Color::Yellow)),
            Span::styled(query, Style::default().fg(Color::White)),
            Span::styled("_", Style::default().fg(Color::White).slow_blink()),
            Span::styled(
                "  [Enter] confirm  [Esc] cancel",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Mode::Help => Line::from(Span::styled(
            " Press any key to close",
            Style::default().fg(Color::DarkGray),
        )),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let footer = Paragraph::new(content).block(block);
    f.render_widget(footer, area);
}

fn render_help(f: &mut Frame) {
    let area = centered_rect(55, 75, f.area());
    f.render_widget(Clear, area);

    let lines = vec![
        Line::from(Span::styled(
            " TDB Keyboard Shortcuts",
            Style::default().fg(Color::White).bold(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  h / Left       ", Style::default().fg(Color::Cyan)),
            Span::raw("Previous step"),
        ]),
        Line::from(vec![
            Span::styled("  l / Right      ", Style::default().fg(Color::Cyan)),
            Span::raw("Next step"),
        ]),
        Line::from(vec![
            Span::styled("  j / Down       ", Style::default().fg(Color::Cyan)),
            Span::raw("Next step"),
        ]),
        Line::from(vec![
            Span::styled("  k / Up         ", Style::default().fg(Color::Cyan)),
            Span::raw("Previous step"),
        ]),
        Line::from(vec![
            Span::styled("  g / Home       ", Style::default().fg(Color::Cyan)),
            Span::raw("First step"),
        ]),
        Line::from(vec![
            Span::styled("  G / End        ", Style::default().fg(Color::Cyan)),
            Span::raw("Last step"),
        ]),
        Line::from(vec![
            Span::styled("  PgUp / PgDn    ", Style::default().fg(Color::Cyan)),
            Span::raw("Jump 50 steps"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  c / C          ", Style::default().fg(Color::Blue)),
            Span::raw("Next / prev function call"),
        ]),
        Line::from(vec![
            Span::styled("  r / R          ", Style::default().fg(Color::Red)),
            Span::raw("Next / prev return"),
        ]),
        Line::from(vec![
            Span::styled("  m / M          ", Style::default().fg(Color::Yellow)),
            Span::raw("Next / prev memory change"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  /              ", Style::default().fg(Color::Cyan)),
            Span::raw("Search / filter instructions"),
        ]),
        Line::from(vec![
            Span::styled("  ?              ", Style::default().fg(Color::Cyan)),
            Span::raw("Show this help"),
        ]),
        Line::from(vec![
            Span::styled("  q / Esc        ", Style::default().fg(Color::Cyan)),
            Span::raw("Quit"),
        ]),
    ];

    let help = Paragraph::new(lines).block(
        Block::default()
            .title(" Help ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black)),
    );
    f.render_widget(help, area);
}

fn centered_rect(pct_x: u16, pct_y: u16, r: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(vert[1])[1]
}

/// Sort key for register names to get a natural ordering.
fn reg_sort_key(name: &str) -> (u8, u32) {
    // Group: 0=general, 1=special, 2=flags
    if let Some(rest) = name.strip_prefix('x') {
        if let Ok(n) = rest.parse::<u32>() {
            return (0, n);
        }
    }
    if let Some(rest) = name.strip_prefix('r') {
        if let Ok(n) = rest.parse::<u32>() {
            return (0, n);
        }
    }
    match name {
        "rax" => (0, 0),
        "rbx" => (0, 1),
        "rcx" => (0, 2),
        "rdx" => (0, 3),
        "rdi" => (0, 4),
        "rsi" => (0, 5),
        "rbp" => (0, 6),
        "rsp" => (0, 7),
        "rip" | "pc" => (1, 0),
        "fp" => (1, 1),
        "lr" => (1, 2),
        "sp" => (1, 3),
        "rflags" | "cpsr" => (2, 0),
        _ => (1, 99),
    }
}
