use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Cell, Clear, List, ListItem, ListState, Paragraph, Row as TableRow, Table,
    TableState, Wrap,
};
use ratatui::Frame;

use crate::app::{App, FilterField, Panel, SortCol};
use crate::options;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .split(frame.area());

    draw_status(frame, chunks[0], app);
    draw_search(frame, chunks[1], app);
    draw_body(frame, chunks[2], app);
    draw_footer(frame, chunks[3], app);

    match app.panel {
        Panel::Filter => draw_filter_panel(frame, app),
        Panel::Deployment => draw_selector(frame, app, true),
        Panel::Mode => draw_selector(frame, app, false),
        Panel::Help => draw_help(frame, app),
        Panel::None => {}
    }
}

fn spinner(app: &App) -> &'static str {
    const FRAMES: [&str; 4] = ["|", "/", "-", "\\"];
    if app.loading {
        FRAMES[(app.tick as usize / 2) % FRAMES.len()]
    } else {
        " "
    }
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let sort_arrow = if app.sort_asc { "▲" } else { "▼" };
    let services: Vec<&str> = options::SERVICES
        .iter()
        .enumerate()
        .filter(|(i, _)| app.services.get(*i).copied().unwrap_or(false))
        .map(|(_, s)| {
            if s.starts_with("Foundry") {
                "Foundry"
            } else {
                "Cognitive"
            }
        })
        .collect();
    let line = Line::from(vec![
        Span::styled(
            " Foundry Price Checker ",
            Style::new().bold().fg(Color::Cyan),
        ),
        Span::raw("│ "),
        Span::styled(
            format!("{} / {}", app.region, app.currency),
            Style::new().fg(Color::Yellow),
        ),
        Span::raw(format!(" │ svc: {} │ ", services.join("+"))),
        Span::styled(
            format!("sort: {} {}", app.sort_col.label(), sort_arrow),
            Style::new().fg(Color::Magenta),
        ),
        Span::raw(" │ "),
        Span::styled(spinner(app), Style::new().fg(Color::Green)),
        Span::raw(" "),
        Span::styled(app.status.clone(), Style::new().fg(Color::Gray)),
        Span::raw(" "),
        Span::styled(arena_indicator(app), Style::new().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn arena_indicator(app: &App) -> String {
    let mut parts = Vec::new();
    if app.arena_loading {
        parts.push("Elo…".to_string());
    } else if app.arena_status.is_some() {
        parts.push("Elo n/a".to_string());
    } else if let Some(idx) = &app.arena {
        if !idx.is_empty() {
            parts.push(format!("Elo {}", idx.len()));
        }
    }
    if app.caps_loading {
        parts.push("caps…".to_string());
    } else if app.caps_status.is_some() {
        parts.push("caps n/a".to_string());
    } else if let Some(idx) = &app.caps {
        if !idx.is_empty() {
            parts.push(format!("caps {}", idx.len()));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("· {}", parts.join(" · "))
    }
}

fn draw_search(frame: &mut Frame, area: Rect, app: &App) {
    let line = if app.search_mode {
        Line::from(vec![
            Span::styled("/", Style::new().fg(Color::Green).bold()),
            Span::styled(app.query.clone(), Style::new().fg(Color::White)),
            Span::styled("█", Style::new().fg(Color::Green)),
        ])
    } else if app.query.is_empty() {
        Line::from(Span::styled(
            " / to search",
            Style::new().fg(Color::DarkGray),
        ))
    } else {
        Line::from(vec![
            Span::styled("filter: ", Style::new().fg(Color::DarkGray)),
            Span::styled(app.query.clone(), Style::new().fg(Color::White)),
            Span::styled(
                format!("  ({} matches)", app.view.len()),
                Style::new().fg(Color::DarkGray),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_body(frame: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::vertical([Constraint::Min(5), Constraint::Length(10)]).split(area);

    let header = sort_header(app);
    let rows: Vec<TableRow> = app
        .view
        .iter()
        .map(|&i| {
            let r = &app.rows[i];
            TableRow::new(vec![
                Cell::from(r.model.clone()),
                Cell::from(r.developer.clone()).style(Style::new().fg(Color::Magenta)),
                Cell::from(r.deployment.clone()),
                Cell::from(r.mode.clone()),
                Cell::from(fmt_price(r.input)).style(Style::new().fg(Color::Green)),
                Cell::from(fmt_price(r.cached)).style(Style::new().fg(Color::Yellow)),
                Cell::from(fmt_price(r.output)).style(Style::new().fg(Color::Red)),
                Cell::from(fmt_elo(r.arena.as_ref())).style(Style::new().fg(Color::Cyan)),
                Cell::from(fmt_context(r.caps.as_ref())).style(Style::new().fg(Color::Blue)),
                Cell::from(fmt_caps(r.caps.as_ref())).style(Style::new().fg(Color::LightBlue)),
            ])
        })
        .collect();

    let price_w = 9 + app.currency.len() as u16;
    let widths = [
        Constraint::Min(16),
        Constraint::Length(13),
        Constraint::Length(11),
        Constraint::Length(8),
        Constraint::Length(price_w),
        Constraint::Length(price_w),
        Constraint::Length(price_w),
        Constraint::Length(6),
        Constraint::Length(7),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title(format!(" Models ({}) ", app.view.len())),
        )
        .column_spacing(1)
        .highlight_symbol("> ")
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED));

    let mut state = TableState::default();
    if !app.view.is_empty() {
        state.select(Some(app.selected.min(app.view.len() - 1)));
    }
    frame.render_stateful_widget(table, chunks[0], &mut state);

    draw_detail(frame, chunks[1], app);
}

fn sort_header(app: &App) -> TableRow<'static> {
    let cols: [(String, Option<SortCol>); 10] = [
        ("Model".to_string(), Some(SortCol::Model)),
        ("Developer".to_string(), Some(SortCol::Developer)),
        ("Deployment".to_string(), Some(SortCol::Deployment)),
        ("Mode".to_string(), Some(SortCol::Mode)),
        (format!("Input {}/1M", app.currency), Some(SortCol::Input)),
        (format!("Cached {}/1M", app.currency), Some(SortCol::Cached)),
        (format!("Output {}/1M", app.currency), Some(SortCol::Output)),
        ("Elo".to_string(), Some(SortCol::Intelligence)),
        ("Ctx".to_string(), Some(SortCol::Context)),
        ("Caps".to_string(), None),
    ];
    let cells: Vec<Cell> = cols
        .into_iter()
        .map(|(label, col)| {
            let arrow = if col == Some(app.sort_col) {
                if app.sort_asc {
                    " ▲"
                } else {
                    " ▼"
                }
            } else {
                ""
            };
            Cell::from(format!("{label}{arrow}")).style(Style::new().bold().fg(Color::Cyan))
        })
        .collect();
    TableRow::new(cells)
}

fn draw_detail(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" Details ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(r) = app.selected_row() else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No row selected",
                Style::new().fg(Color::DarkGray),
            ))),
            inner,
        );
        return;
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Model:      ", Style::new().fg(Color::DarkGray)),
            Span::styled(r.model.clone(), Style::new().bold()),
            Span::raw("   "),
            Span::styled("Developer: ", Style::new().fg(Color::DarkGray)),
            Span::styled(r.developer.clone(), Style::new().fg(Color::Magenta)),
        ]),
        Line::from(vec![
            Span::styled("Deployment: ", Style::new().fg(Color::DarkGray)),
            Span::raw(r.deployment.clone()),
            Span::raw("   "),
            Span::styled("Mode: ", Style::new().fg(Color::DarkGray)),
            Span::raw(r.mode.clone()),
        ]),
        Line::from(vec![
            Span::styled("Input:      ", Style::new().fg(Color::DarkGray)),
            Span::styled(fmt_price(r.input), Style::new().fg(Color::Green)),
            Span::styled(
                format!(" {}/1M", app.currency),
                Style::new().fg(Color::DarkGray),
            ),
            Span::raw("    "),
            Span::styled("Cached: ", Style::new().fg(Color::DarkGray)),
            Span::styled(fmt_price(r.cached), Style::new().fg(Color::Yellow)),
            Span::raw("    "),
            Span::styled("Output: ", Style::new().fg(Color::DarkGray)),
            Span::styled(fmt_price(r.output), Style::new().fg(Color::Red)),
        ]),
        Line::from(vec![
            Span::styled("Product:    ", Style::new().fg(Color::DarkGray)),
            Span::raw(r.product.clone()),
        ]),
        Line::from(match &r.arena {
            Some(a) => vec![
                Span::styled("LMArena:    ", Style::new().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:.0}", a.rating),
                    Style::new().fg(Color::Cyan).bold(),
                ),
                Span::styled(
                    format!("  rank #{}", a.rank),
                    Style::new().fg(Color::DarkGray),
                ),
                Span::styled(format!("  ({})", a.name), Style::new().fg(Color::DarkGray)),
            ],
            None => vec![
                Span::styled("LMArena:    ", Style::new().fg(Color::DarkGray)),
                Span::styled("-", Style::new().fg(Color::DarkGray)),
            ],
        }),
        Line::from(vec![
            Span::styled("Context:    ", Style::new().fg(Color::DarkGray)),
            Span::styled(fmt_context(r.caps.as_ref()), Style::new().fg(Color::Blue)),
            Span::styled(
                if r.caps.as_ref().is_some_and(|c| c.context > 0) {
                    " tokens"
                } else {
                    ""
                },
                Style::new().fg(Color::DarkGray),
            ),
            Span::raw("    "),
            Span::styled("Source: ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                r.caps
                    .as_ref()
                    .map(|c| c.source.clone())
                    .unwrap_or_else(|| "-".to_string()),
                Style::new().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled("Caps:       ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                caps_words(r.caps.as_ref()),
                Style::new().fg(Color::LightBlue),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let hint = if app.search_mode {
        "type to filter · Enter accept · Esc clear"
    } else {
        "q quit · / search · s sort · r reverse · d deployment · m mode · f filters · R refresh · e export · ? help"
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {hint}"),
            Style::new().fg(Color::DarkGray),
        ))),
        area,
    );
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

fn draw_filter_panel(frame: &mut Frame, app: &App) {
    let area = centered(frame.area(), 64, 18);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" Filters ")
        .title_bottom(
            Line::from(" Tab switch · ↑↓ move · Space toggle · Enter apply · Esc close ")
                .centered(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cols = Layout::horizontal([Constraint::Length(18), Constraint::Min(20)]).split(inner);

    let fields = [
        FilterField::Region,
        FilterField::Currency,
        FilterField::Services,
    ];
    let field_items: Vec<ListItem> = fields
        .iter()
        .map(|f| {
            let value = match f {
                FilterField::Region => app.region.clone(),
                FilterField::Currency => app.currency.clone(),
                FilterField::Services => {
                    let on = app.services.iter().filter(|s| **s).count();
                    format!("{on}/{}", options::SERVICES.len())
                }
            };
            let marker = if *f == app.filter_field { "› " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::raw(marker),
                Span::styled(f.label(), Style::new().bold()),
                Span::raw("  "),
                Span::styled(value, Style::new().fg(Color::Yellow)),
            ]))
        })
        .collect();
    let mut field_state = ListState::default();
    field_state.select(Some(
        fields
            .iter()
            .position(|f| *f == app.filter_field)
            .unwrap_or(0),
    ));
    frame.render_stateful_widget(
        List::new(field_items).highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
        cols[0],
        &mut field_state,
    );

    let option_items: Vec<ListItem> = match app.filter_field {
        FilterField::Region => options::REGIONS.iter().map(|r| ListItem::new(*r)).collect(),
        FilterField::Currency => options::CURRENCIES
            .iter()
            .map(|c| ListItem::new(*c))
            .collect(),
        FilterField::Services => options::SERVICES
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mark = if app.services.get(i).copied().unwrap_or(false) {
                    "[x]"
                } else {
                    "[ ]"
                };
                ListItem::new(format!("{mark} {s}"))
            })
            .collect(),
    };
    let mut opt_state = ListState::default();
    opt_state.select(Some(
        app.filter_opt.min(option_items.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(
        List::new(option_items)
            .block(Block::bordered().border_type(BorderType::Rounded))
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> "),
        cols[1],
        &mut opt_state,
    );
}

fn draw_selector(frame: &mut Frame, app: &App, deployment: bool) {
    let (title, options) = if deployment {
        (" Deployment ", app.deployment_options())
    } else {
        (" Mode ", app.mode_options())
    };
    let height = (options.len() as u16 + 2).min(18);
    let area = centered(frame.area(), 40, height);
    frame.render_widget(Clear, area);

    let items: Vec<ListItem> = options.iter().map(|o| ListItem::new(o.clone())).collect();
    let list = List::new(items)
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title(title.to_string()),
        )
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    let mut state = ListState::default();
    state.select(Some(app.selector_opt.min(options.len().saturating_sub(1))));
    frame.render_stateful_widget(list, area, &mut state);
}

fn help_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "Foundry Price Checker",
            Style::new().bold().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from("  /          search models, developers, products, mode"),
        Line::from("  s / r      cycle sort column / reverse order"),
        Line::from("  d / m      filter by deployment / mode"),
        Line::from("  f          filter panel (region, currency, services)"),
        Line::from("  R          refetch prices + LMArena Elo"),
        Line::from("  e          export current view to CSV"),
        Line::from("  mouse      wheel scrolls one row / option at a time"),
        Line::from("  Elo        LMArena text leaderboard, approximate match"),
        Line::from("  Ctx/Caps   OpenRouter: context window + V/T/R/A/J flags"),
        Line::from("  j/k ↑↓     move selection      PgUp/PgDn  page"),
        Line::from("  Home/End   first / last row"),
        Line::from("  ?          toggle this help"),
        Line::from("  q          quit"),
        Line::from(""),
        Line::from(Span::styled(
            "  Esc closes panels · Enter applies",
            Style::new().fg(Color::DarkGray),
        )),
    ]
}

fn draw_help(frame: &mut Frame, app: &mut App) {
    let lines = help_lines();
    let area = centered(frame.area(), 62, lines.len() as u16 + 2);
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" Help ")
        .title_alignment(Alignment::Left);
    let inner = block.inner(area);

    let inner_w = inner.width.max(1) as usize;
    let rows: usize = lines
        .iter()
        .map(|l| {
            let w = l.width();
            if w == 0 {
                1
            } else {
                w.div_ceil(inner_w)
            }
        })
        .sum();
    let max_scroll = rows.saturating_sub(inner.height as usize) as u16;
    if app.help_scroll > max_scroll {
        app.help_scroll = max_scroll;
    }

    let block = if max_scroll > 0 {
        block.title_bottom(
            Line::from(format!(" ↑↓ scroll {}/{} ", app.help_scroll, max_scroll)).centered(),
        )
    } else {
        block
    };

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((app.help_scroll, 0)),
        area,
    );
}

fn fmt_elo(intel: Option<&crate::arena::Intel>) -> String {
    match intel {
        Some(i) => format!("{:.0}", i.rating),
        None => "-".to_string(),
    }
}

fn fmt_context(caps: Option<&crate::openrouter::Caps>) -> String {
    match caps {
        Some(c) if c.context > 0 => {
            if c.context >= 1_000_000 {
                let s = format!("{:.1}", c.context as f64 / 1_000_000.0);
                format!("{}M", s.trim_end_matches(".0"))
            } else {
                format!("{}K", c.context.div_ceil(1000))
            }
        }
        _ => "-".to_string(),
    }
}

fn fmt_caps(caps: Option<&crate::openrouter::Caps>) -> String {
    let Some(c) = caps else {
        return "-".to_string();
    };
    let mut flags = String::new();
    for (on, ch) in [
        (c.vision, 'V'),
        (c.tools, 'T'),
        (c.reasoning, 'R'),
        (c.audio, 'A'),
        (c.json, 'J'),
    ] {
        if on {
            flags.push(ch);
        }
    }
    if flags.is_empty() {
        "text".to_string()
    } else {
        flags
    }
}

fn caps_words(caps: Option<&crate::openrouter::Caps>) -> String {
    let Some(c) = caps else {
        return "-".to_string();
    };
    let mut words = vec!["text"];
    if c.vision {
        words.push("image");
    }
    if c.audio {
        words.push("audio");
    }
    if c.tools {
        words.push("tools");
    }
    if c.reasoning {
        words.push("reasoning");
    }
    if c.json {
        words.push("structured outputs");
    }
    words.join(", ")
}

fn fmt_price(v: Option<f64>) -> String {
    match v {
        None => "-".to_string(),
        Some(x) => {
            let s = format!("{x:.4}");
            let s = s.trim_end_matches('0').trim_end_matches('.');
            group_thousands(s)
        }
    }
}

fn group_thousands(s: &str) -> String {
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (s, None),
    };
    let neg = int.starts_with('-');
    let digits = int.trim_start_matches('-');
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(&grouped);
    if let Some(f) = frac {
        out.push('.');
        out.push_str(f);
    }
    out
}
