use std::{fs::read_to_string, path::PathBuf};

pub use app::App;
use serde::Deserialize;

pub mod app;
mod view;

#[derive(Debug, Clone, Copy, Default)]
pub enum Stream {
    #[default]
    Stderr,
    Stdout,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Item<'a> {
    #[serde(rename = "test")]
    Test { name: &'a str, event: &'a str },
    #[serde(rename = "suite")]
    Suite {
        failed: u32,
        passed: u32,
        ignored: u32,
    },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Stats {
    failed: u32,
    passed: u32,
    ignored: u32,
}

fn parse_events(events: &str) -> (Vec<&str>, Stats) {
    let lines = events
        .lines()
        .filter_map(|x| serde_json::from_str::<Item>(x).ok())
        .skip(1);

    let mut failed = vec![];
    let mut stats = None;
    let mut ok_count = 0;

    for event in lines {
        match event {
            Item::Test { name, event } => {
                if event != "failed" {
                    ok_count += 1;
                    continue;
                }
                let Some((_, path)) = name.split_once("[ui] ") else {
                    // It's not UI test
                    continue;
                };
                failed.push(path);
            }
            Item::Suite {
                failed,
                passed,
                ignored,
            } => {
                stats = Some(Stats {
                    failed,
                    passed,
                    ignored,
                })
            }
        }
    }

    let stats = stats.unwrap_or(Stats {
        failed: failed.len() as u32,
        passed: ok_count,
        ignored: 0,
    });

    (failed, stats)
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let file = std::env::args()
        .nth(1)
        .unwrap_or("/home/ardi/repos/rust/blah.json".to_owned());
    let test_data = read_to_string(&file)
        .expect("Can't find json output")
        .leak();
    let (paths, stats) = parse_events(test_data);

    if paths.is_empty() {
        println!(
            "No failed tests: {} ok and {} ignored",
            stats.passed, stats.ignored
        );
        return Ok(());
    }

    let terminal = ratatui::init();
    let app = App {
        paths,
        stats,
        rust_path: PathBuf::from("/home/ardi/repos/rust"),
        ..Default::default()
    };
    let result = app.run(terminal);
    ratatui::restore();
    result
}
