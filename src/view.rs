use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

use crate::app::{Config, DiffMode, TestData};
use crate::Stats;

pub struct FullLayout {
    pub top_bar: Rect,
    pub diff_show: DiffShow,
    pub help_bar: Option<Rect>,
}

impl FullLayout {
    pub fn new(cfg: &Config, area: Rect) -> Self {
        let rects = Layout::default()
            .direction(Direction::Vertical)
            .constraints(if !cfg.hide_help {
                [
                    Constraint::Min(1),
                    Constraint::Percentage(100),
                    Constraint::Min(1),
                ]
                .as_slice()
            } else {
                [Constraint::Min(1), Constraint::Percentage(100)].as_slice()
            })
            .split(area);
        let diff_show = DiffShow::new(cfg.show_mode, rects[1]);
        Self {
            top_bar: rects[0],
            diff_show,
            help_bar: if !cfg.hide_help { Some(rects[2]) } else { None },
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum ShowMode {
    #[default]
    Vertical,
    VerticalOnly,
    SideBySide,
    SideBySideOnly,
    RustcArgs {
        oneline: bool,
    },
}

#[derive(Debug, Clone)]
pub enum DiffShow {
    Vertical { code: Rect, diff: Rect },
    VerticalOnly { diff: Rect },
    SideBySide { code: Rect, rhs: Rect, lhs: Rect },
    SideBySideOnly { rhs: Rect, lhs: Rect },
    RustcArgs { args: Rect, oneline: bool },
}

impl DiffShow {
    pub fn new(mode: ShowMode, rect: Rect) -> Self {
        match mode {
            ShowMode::SideBySide => {
                let layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(rect);
                let diff_layout = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(layout[1]);
                Self::SideBySide {
                    code: layout[0],
                    rhs: diff_layout[0],
                    lhs: diff_layout[1],
                }
            }
            ShowMode::SideBySideOnly => {
                let diff_layout = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(rect);
                Self::SideBySideOnly {
                    rhs: diff_layout[0],
                    lhs: diff_layout[1],
                }
            }
            ShowMode::Vertical => {
                let layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(rect);
                Self::Vertical {
                    code: layout[0],
                    diff: layout[1],
                }
            }
            ShowMode::VerticalOnly => DiffShow::VerticalOnly { diff: rect },
            ShowMode::RustcArgs { oneline } => DiffShow::RustcArgs {
                args: rect,
                oneline,
            },
        }
    }
}
