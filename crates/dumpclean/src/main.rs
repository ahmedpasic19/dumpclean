mod index;
mod keys;
mod ops;

use anyhow::{bail, Context, Result};
use index::Index;
use ops::TestResult;
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Cell, Clear, Gauge, Paragraph, Row, Table, TableState, Wrap},
    DefaultTerminal, Frame,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const USAGE_BODY: &str = "\
dumpclean - remove tables/views/routines/events from huge MySQL dumps

USAGE:
  dumpclean <dump.sql> [--mysql-args \"-h 127.0.0.1 -u root mydb\"]
  dumpclean <dump.sql> --list
  dumpclean <dump.sql> --remove tbl1,tbl2 [--in-place | --out cleaned.sql]
  dumpclean <dump.sql> --test --mysql-args \"-u root mydb\"   (import, print failing tables)

OPTIONS:
  --mysql-args   arguments passed to mysql.exe for the test-import mode (key: t)
                 (put the password in MYSQL_PWD or use --defaults-extra-file=...)
  --list         print all items and exit
  --remove       non-interactive removal of the named items
  --test         non-interactive test import, prints errors grouped by table
  --exclude      with --test: leave these items out of the test import
  --in-place     rewrite the dump file itself (default: <name>.cleaned.sql)
  --out          output path for --remove";

fn usage() -> String {
    format!("{USAGE_BODY}{}", keys::USAGE_KEYS)
}

struct Args {
    path: PathBuf,
    mysql_args: Vec<String>,
    list: bool,
    test: bool,
    remove: Vec<String>,
    exclude: Vec<String>,
    in_place: bool,
    out: Option<PathBuf>,
}

fn parse_args() -> Result<Args> {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        path: PathBuf::new(),
        mysql_args: vec![],
        list: false,
        test: false,
        remove: vec![],
        exclude: vec![],
        in_place: false,
        out: None,
    };
    let mut path = None;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--mysql-args" => {
                a.mysql_args = it
                    .next()
                    .context("--mysql-args needs a value")?
                    .split_whitespace()
                    .map(String::from)
                    .collect()
            }
            "--list" => a.list = true,
            "--test" => a.test = true,
            "--remove" => {
                a.remove = it
                    .next()
                    .context("--remove needs a value")?
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            "--exclude" => {
                a.exclude = it
                    .next()
                    .context("--exclude needs a value")?
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            "--in-place" => a.in_place = true,
            "--out" => a.out = Some(PathBuf::from(it.next().context("--out needs a value")?)),
            s if s.starts_with("--") => bail!("unknown option {s}\n\n{}", usage()),
            _ => path = Some(PathBuf::from(&arg)),
        }
    }
    a.path = path.with_context(|| format!("missing dump file\n\n{}", usage()))?;
    Ok(a)
}

fn default_out(path: &Path) -> PathBuf {
    path.with_extension("cleaned.sql")
}

// ---------- formatting ----------

fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn fmt_size(b: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 3 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.2} {}", U[i])
    }
}

// ---------- drawing helpers ----------

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn draw_busy(f: &mut Frame, title: &str, done: u64, total: u64) {
    let area = centered(f.area(), 64, 5);
    f.render_widget(Clear, area);
    let ratio = if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0)
    };
    let g = Gauge::default()
        .block(Block::bordered().title(format!(" {title} ")))
        .ratio(ratio)
        .label(format!(
            "{:.0}%  ({} / {})",
            ratio * 100.0,
            fmt_size(done),
            fmt_size(total)
        ))
        .gauge_style(Style::new().fg(Color::Cyan));
    f.render_widget(g, area);
}

/// Progress callback that redraws a gauge (throttled) while a long operation runs.
fn busy<'a>(term: &'a mut DefaultTerminal, title: &'a str) -> impl FnMut(u64, u64) + 'a {
    let _ = term.draw(|f| draw_busy(f, title, 0, 1));
    let mut last: Option<Instant> = None;
    move |done, total| {
        if last.map_or(true, |l| l.elapsed() >= Duration::from_millis(100)) {
            last = Some(Instant::now());
            let _ = term.draw(|f| draw_busy(f, title, done, total));
        }
    }
}

// ---------- app ----------

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Filter,
    SaveMenu,
    ConfirmInPlace,
    ConfirmQuit,
    ConfirmTest,
    Results,
}

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    File,
    Size,
    Lines,
    Name,
}

impl Sort {
    fn next(self) -> Sort {
        match self {
            Sort::File => Sort::Size,
            Sort::Size => Sort::Lines,
            Sort::Lines => Sort::Name,
            Sort::Name => Sort::File,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Sort::File => "file order",
            Sort::Size => "size",
            Sort::Lines => "lines",
            Sort::Name => "name",
        }
    }
}

struct App {
    path: PathBuf,
    idx: Index,
    mysql_args: Vec<String>,
    view: Vec<usize>,
    sort: Sort,
    filter: String,
    state: TableState,
    mode: Mode,
    status: String,
    failed: HashMap<usize, u64>,
    results: Option<TestResult>,
    scroll: u16,
}

impl App {
    fn selected_item(&self) -> Option<usize> {
        self.state
            .selected()
            .and_then(|i| self.view.get(i).copied())
    }

    fn rebuild_view(&mut self, keep: Option<usize>) {
        let f = self.filter.to_lowercase();
        let items = &self.idx.items;
        let mut v: Vec<usize> = (0..items.len())
            .filter(|&i| f.is_empty() || items[i].name.to_lowercase().contains(&f))
            .collect();
        match self.sort {
            Sort::File => {}
            Sort::Size => v.sort_by_key(|&i| std::cmp::Reverse(items[i].bytes)),
            Sort::Lines => v.sort_by_key(|&i| std::cmp::Reverse(items[i].lines)),
            Sort::Name => v.sort_by_key(|&i| items[i].name.to_lowercase()),
        }
        self.view = v;
        let pos = keep
            .and_then(|k| self.view.iter().position(|&i| i == k))
            .or_else(|| {
                (!self.view.is_empty())
                    .then(|| self.state.selected().unwrap_or(0).min(self.view.len() - 1))
            });
        self.state.select(pos);
    }

    fn refresh(&mut self) {
        let k = self.selected_item();
        self.rebuild_view(k);
    }

    fn move_sel(&mut self, delta: i64) {
        if self.view.is_empty() {
            return;
        }
        let cur = self.state.selected().unwrap_or(0) as i64;
        let new = (cur + delta).clamp(0, self.view.len() as i64 - 1);
        self.state.select(Some(new as usize));
    }

    fn marked(&self) -> (usize, u64, u64) {
        self.idx
            .items
            .iter()
            .filter(|i| i.marked)
            .fold((0, 0, 0), |(n, b, l), i| (n + 1, b + i.bytes, l + i.lines))
    }

    // ----- drawing -----

    fn draw(&mut self, f: &mut Frame) {
        let [main, footer] =
            Layout::vertical([Constraint::Min(5), Constraint::Length(4)]).areas(f.area());

        let rows: Vec<Row> = self
            .view
            .iter()
            .map(|&i| {
                let it = &self.idx.items[i];
                let test = self
                    .failed
                    .get(&i)
                    .map(|n| format!("✗ {} err", fmt_num(*n)))
                    .unwrap_or_default();
                let mut row = Row::new(vec![
                    Cell::from(if it.marked { "[x]" } else { "[ ]" }),
                    Cell::from(it.kind.label()),
                    Cell::from(it.name.clone()),
                    Cell::from(Line::from(fmt_num(it.lines)).right_aligned()),
                    Cell::from(Line::from(fmt_size(it.bytes)).right_aligned()),
                    Cell::from(test),
                ]);
                if it.marked {
                    row = row.style(
                        Style::new()
                            .fg(Color::Red)
                            .add_modifier(Modifier::CROSSED_OUT),
                    );
                } else if self.failed.contains_key(&i) {
                    row = row.style(Style::new().fg(Color::Yellow));
                }
                row
            })
            .collect();

        let widths = [
            Constraint::Length(4),
            Constraint::Length(10),
            Constraint::Min(20),
            Constraint::Length(16),
            Constraint::Length(11),
            Constraint::Length(12),
        ];
        let title = format!(
            " {} — {} items, {} lines, {} ",
            self.path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            fmt_num(self.idx.items.len() as u64),
            fmt_num(self.idx.total_lines),
            fmt_size(self.idx.total_bytes)
        );
        let header = Row::new(vec!["", "Type", "Name", "Lines", "Size", "Test"])
            .style(Style::new().add_modifier(Modifier::BOLD).fg(Color::Cyan));
        let table = Table::new(rows, widths)
            .header(header)
            .block(Block::bordered().title(title))
            .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED))
            .highlight_symbol("▶ ");
        f.render_stateful_widget(table, main, &mut self.state);

        // footer
        let (n, bytes, lines) = self.marked();
        let line1 = if self.mode == Mode::Filter {
            Line::from(format!(
                " Filter: {}█   (Enter keep, Esc clear)",
                self.filter
            ))
        } else if !self.status.is_empty() {
            Line::from(format!(" {}", self.status)).style(Style::new().fg(Color::Green))
        } else {
            Line::from(format!(
                " {n} marked · removes {} ({} lines) · showing {}/{} · sort: {}{}",
                fmt_size(bytes),
                fmt_num(lines),
                self.view.len(),
                self.idx.items.len(),
                self.sort.label(),
                if self.filter.is_empty() {
                    String::new()
                } else {
                    format!(" · filter: {}", self.filter)
                }
            ))
        };
        let line2 = Line::from(keys::FOOTER).style(Style::new().fg(Color::DarkGray));
        f.render_widget(
            Paragraph::new(vec![line1, line2]).block(Block::bordered()),
            footer,
        );

        // popups
        match self.mode {
            Mode::SaveMenu => {
                let out = default_out(&self.path);
                self.popup(
                    f,
                    " Write ",
                    vec![
                        Line::from(format!(
                            "{n} items marked, {} will be removed.",
                            fmt_size(bytes)
                        )),
                        Line::from(""),
                        Line::from(format!("[n]  write new file  {}", out.display())),
                        Line::from("     (original stays untouched, needs free disk space)"),
                        Line::from("[i]  delete in the dump file itself (no extra disk space)"),
                        Line::from(""),
                        Line::from("[Esc] cancel"),
                    ],
                    80,
                    10,
                );
            }
            Mode::ConfirmInPlace => self.popup(
                f,
                " Rewrite file in place? ",
                vec![
                    Line::from(format!(
                        "{} will be modified directly.",
                        self.path.display()
                    )),
                    Line::from("If the process is killed or the PC loses power halfway,"),
                    Line::from("the file is corrupted. Back it up first if unsure."),
                    Line::from(""),
                    Line::from("[y] do it    [any other key] cancel"),
                ],
                70,
                9,
            ),
            Mode::ConfirmQuit => self.popup(
                f,
                " Quit? ",
                vec![
                    Line::from(format!("{n} marked items have not been written.")),
                    Line::from("[y] quit   [any other key] stay"),
                ],
                50,
                6,
            ),
            Mode::ConfirmTest => self.popup(
                f,
                " Test import ",
                vec![
                    Line::from("This REALLY imports the dump (minus marked items) using:"),
                    Line::from(format!("  mysql {} --force", self.mysql_args.join(" "))),
                    Line::from("Existing tables in that database will be dropped/overwritten."),
                    Line::from(""),
                    Line::from("[y] run   [any other key] cancel"),
                ],
                84,
                9,
            ),
            Mode::Results => self.draw_results(f),
            _ => {}
        }
    }

    fn popup(&self, f: &mut Frame, title: &str, lines: Vec<Line>, w: u16, h: u16) {
        let area = centered(f.area(), w, h);
        f.render_widget(Clear, area);
        f.render_widget(
            Paragraph::new(lines)
                .block(Block::bordered().title(title))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn draw_results(&self, f: &mut Frame) {
        let area = centered(
            f.area(),
            f.area().width.saturating_sub(4),
            f.area().height.saturating_sub(2),
        );
        f.render_widget(Clear, area);
        let mut lines: Vec<Line> = Vec::new();
        if let Some(r) = &self.results {
            let head = format!(
                "Import {}. {} item(s) with errors.",
                if r.aborted {
                    "aborted early"
                } else if r.exit_ok {
                    "finished"
                } else {
                    "finished with errors"
                },
                r.groups.len()
            );
            lines.push(Line::from(head).style(Style::new().add_modifier(Modifier::BOLD)));
            lines.push(Line::from(""));
            for g in &r.groups {
                let title = match g.owner {
                    Some(o) => format!(
                        "✗ {} {}   ({} errors)",
                        self.idx.items[o].kind.label(),
                        self.idx.items[o].name,
                        fmt_num(g.count)
                    ),
                    None => format!(
                        "✗ outside any table/view/routine (header/footer)   ({} errors)",
                        fmt_num(g.count)
                    ),
                };
                lines.push(
                    Line::from(title)
                        .style(Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)),
                );
                lines.push(Line::from(format!(
                    "    {}",
                    g.first.chars().take(400).collect::<String>()
                )));
            }
            if !r.other.is_empty() {
                lines.push(Line::from(""));
                lines.push(
                    Line::from("Other mysql output:")
                        .style(Style::new().add_modifier(Modifier::BOLD)),
                );
                for o in &r.other {
                    lines.push(Line::from(format!("    {o}")));
                }
            }
        }
        lines.push(Line::from(""));
        lines.push(
            Line::from("↑↓ scroll   m mark all failing items for removal   Esc back")
                .style(Style::new().fg(Color::DarkGray)),
        );
        f.render_widget(
            Paragraph::new(lines)
                .block(Block::bordered().title(" Test import results "))
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            area,
        );
    }

    // ----- input -----

    fn on_key(&mut self, key: KeyEvent, term: &mut DefaultTerminal) -> Result<bool> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(true);
        }
        let code = key.code;
        match self.mode {
            Mode::Normal => return Ok(self.key_normal(code)),
            Mode::Filter => {
                match code {
                    KeyCode::Enter => self.mode = Mode::Normal,
                    KeyCode::Esc => {
                        self.filter.clear();
                        self.mode = Mode::Normal;
                    }
                    KeyCode::Backspace => {
                        self.filter.pop();
                    }
                    KeyCode::Char(c) => self.filter.push(c),
                    KeyCode::Up => self.move_sel(-1),
                    KeyCode::Down => self.move_sel(1),
                    _ => {}
                }
                self.refresh();
            }
            Mode::SaveMenu => match code {
                KeyCode::Char('n') => self.save_new(term),
                KeyCode::Char('i') => self.mode = Mode::ConfirmInPlace,
                _ if code == KeyCode::Esc => self.mode = Mode::Normal,
                _ => {}
            },
            Mode::ConfirmInPlace => {
                if code == KeyCode::Char('y') {
                    self.save_in_place(term);
                } else {
                    self.mode = Mode::Normal;
                }
            }
            Mode::ConfirmQuit => {
                if code == KeyCode::Char('y') {
                    return Ok(true);
                }
                self.mode = Mode::Normal;
            }
            Mode::ConfirmTest => {
                if code == KeyCode::Char('y') {
                    self.run_test(term);
                } else {
                    self.mode = Mode::Normal;
                }
            }
            Mode::Results => match code {
                KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
                KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
                KeyCode::Char('m') => {
                    let ids: Vec<usize> = self.failed.keys().copied().collect();
                    for i in &ids {
                        self.idx.items[*i].marked = true;
                    }
                    self.status = format!("Marked {} failing items. Press w to write.", ids.len());
                    self.mode = Mode::Normal;
                }
                KeyCode::Esc | KeyCode::Char('q') => self.mode = Mode::Normal,
                _ => {}
            },
        }
        Ok(false)
    }

    fn key_normal(&mut self, code: KeyCode) -> bool {
        self.status.clear();
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.marked().0 > 0 {
                    self.mode = Mode::ConfirmQuit;
                } else {
                    return true;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
            KeyCode::PageUp => self.move_sel(-15),
            KeyCode::PageDown => self.move_sel(15),
            KeyCode::Home => self.move_sel(-(1 << 40)),
            KeyCode::End => self.move_sel(1 << 40),
            KeyCode::Char(' ') => {
                if let Some(i) = self.selected_item() {
                    let m = &mut self.idx.items[i].marked;
                    *m = !*m;
                    self.move_sel(1);
                }
            }
            KeyCode::Char('a') => {
                for &i in &self.view {
                    self.idx.items[i].marked = true;
                }
            }
            KeyCode::Char('u') => {
                for it in &mut self.idx.items {
                    it.marked = false;
                }
            }
            KeyCode::Char('/') => self.mode = Mode::Filter,
            KeyCode::Char('s') => {
                self.sort = self.sort.next();
                self.refresh();
            }
            KeyCode::Char('w') => {
                if self.marked().0 == 0 {
                    self.status = "Nothing marked.".into();
                } else {
                    self.mode = Mode::SaveMenu;
                }
            }
            KeyCode::Char('t') => {
                if self.mysql_args.is_empty() {
                    self.status = "Start with --mysql-args \"-h 127.0.0.1 -u root dbname\" to enable test import.".into();
                } else {
                    self.mode = Mode::ConfirmTest;
                }
            }
            KeyCode::Char('r') => {
                if self.results.is_some() {
                    self.scroll = 0;
                    self.mode = Mode::Results;
                }
            }
            _ => {}
        }
        false
    }

    // ----- long operations -----

    fn save_new(&mut self, term: &mut DefaultTerminal) {
        let out = default_out(&self.path);
        let res = ops::write_new(
            &self.path,
            &out,
            &self.idx,
            busy(term, "Writing cleaned dump"),
        );
        self.mode = Mode::Normal;
        self.status = match res {
            Ok(sz) => format!(
                "Wrote {} ({}). Original untouched.",
                out.display(),
                fmt_size(sz)
            ),
            Err(e) => format!("Error: {e}"),
        };
    }

    fn save_in_place(&mut self, term: &mut DefaultTerminal) {
        let res = ops::write_in_place(&self.path, &self.idx, busy(term, "Rewriting dump in place"));
        self.mode = Mode::Normal;
        match res {
            Ok(sz) => match index::build_index(&self.path, busy(term, "Re-indexing")) {
                Ok(idx) => {
                    self.idx = idx;
                    self.failed.clear();
                    self.results = None;
                    self.state.select(Some(0));
                    self.rebuild_view(None);
                    self.status = format!("Done. Dump is now {}.", fmt_size(sz));
                }
                Err(e) => self.status = format!("Written, but re-index failed: {e}"),
            },
            Err(e) => self.status = format!("Error: {e}"),
        }
    }

    fn run_test(&mut self, term: &mut DefaultTerminal) {
        self.mode = Mode::Normal;
        let res = ops::test_import(
            &self.path,
            &self.idx,
            &self.mysql_args,
            busy(term, "Importing into MySQL (--force)"),
        );
        match res {
            Ok(r) => {
                self.failed.clear();
                for g in &r.groups {
                    if let Some(o) = g.owner {
                        self.failed.insert(o, g.count);
                    }
                }
                if r.groups.is_empty() && r.other.is_empty() && r.exit_ok {
                    self.status = "Import finished without errors.".into();
                    self.results = None;
                } else {
                    self.scroll = 0;
                    self.results = Some(r);
                    self.mode = Mode::Results;
                }
            }
            Err(e) => self.status = format!("Test import failed: {e}"),
        }
    }
}

fn run(term: &mut DefaultTerminal, args: Args) -> Result<()> {
    let idx = index::build_index(&args.path, busy(term, "Indexing dump"))
        .with_context(|| format!("reading {}", args.path.display()))?;
    let mut app = App {
        path: args.path,
        idx,
        mysql_args: args.mysql_args,
        view: vec![],
        sort: Sort::File,
        filter: String::new(),
        state: TableState::default().with_selected(Some(0)),
        mode: Mode::Normal,
        status: String::new(),
        failed: HashMap::new(),
        results: None,
        scroll: 0,
    };
    app.rebuild_view(None);
    loop {
        term.draw(|f| app.draw(f))?;
        if let Event::Key(k) = event::read()? {
            if k.kind != KeyEventKind::Press {
                continue; // Windows sends Press and Release
            }
            if app.on_key(k, term)? {
                return Ok(());
            }
        }
    }
}

fn headless(a: &Args) -> Result<()> {
    let mut idx = index::build_index(&a.path, |d, t| {
        eprint!("\rIndexing {:>3}%", d * 100 / t.max(1))
    })?;
    eprintln!("\rIndexing done.    ");
    if a.list {
        for it in &idx.items {
            println!(
                "{}\t{}\t{}\t{}",
                it.kind.label(),
                it.name,
                it.lines,
                it.bytes
            );
        }
    }
    if a.test {
        if a.mysql_args.is_empty() {
            bail!("--test needs --mysql-args");
        }
        for it in idx.items.iter_mut().filter(|i| a.exclude.contains(&i.name)) {
            it.marked = true;
        }
        let r = ops::test_import(&a.path, &idx, &a.mysql_args, |d, t| {
            eprint!("\rImporting {:>3}%", d * 100 / t.max(1))
        })?;
        eprintln!();
        for g in &r.groups {
            match g.owner {
                Some(o) => println!(
                    "{}\t{}\t{} errors\t{}",
                    idx.items[o].kind.label(),
                    idx.items[o].name,
                    g.count,
                    g.first
                ),
                None => println!("(outside items)\t{} errors\t{}", g.count, g.first),
            }
        }
        for o in &r.other {
            println!("mysql: {o}");
        }
    }
    if !a.remove.is_empty() {
        for name in &a.remove {
            let mut hit = false;
            for it in idx.items.iter_mut().filter(|i| &i.name == name) {
                it.marked = true;
                hit = true;
            }
            if !hit {
                bail!("no item named `{name}`");
            }
        }
        let prog = |d: u64, t: u64| eprint!("\rWriting {:>3}%", d * 100 / t.max(1));
        if a.in_place {
            ops::write_in_place(&a.path, &idx, prog)?;
        } else {
            let out = a.out.clone().unwrap_or_else(|| default_out(&a.path));
            ops::write_new(&a.path, &out, &idx, prog)?;
            eprintln!("\nWrote {}", out.display());
        }
        eprintln!();
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = parse_args()?;
    if args.list || args.test || !args.remove.is_empty() {
        return headless(&args);
    }
    let mut term = ratatui::init();
    let res = run(&mut term, args);
    ratatui::restore();
    res
}
