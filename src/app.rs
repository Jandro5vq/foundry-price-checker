use std::cmp::Ordering;
use std::sync::mpsc::Sender;

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::api::{self, FetchMsg};
use crate::arena::ArenaIndex;
use crate::export;
use crate::model::{build_rows, Row};
use crate::openrouter::CapsIndex;
use crate::options;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortCol {
    Model,
    Developer,
    Deployment,
    Mode,
    Input,
    Cached,
    Output,
    Intelligence,
    Context,
}

impl SortCol {
    pub const ALL: [SortCol; 9] = [
        SortCol::Model,
        SortCol::Developer,
        SortCol::Deployment,
        SortCol::Mode,
        SortCol::Input,
        SortCol::Cached,
        SortCol::Output,
        SortCol::Intelligence,
        SortCol::Context,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SortCol::Model => "Model",
            SortCol::Developer => "Developer",
            SortCol::Deployment => "Deployment",
            SortCol::Mode => "Mode",
            SortCol::Input => "Input",
            SortCol::Cached => "Cached",
            SortCol::Output => "Output",
            SortCol::Intelligence => "Elo",
            SortCol::Context => "Context",
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|c| *c == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    fn is_numeric(self) -> bool {
        matches!(
            self,
            SortCol::Input
                | SortCol::Cached
                | SortCol::Output
                | SortCol::Intelligence
                | SortCol::Context
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    None,
    Filter,
    Deployment,
    Mode,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterField {
    Region,
    Currency,
    Services,
}

impl FilterField {
    fn next(self) -> Self {
        match self {
            FilterField::Region => FilterField::Currency,
            FilterField::Currency => FilterField::Services,
            FilterField::Services => FilterField::Region,
        }
    }

    fn prev(self) -> Self {
        match self {
            FilterField::Region => FilterField::Services,
            FilterField::Currency => FilterField::Region,
            FilterField::Services => FilterField::Currency,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FilterField::Region => "Region",
            FilterField::Currency => "Currency",
            FilterField::Services => "Services",
        }
    }
}

pub struct App {
    pub rows: Vec<Row>,
    pub view: Vec<usize>,
    pub selected: usize,
    pub query: String,
    pub search_mode: bool,
    pub sort_col: SortCol,
    pub sort_asc: bool,
    pub region: String,
    pub currency: String,
    pub services: Vec<bool>,
    pub deployment_filter: Option<String>,
    pub mode_filter: Option<String>,
    pub loading: bool,
    pub status: String,
    pub progress: Option<(usize, usize)>,
    pub skipped: usize,
    pub panel: Panel,
    pub filter_field: FilterField,
    pub filter_opt: usize,
    pub selector_opt: usize,
    pub should_quit: bool,
    pub tick: u64,
    pub arena: Option<ArenaIndex>,
    pub arena_enabled: bool,
    pub arena_loading: bool,
    pub arena_status: Option<String>,
    pub caps: Option<CapsIndex>,
    pub caps_enabled: bool,
    pub caps_loading: bool,
    pub caps_status: Option<String>,
    pub help_scroll: u16,
}

impl App {
    pub fn new(
        region: String,
        currency: String,
        services: Vec<String>,
        arena_enabled: bool,
        caps_enabled: bool,
    ) -> Self {
        let flags = options::SERVICES
            .iter()
            .map(|s| {
                if services.is_empty() {
                    true
                } else {
                    services.iter().any(|x| x.eq_ignore_ascii_case(s))
                }
            })
            .collect();
        Self {
            rows: Vec::new(),
            view: Vec::new(),
            selected: 0,
            query: String::new(),
            search_mode: false,
            sort_col: SortCol::Model,
            sort_asc: true,
            region,
            currency,
            services: flags,
            deployment_filter: None,
            mode_filter: None,
            loading: false,
            status: "Press R to fetch prices".to_string(),
            progress: None,
            skipped: 0,
            panel: Panel::None,
            filter_field: FilterField::Region,
            filter_opt: 0,
            selector_opt: 0,
            should_quit: false,
            tick: 0,
            arena: None,
            arena_enabled,
            arena_loading: arena_enabled,
            arena_status: None,
            caps: None,
            caps_enabled,
            caps_loading: caps_enabled,
            caps_status: None,
            help_scroll: 0,
        }
    }

    pub fn refresh(&mut self, tx: &Sender<FetchMsg>) {
        let services: Vec<String> = options::SERVICES
            .iter()
            .enumerate()
            .filter(|(i, _)| self.services.get(*i).copied().unwrap_or(false))
            .map(|(_, s)| s.to_string())
            .collect();
        if services.is_empty() {
            self.status = "Select at least one service".to_string();
            return;
        }
        self.loading = true;
        self.progress = None;
        self.status = format!("Fetching {} in {}...", self.region, self.currency);
        self.rows.clear();
        self.view.clear();
        self.selected = 0;
        api::spawn_fetch(
            self.region.clone(),
            self.currency.clone(),
            services,
            tx.clone(),
        );
        if self.arena_enabled {
            self.arena_loading = true;
            self.arena_status = None;
            crate::arena::spawn_fetch(tx.clone());
        }
        if self.caps_enabled {
            self.caps_loading = true;
            self.caps_status = None;
            crate::openrouter::spawn_fetch(tx.clone());
        }
    }

    fn annotate_arena(&mut self) {
        let Some(index) = &self.arena else {
            return;
        };
        for row in &mut self.rows {
            row.arena = index.lookup(&row.model, &row.product);
        }
    }

    fn annotate_caps(&mut self) {
        let Some(index) = &self.caps else {
            return;
        };
        for row in &mut self.rows {
            row.caps = index.lookup(&row.model, &row.product);
        }
    }

    pub fn on_fetch(&mut self, msg: FetchMsg) {
        match msg {
            FetchMsg::Progress { page, count } => {
                self.progress = Some((page, count));
                self.status = format!("Fetching... page {page} ({count} items)");
            }
            FetchMsg::Done(items) => {
                let (rows, other) = build_rows(&items, None);
                self.rows = rows;
                self.skipped = other;
                self.loading = false;
                self.progress = None;
                self.annotate_arena();
                self.annotate_caps();
                self.status = format!("{} rows ({} skipped)", self.rows.len(), other);
                self.selected = 0;
                self.recompute();
            }
            FetchMsg::Error(e) => {
                self.loading = false;
                self.progress = None;
                self.status = format!("Error: {e}");
            }
            FetchMsg::Arena(entries) => {
                self.arena = Some(ArenaIndex::new(entries));
                self.arena_loading = false;
                self.annotate_arena();
                self.recompute();
            }
            FetchMsg::ArenaError(e) => {
                self.arena_loading = false;
                self.arena_status = Some(format!("Elo unavailable: {e}"));
            }
            FetchMsg::Caps(entries) => {
                self.caps = Some(CapsIndex::new(entries));
                self.caps_loading = false;
                self.annotate_caps();
                self.recompute();
            }
            FetchMsg::CapsError(e) => {
                self.caps_loading = false;
                self.caps_status = Some(format!("capabilities unavailable: {e}"));
            }
        }
    }

    pub fn recompute(&mut self) {
        let q = self.query.to_lowercase();
        self.view = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                (q.is_empty()
                    || r.model.to_lowercase().contains(&q)
                    || r.developer.to_lowercase().contains(&q)
                    || r.product.to_lowercase().contains(&q)
                    || r.deployment.to_lowercase().contains(&q)
                    || r.mode.to_lowercase().contains(&q))
                    && self
                        .deployment_filter
                        .as_ref()
                        .is_none_or(|d| &r.deployment == d)
                    && self.mode_filter.as_ref().is_none_or(|m| &r.mode == m)
            })
            .map(|(i, _)| i)
            .collect();

        let col = self.sort_col;
        let asc = self.sort_asc;
        self.view
            .sort_by(|&a, &b| compare(&self.rows[a], &self.rows[b], col, asc));
        if self.selected >= self.view.len() {
            self.selected = self.view.len().saturating_sub(1);
        }
    }

    pub fn selected_row(&self) -> Option<&Row> {
        self.view.get(self.selected).map(|&i| &self.rows[i])
    }

    fn move_selection(&mut self, delta: i64) {
        if self.view.is_empty() {
            return;
        }
        let max = self.view.len() as i64 - 1;
        let next = (self.selected as i64 + delta).clamp(0, max);
        self.selected = next as usize;
    }

    /// Mouse wheel: exactly one line per notch, routed to the active panel.
    pub fn on_scroll(&mut self, delta: i64) {
        match self.panel {
            Panel::Filter => {
                let len = self.filter_options().len().max(1) as i64;
                self.filter_opt = (self.filter_opt as i64 + delta).rem_euclid(len) as usize;
            }
            Panel::Deployment | Panel::Mode => {
                let len = if self.panel == Panel::Deployment {
                    self.deployment_options().len()
                } else {
                    self.mode_options().len()
                }
                .max(1) as i64;
                self.selector_opt = (self.selector_opt as i64 + delta).rem_euclid(len) as usize;
            }
            Panel::Help => {
                self.help_scroll = (self.help_scroll as i64 + delta).max(0) as u16;
            }
            Panel::None => self.move_selection(delta),
        }
    }

    fn cycle_sort(&mut self) {
        self.sort_col = self.sort_col.next();
    }

    fn open_filter_panel(&mut self) {
        self.panel = Panel::Filter;
        self.filter_field = FilterField::Region;
        self.filter_opt = options::region_index(&self.region);
    }

    fn open_selector(&mut self, panel: Panel) {
        self.panel = panel;
        self.selector_opt = match panel {
            Panel::Deployment => self
                .deployment_filter
                .as_ref()
                .and_then(|d| self.deployment_options().iter().position(|o| o == d))
                .unwrap_or(0),
            Panel::Mode => self
                .mode_filter
                .as_ref()
                .and_then(|m| self.mode_options().iter().position(|o| o == m))
                .unwrap_or(0),
            _ => 0,
        };
    }

    pub fn deployment_options(&self) -> Vec<String> {
        let mut v: Vec<String> = self.rows.iter().map(|r| r.deployment.clone()).collect();
        v.sort();
        v.dedup();
        let mut out = vec!["All".to_string()];
        out.extend(v);
        out
    }

    pub fn mode_options(&self) -> Vec<String> {
        let mut v: Vec<String> = self.rows.iter().map(|r| r.mode.clone()).collect();
        v.sort();
        v.dedup();
        let mut out = vec!["All".to_string()];
        out.extend(v);
        out
    }

    fn filter_options(&self) -> Vec<String> {
        match self.filter_field {
            FilterField::Region => options::REGIONS.iter().map(|s| s.to_string()).collect(),
            FilterField::Currency => options::CURRENCIES.iter().map(|s| s.to_string()).collect(),
            FilterField::Services => options::SERVICES.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn current_filter_index(&self) -> usize {
        match self.filter_field {
            FilterField::Region => options::region_index(&self.region),
            FilterField::Currency => options::currency_index(&self.currency),
            FilterField::Services => self.filter_opt.min(options::SERVICES.len() - 1),
        }
    }

    fn export_view(&mut self) {
        let path = format!("model_prices_{}.csv", export::timestamp());
        let rows: Vec<&Row> = self.view.iter().map(|&i| &self.rows[i]).collect();
        match export::export(&rows, &path) {
            Ok(n) => self.status = format!("Exported {n} rows to {path}"),
            Err(e) => self.status = format!("Export failed: {e:#}"),
        }
    }

    pub fn on_key(&mut self, key: KeyEvent, tx: &Sender<FetchMsg>) {
        if self.panel != Panel::None {
            self.on_panel_key(key, tx);
            return;
        }
        if self.search_mode {
            match key.code {
                KeyCode::Esc => {
                    self.search_mode = false;
                    self.query.clear();
                    self.recompute();
                }
                KeyCode::Enter => self.search_mode = false,
                KeyCode::Backspace => {
                    self.query.pop();
                    self.recompute();
                }
                KeyCode::Char(c) => {
                    self.query.push(c);
                    self.recompute();
                }
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('/') => self.search_mode = true,
            KeyCode::Char('s') => {
                self.cycle_sort();
                self.recompute();
            }
            KeyCode::Char('r') => {
                self.sort_asc = !self.sort_asc;
                self.recompute();
            }
            KeyCode::Char('f') => self.open_filter_panel(),
            KeyCode::Char('d') => self.open_selector(Panel::Deployment),
            KeyCode::Char('m') => self.open_selector(Panel::Mode),
            KeyCode::Char('e') => self.export_view(),
            KeyCode::Char('R') => self.refresh(tx),
            KeyCode::Char('?') => {
                self.help_scroll = 0;
                self.panel = Panel::Help;
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.view.len().saturating_sub(1),
            _ => {}
        }
    }

    fn on_panel_key(&mut self, key: KeyEvent, tx: &Sender<FetchMsg>) {
        match self.panel {
            Panel::Help => match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => self.panel = Panel::None,
                KeyCode::Down | KeyCode::Char('j') => {
                    self.help_scroll = self.help_scroll.saturating_add(1)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.help_scroll = self.help_scroll.saturating_sub(1)
                }
                KeyCode::PageDown => self.help_scroll = self.help_scroll.saturating_add(10),
                KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(10),
                KeyCode::Home => self.help_scroll = 0,
                KeyCode::End => self.help_scroll = u16::MAX,
                _ => {}
            },
            Panel::Filter => self.on_filter_key(key, tx),
            Panel::Deployment => self.on_selector_key(key, true),
            Panel::Mode => self.on_selector_key(key, false),
            Panel::None => {}
        }
    }

    fn on_filter_key(&mut self, key: KeyEvent, tx: &Sender<FetchMsg>) {
        let len = self.filter_options().len();
        match key.code {
            KeyCode::Esc => self.panel = Panel::None,
            KeyCode::Tab | KeyCode::Right => {
                self.filter_field = self.filter_field.next();
                self.filter_opt = self.current_filter_index();
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.filter_field = self.filter_field.prev();
                self.filter_opt = self.current_filter_index();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.filter_opt = (self.filter_opt + 1) % len;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.filter_opt = (self.filter_opt + len - 1) % len;
            }
            KeyCode::Char(' ') if self.filter_field == FilterField::Services => {
                if let Some(f) = self.services.get_mut(self.filter_opt) {
                    *f = !*f;
                }
            }
            KeyCode::Enter => {
                match self.filter_field {
                    FilterField::Region => {
                        self.region = options::REGIONS[self.filter_opt].to_string();
                    }
                    FilterField::Currency => {
                        self.currency = options::CURRENCIES[self.filter_opt].to_string();
                    }
                    FilterField::Services => {}
                }
                if !self.services.iter().any(|s| *s) {
                    self.status = "Select at least one service".to_string();
                    return;
                }
                self.panel = Panel::None;
                self.refresh(tx);
            }
            _ => {}
        }
    }

    fn on_selector_key(&mut self, key: KeyEvent, deployment: bool) {
        let options = if deployment {
            self.deployment_options()
        } else {
            self.mode_options()
        };
        let len = options.len().max(1);
        match key.code {
            KeyCode::Esc => self.panel = Panel::None,
            KeyCode::Down | KeyCode::Char('j') => {
                self.selector_opt = (self.selector_opt + 1) % len;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selector_opt = (self.selector_opt + len - 1) % len;
            }
            KeyCode::Enter => {
                let choice = options.get(self.selector_opt).cloned();
                let value = match choice.as_deref() {
                    None | Some("All") => None,
                    Some(v) => Some(v.to_string()),
                };
                if deployment {
                    self.deployment_filter = value;
                } else {
                    self.mode_filter = value;
                }
                self.panel = Panel::None;
                self.recompute();
            }
            _ => {}
        }
    }
}

fn compare(a: &Row, b: &Row, col: SortCol, asc: bool) -> Ordering {
    if col.is_numeric() {
        let (x, y) = match col {
            SortCol::Input => (a.input, b.input),
            SortCol::Cached => (a.cached, b.cached),
            SortCol::Output => (a.output, b.output),
            SortCol::Intelligence => (
                a.arena.as_ref().map(|i| i.rating),
                b.arena.as_ref().map(|i| i.rating),
            ),
            SortCol::Context => (
                a.caps.as_ref().map(|c| c.context as f64),
                b.caps.as_ref().map(|c| c.context as f64),
            ),
            _ => (None, None),
        };
        return match (x, y) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(x), Some(y)) => {
                let ord = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
                if asc {
                    ord
                } else {
                    ord.reverse()
                }
            }
        };
    }
    let ord = match col {
        SortCol::Model => a.model.to_lowercase().cmp(&b.model.to_lowercase()),
        SortCol::Developer => a.developer.to_lowercase().cmp(&b.developer.to_lowercase()),
        SortCol::Deployment => a.deployment.cmp(&b.deployment),
        SortCol::Mode => a.mode.cmp(&b.mode),
        _ => Ordering::Equal,
    };
    if asc {
        ord
    } else {
        ord.reverse()
    }
}
