//! Local quotes for the same board the CTP feed updates.

use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::quote::{valid_price, Quote};

#[derive(Default)]
pub struct Book {
    walks: HashMap<String, Walk>,
    replay: Vec<Quote>,
    replay_at: usize,
}

impl Book {
    pub fn with_replay(path: Option<&Path>) -> (Self, Option<String>) {
        let Some(path) = path else {
            return (Self::default(), None);
        };
        match load_replay(path) {
            Ok(replay) => (
                Self {
                    replay,
                    ..Self::default()
                },
                None,
            ),
            Err(err) => (
                Self::default(),
                Some(format!("回放文件不可用 ({err})，改用模拟游走")),
            ),
        }
    }

    pub fn quote(&mut self, instrument: &str) -> Quote {
        if !self.replay.is_empty() {
            for _ in 0..self.replay.len() {
                let index = self.replay_at % self.replay.len();
                self.replay_at += 1;
                if self.replay[index].instrument == instrument {
                    let mut quote = self.replay[index].clone();
                    if quote.update_time.is_empty() {
                        quote.update_time = china_now();
                    }
                    return quote;
                }
            }
        }
        self.walks
            .entry(instrument.to_owned())
            .or_insert_with(|| Walk::new(instrument))
            .next_quote()
    }
}

struct Walk {
    instrument: String,
    state: u64,
    price: f64,
    pre: f64,
    tick: f64,
    volume: i32,
    open_interest: f64,
}

impl Walk {
    fn new(instrument: &str) -> Self {
        let pre = base_price(instrument);
        Self {
            instrument: instrument.to_owned(),
            state: seed_of(instrument),
            price: pre,
            pre,
            tick: tick_size(instrument),
            volume: 1000,
            open_interest: 100_000.0,
        }
    }

    fn next_quote(&mut self) -> Quote {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let steps = ((self.state >> 33) as i32 % 5) - 2;
        self.price = (self.price + f64::from(steps) * self.tick).max(self.tick);
        self.volume += 1 + (self.state as i32 & 3);
        self.open_interest += f64::from((self.state as i32 & 7) - 3);
        Quote {
            instrument: self.instrument.clone(),
            last: Some(self.price),
            bid: Some(self.price - self.tick),
            ask: Some(self.price + self.tick),
            bid_vol: 1 + (self.state as i32 & 15),
            ask_vol: 1 + ((self.state >> 4) as i32 & 15),
            volume: self.volume,
            open_interest: self.open_interest.max(0.0),
            pre_settlement: Some(self.pre),
            update_time: china_now(),
        }
    }
}

pub fn china_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
        .saturating_add(8 * 3600);
    let clock = secs % 86_400;
    format!(
        "{:02}:{:02}:{:02}",
        clock / 3600,
        (clock % 3600) / 60,
        clock % 60
    )
}

fn seed_of(instrument: &str) -> u64 {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for byte in instrument.bytes() {
        seed = seed.wrapping_mul(131).wrapping_add(u64::from(byte));
    }
    seed
}

fn base_price(instrument: &str) -> f64 {
    let id = instrument.to_ascii_lowercase();
    if id.starts_with("rb") {
        3500.0
    } else if id.starts_with("ag") {
        7800.0
    } else if id.starts_with("au") {
        560.0
    } else if id.starts_with("cu") {
        76_000.0
    } else {
        1000.0
    }
}

fn tick_size(instrument: &str) -> f64 {
    let id = instrument.to_ascii_lowercase();
    if id.starts_with("cu") {
        10.0
    } else if id.starts_with("au") {
        0.02
    } else {
        1.0
    }
}

#[derive(Debug, Deserialize)]
struct ReplayRow {
    instrument: String,
    #[serde(default)]
    last: Option<f64>,
    #[serde(default)]
    bid: Option<f64>,
    #[serde(default)]
    ask: Option<f64>,
    #[serde(default)]
    bid_vol: i32,
    #[serde(default)]
    ask_vol: i32,
    #[serde(default)]
    volume: i32,
    #[serde(default)]
    open_interest: f64,
    #[serde(default)]
    pre_settlement: Option<f64>,
    #[serde(default)]
    update_time: String,
}

pub fn load_replay(path: &Path) -> Result<Vec<Quote>, String> {
    let text = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    parse_replay(&text)
}

pub fn parse_replay(text: &str) -> Result<Vec<Quote>, String> {
    let mut quotes = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: ReplayRow = serde_json::from_str(line)
            .map_err(|err| format!("{} 第 {} 行: {err}", "回放", index + 1))?;
        quotes.push(Quote {
            instrument: row.instrument,
            last: row.last.and_then(valid_price),
            bid: row.bid.and_then(valid_price),
            ask: row.ask.and_then(valid_price),
            bid_vol: row.bid_vol,
            ask_vol: row.ask_vol,
            volume: row.volume,
            open_interest: row.open_interest,
            pre_settlement: row.pre_settlement.and_then(valid_price),
            update_time: row.update_time,
        });
    }
    Ok(quotes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_fixture_and_walk_move() {
        let quotes = parse_replay(include_str!("../fixtures/demo-replay.jsonl")).unwrap();
        assert_eq!(quotes[0].instrument, "rb2610");
        assert_eq!(quotes[0].change(), Some(12.0));
        let mut book = Book::default();
        let first = book.quote("rb2610");
        let second = book.quote("rb2610");
        assert_eq!(first.instrument, "rb2610");
        assert!(first.last.is_some());
        assert_ne!(first.last, second.last);
    }
}
