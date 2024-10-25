use std::{
    fs::read_to_string,
    mem,
    path::{Path, PathBuf},
    process::exit,
};

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    style::Stylize,
    text::{Line, Span, Text},
    widgets::{Block, Paragraph},
    DefaultTerminal, Frame,
};

use crate::{
    view::{DiffShow, FullLayout, ShowMode},
    Stats, Stream,
};

#[derive(Debug, Clone)]
pub struct TestData {
    pub test_code: String,
    pub actual: String,
    pub expect: String,
    pub stream: Stream,
    pub test_name: String,
    // TODO: I don't know how to extract it from stderr... probably need to modify compiletest
    pub rustc_args: String,
    // Used for blessing
    pub expected_path: PathBuf,
}

#[derive(Debug, Default)]
pub struct App {
    /// Is the application running?
    pub running: bool,
    pub config: Config,
    pub prev_view: ShowMode,
    pub current_test: usize,
    pub current_stream: Stream,
    pub stats: Stats,
    pub paths: Vec<&'static str>,
    pub cached_streams: CachedStreams, // This caches the loading of the test data
    pub rust_path: PathBuf,

    pub scroll_pos_diff: u16,
    pub scroll_pos_code: u16,
}

#[derive(Debug, Clone, Default)]
enum CachedData {
    #[default]
    Unloaded,
    Missing,
    Present(TestData),
}

#[derive(Debug, Clone, Default)]
pub struct CachedStreams {
    stderr: CachedData,
    stdout: CachedData,
}

impl App {
    pub fn load_curr_data(&mut self) {
        let path_str = self.paths[self.current_test];
        let path = Path::new(path_str);
        let test_code = self.rust_path.join(path);
        let expected_stderr_path = test_code.with_extension("stderr");
        let expected_stdout_path = test_code.with_extension("stdout");
        let target_path = path
            // In the build it has the path test instead of tests
            .strip_prefix("tests/")
            .expect("Path didn't start with tests/");
        // FIXME: get the actual triplet
        let actual_path = self
            .rust_path
            .join("build/x86_64-unknown-linux-gnu/test")
            .join(target_path)
            .with_extension("")
            .join(path.file_stem().unwrap());
        let actual_stderr = actual_path.with_extension("stderr");
        let actual_stdout = actual_path.with_extension("stdout");

        let Ok(test_code) = read_to_string(&test_code) else {
            // TODO: Handle this
            self.cached_streams.stderr = CachedData::Missing;
            self.cached_streams.stdout = CachedData::Missing;
            return;
        };
        let expected_stderr = read_to_string(&expected_stderr_path).ok();
        let expected_stdout = read_to_string(&expected_stdout_path).ok();
        let actual_stderr = read_to_string(actual_stderr).ok();
        let actual_stdout = read_to_string(actual_stdout).ok();

        if expected_stderr.is_some() || actual_stderr.is_some() && expected_stderr != actual_stderr
        {
            let actual = actual_stderr.unwrap_or_default();
            let expect = expected_stderr.unwrap_or_default();
            let stream = TestData {
                test_code: test_code.clone(),
                actual,
                expect,
                stream: Stream::Stderr,
                test_name: path_str.to_owned(),
                rustc_args: "TODO".to_owned(),
                expected_path: expected_stderr_path,
                // TODO: Where do I get this info
                // number_of_errs: 1,
            };
            self.cached_streams.stderr = CachedData::Present(stream);
        } else {
            self.cached_streams.stderr = CachedData::Missing;
        }

        if expected_stdout.is_some() || actual_stdout.is_some() && expected_stdout != actual_stdout
        {
            let actual = actual_stdout.unwrap_or_default();
            let expect = expected_stdout.unwrap_or_default();
            let stream = TestData {
                test_code: test_code.clone(),
                actual,
                expect,
                stream: Stream::Stdout,
                test_name: path_str.to_owned(),
                rustc_args: "TODO".to_owned(),
                expected_path: expected_stdout_path,
                // TODO: Where do I get this info
                // number_of_outs: 1,
            };
            self.cached_streams.stdout = CachedData::Present(stream);
        } else {
            self.cached_streams.stdout = CachedData::Missing;
            // if matches!(self.cached_streams.stderr, CachedData::Missing) {
            //     unreachable!("what");
            // }
        }
    }

    pub fn advance_test(&mut self) {
        self.reset_scroll();
        self.current_stream = Stream::Stderr;
        self.current_test += 1;
        self.cached_streams = Default::default();
        if self.current_test == self.paths.len() {
            ratatui::restore();
            exit(0);
        }
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_pos_code = 0;
        self.scroll_pos_diff = 0;
    }

    pub fn advance_stream(&mut self) {
        self.reset_scroll();
        match self.current_stream {
            Stream::Stderr => self.current_stream = Stream::Stdout,
            Stream::Stdout => self.advance_test(),
        }
    }

    pub fn previous_test(&mut self) {
        self.reset_scroll();
        self.current_stream = Stream::Stderr;
        self.current_test = self.current_test.saturating_sub(1);
        self.cached_streams = Default::default();
    }

    pub fn request_curr_test(&mut self) -> &TestData {
        match self.current_stream {
            // We do this one first, then the other
            Stream::Stderr => match self.cached_streams.stderr {
                CachedData::Unloaded => {
                    self.load_curr_data();
                    self.request_curr_test()
                }
                CachedData::Missing => {
                    self.current_stream = Stream::Stdout;
                    self.request_curr_test()
                }
                CachedData::Present(ref test_data) => test_data,
            },
            Stream::Stdout => match self.cached_streams.stdout {
                CachedData::Missing => {
                    self.advance_test();
                    self.request_curr_test()
                }
                CachedData::Present(ref test_data) => test_data,
                CachedData::Unloaded => unreachable!(),
            },
        }
    }

    /// Run the application's main loop.
    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        self.running = true;
        while self.running {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_crossterm_events()?;
        }
        Ok(())
    }

    /// Renders the user interface.
    ///
    /// This is where you add new widgets. See the following resources for more information:
    /// - <https://docs.rs/ratatui/latest/ratatui/widgets/index.html>
    /// - <https://github.com/ratatui/ratatui/tree/master/examples>
    fn draw(&mut self, frame: &mut Frame) {
        fn mk_paragraph<'a>(title: &'a str, text: impl Into<Text<'a>>) -> Paragraph<'a> {
            let line = Line::from(title).bold().centered();
            let block = Block::bordered().title_top(line);
            Paragraph::new(text).block(block)
        }

        let layout = FullLayout::new(&self.config, frame.area());
        let current_test = self.current_test;
        let total_tests = self.stats.failed;
        let ok = self.stats.passed;
        let ignored = self.stats.ignored;
        let diff_mode = self.config.diff_mode;
        let scroll_code = (self.scroll_pos_code, 0);
        let scroll_diff = (self.scroll_pos_diff, 0);
        let diff_mode = self.config.diff_mode;

        if let Some(rect) = layout.help_bar {
            frame.render_widget(Paragraph::new(self.config.help_string()).centered(), rect);
        }

        let TestData {
            actual,
            expect,
            stream,
            test_name,
            rustc_args,
            test_code,
            expected_path,
        } = self.request_curr_test();

        let top_bar_text = format!("Showing {test_name} {stream:?}. {current_test}/{total_tests}.  Ok: {ok}, Ignored: {ignored}");
        frame.render_widget(Paragraph::new(top_bar_text).centered(), layout.top_bar);

        match layout.diff_show {
            DiffShow::SideBySide { code, lhs, rhs } => {
                let (expect, actual) = diff_vertical(expect, actual, diff_mode);
                frame.render_widget(
                    mk_paragraph("code", test_code.as_str()).scroll(scroll_code),
                    code,
                );
                frame.render_widget(mk_paragraph("expected", expect).scroll(scroll_diff), lhs);
                frame.render_widget(mk_paragraph("actual", actual).scroll(scroll_diff), rhs);
            }
            DiffShow::SideBySideOnly { rhs, lhs } => {
                let (expect, actual) = diff_vertical(expect, actual, diff_mode);
                frame.render_widget(mk_paragraph("expected", expect).scroll(scroll_diff), lhs);
                frame.render_widget(mk_paragraph("actual", actual).scroll(scroll_diff), rhs);
            }
            DiffShow::Vertical { code, diff } => {
                let tx_diff = diff_horizontal(expect, actual, diff_mode);
                frame.render_widget(
                    mk_paragraph("code", test_code.as_str()).scroll(scroll_code),
                    code,
                );
                frame.render_widget(mk_paragraph("diff", tx_diff).scroll(scroll_diff), diff);
            }
            DiffShow::VerticalOnly { diff } => {
                let tx_diff = diff_horizontal(expect, actual, diff_mode);
                frame.render_widget(mk_paragraph("diff", tx_diff).scroll(scroll_diff), diff);
            }
            DiffShow::RustcArgs { args, oneline } => {
                // This could be done inline but idc.
                let text = if oneline {
                    rustc_args.to_owned()
                } else {
                    rustc_args.replace(' ', "\n")
                };

                frame.render_widget(mk_paragraph("rustc arguments", text.as_str()), args);
            }
        };
    }

    /// Reads the crossterm events and updates the state of [`App`].
    ///
    /// If your application needs to perform work in between handling events, you can use the
    /// [`event::poll`] function to check if there are any events available with a timeout.
    fn handle_crossterm_events(&mut self) -> Result<()> {
        match event::read()? {
            // it's important to check KeyEventKind::Press to avoid handling key release events
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key_event(key),
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
            _ => {}
        }
        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    fn on_key_event(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.quit(),
            KeyCode::Char('d') => {
                self.config.diff_mode.rotate_next();
            }
            KeyCode::Char('r') => match self.config.show_mode {
                ShowMode::RustcArgs { oneline } => {
                    self.config.show_mode = self.prev_view;
                    self.prev_view = ShowMode::RustcArgs { oneline }
                }
                _ => {
                    self.prev_view = self.config.show_mode;
                    self.config.show_mode = ShowMode::RustcArgs { oneline: false }
                }
            },
            KeyCode::Char('o') => {
                if let ShowMode::RustcArgs { oneline } = self.config.show_mode {
                    self.config.show_mode = ShowMode::RustcArgs { oneline: !oneline }
                }
            }
            KeyCode::Char('p') => {
                mem::swap(&mut self.config.show_mode, &mut self.prev_view);
            }
            KeyCode::Char('h') => {
                self.config.hide_help = !self.config.hide_help;
            }
            KeyCode::Char('b') => {
                self.bless();
            }
            KeyCode::Char('n') => {
                self.advance_stream();
            }
            KeyCode::Char('N') => {
                self.previous_test();
            }
            KeyCode::Char('j') => {
                self.scroll_pos_diff += 1;
            }
            KeyCode::Char('k') => {
                self.scroll_pos_diff = self.scroll_pos_diff.saturating_sub(1);
            }
            KeyCode::Char('J') => {
                self.scroll_pos_code += 1;
            }
            KeyCode::Char('K') => {
                self.scroll_pos_code = self.scroll_pos_code.saturating_sub(1);
            }
            KeyCode::Char('c') => match self.config.show_mode {
                ShowMode::SideBySide => self.config.show_mode = ShowMode::SideBySideOnly,
                ShowMode::SideBySideOnly => self.config.show_mode = ShowMode::SideBySide,
                ShowMode::Vertical => self.config.show_mode = ShowMode::VerticalOnly,
                ShowMode::VerticalOnly => self.config.show_mode = ShowMode::Vertical,
                _ => {}
            },
            KeyCode::Char('s') => match self.config.show_mode {
                ShowMode::SideBySide => self.config.show_mode = ShowMode::Vertical,
                ShowMode::SideBySideOnly => self.config.show_mode = ShowMode::VerticalOnly,
                ShowMode::Vertical => self.config.show_mode = ShowMode::SideBySide,
                ShowMode::VerticalOnly => self.config.show_mode = ShowMode::SideBySideOnly,
                _ => {}
            },
            _ => {}
        };
    }

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
    }

    fn bless(&mut self) {
        let data = match self.current_stream {
            Stream::Stderr => &self.cached_streams.stderr,
            Stream::Stdout => &self.cached_streams.stdout,
        };
        if let CachedData::Present(data) = data {
            std::fs::write(&data.expected_path, &data.actual).unwrap();
        }
        self.advance_stream();
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Config {
    pub diff_mode: DiffMode,
    pub show_mode: ShowMode,
    pub hide_help: bool,
}

impl Config {
    pub fn help_string(&self) -> String {
        // FIXME: the colors don't work!
        // Need to use proper ratatui spans to fix this..
        let bless = format!("{}less", "b".blue().bold());
        let help = format!("{}elp toggle", "h".green().bold());
        let previous_mode = format!("{}revious mode", "p".yellow().bold());
        let rustc_args = format!("{}ustc args", "r".green().bold());
        let next_diff = format!(
            "next {}iff mode: {}",
            "d".red().bold(),
            self.diff_mode.next_text()
        );

        let vertical = format!("vertical {}how mode", "s".red().bold());
        let horizontal = format!("horizontal {}how mode", "s".red().bold());
        let show_code = format!("show {}ode", "c".magenta().bold());
        let hide_code = format!("hide {}ode", "c".magenta().bold());

        let show_mode_specific = match self.show_mode {
            ShowMode::SideBySide => {
                format!("{vertical} | {hide_code} | {next_diff} | {rustc_args}")
            }
            ShowMode::SideBySideOnly => {
                format!("{vertical} | {show_code} | {next_diff} | {rustc_args}")
            }
            ShowMode::Vertical => {
                format!("{horizontal} | {hide_code} | {next_diff} | {rustc_args}")
            }
            ShowMode::VerticalOnly => {
                format!("{horizontal} | {show_code} | {next_diff} | {rustc_args}")
            }
            ShowMode::RustcArgs { oneline } => {
                format!(
                    "{} {}neline",
                    "o".red(),
                    if oneline { "disable" } else { "enable" }
                )
            }
        };

        format!("{bless} | {show_mode_specific} | {previous_mode} | {help}")
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum DiffMode {
    #[default]
    Line,
    Word, // TODO: These are buggy
    Char,
}

impl DiffMode {
    pub const fn rotate_next(&mut self) {
        match self {
            DiffMode::Char => *self = DiffMode::Word,
            DiffMode::Word => *self = DiffMode::Line,
            DiffMode::Line => *self = DiffMode::Char,
        }
    }

    pub const fn next_text(&self) -> &'static str {
        match self {
            DiffMode::Char => "word",
            DiffMode::Word => "line",
            DiffMode::Line => "char",
        }
    }
}

fn diff_vertical_linewise<'a>(lhs: &'a str, rhs: &'a str) -> (Text<'a>, Text<'a>) {
    let diff = similar::TextDiff::from_lines(lhs, rhs);

    let mut lhs: Vec<Line<'_>> = vec![];
    let mut rhs: Vec<Line<'_>> = vec![];
    for hunk in diff.iter_all_changes() {
        match hunk.tag() {
            similar::ChangeTag::Equal => {
                lhs.push(hunk.value().into());
                rhs.push(hunk.value().into());
            }
            similar::ChangeTag::Delete => {
                lhs.push(hunk.value().red().into());
            }
            similar::ChangeTag::Insert => {
                rhs.push(hunk.value().green().bold().into());
            }
        }
    }

    (lhs.into(), rhs.into())
}

fn diff_vertical<'a>(lhs: &'a str, rhs: &'a str, diffmode: DiffMode) -> (Text<'a>, Text<'a>) {
    let diff = match diffmode {
        DiffMode::Char => similar::TextDiff::from_chars(lhs, rhs),
        DiffMode::Word => similar::TextDiff::from_words(lhs, rhs),
        DiffMode::Line => return diff_vertical_linewise(lhs, rhs),
    };

    let diff = similar::TextDiffConfig::default()
        .newline_terminated(true)
        .diff_words(lhs, rhs);

    let mut lhs: Vec<Span<'_>> = vec![];
    let mut rhs: Vec<Span<'_>> = vec![];
    for hunk in diff.iter_all_changes() {
        match hunk.tag() {
            similar::ChangeTag::Equal => {
                lhs.push(hunk.value().into());
                rhs.push(hunk.value().into());
            }
            similar::ChangeTag::Delete => {
                lhs.push(hunk.value().red());
            }
            similar::ChangeTag::Insert => {
                rhs.push(hunk.value().green());
            }
        }
    }
    (Line::from(rhs).into(), Line::from(lhs).into())
}

fn diff_horizontal_linewise<'a>(lhs: &'a str, rhs: &'a str) -> Text<'a> {
    let diff = similar::TextDiff::from_lines(lhs, rhs);

    let mut text: Vec<Line<'_>> = vec![];
    for hunk in diff.iter_all_changes() {
        match hunk.tag() {
            similar::ChangeTag::Equal => {
                text.push(hunk.value().into());
            }
            similar::ChangeTag::Delete => {
                text.push(hunk.value().red().into());
            }
            similar::ChangeTag::Insert => {
                text.push(hunk.value().green().into());
            }
        }
    }
    text.into()
}

fn diff_horizontal<'a>(lhs: &'a str, rhs: &'a str, diffmode: DiffMode) -> Text<'a> {
    let diff = match diffmode {
        DiffMode::Char => similar::TextDiff::from_chars(lhs, rhs),
        DiffMode::Word => similar::TextDiff::from_words(lhs, rhs),
        DiffMode::Line => return diff_horizontal_linewise(lhs, rhs),
    };
    let mut text: Vec<Span<'_>> = vec![];
    for hunk in diff.iter_all_changes() {
        match hunk.tag() {
            similar::ChangeTag::Equal => {
                text.push(hunk.value().into());
            }
            similar::ChangeTag::Delete => {
                text.push(hunk.value().red());
            }
            similar::ChangeTag::Insert => {
                text.push(hunk.value().green());
            }
        }
    }
    Line::from(text).into()
}
