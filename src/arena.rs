use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::Field;

/// LMArena text leaderboard parquet (Hugging Face `lmarena-ai/leaderboard-dataset`).
const PARQUET_URL: &str = "https://huggingface.co/datasets/lmarena-ai/leaderboard-dataset/resolve/main/text/latest-00000-of-00001.parquet";
const MAX_ATTEMPTS: usize = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub model_name: String,
    pub rating: f64,
    pub rank: f64,
    pub vote_count: f64,
}

/// A matched arena rating for an Azure model row.
#[derive(Debug, Clone, PartialEq)]
pub struct Intel {
    pub name: String,
    pub rating: f64,
    pub rank: u32,
}

pub fn spawn_fetch(tx: Sender<crate::api::FetchMsg>) {
    thread::spawn(move || match fetch_blocking() {
        Ok(entries) => {
            let _ = tx.send(crate::api::FetchMsg::Arena(entries));
        }
        Err(e) => {
            let _ = tx.send(crate::api::FetchMsg::ArenaError(format!("{e:#}")));
        }
    });
}

pub fn fetch_blocking() -> Result<Vec<Entry>> {
    let bytes = download()?;
    parse_parquet(bytes)
}

fn download() -> Result<Bytes> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let mut last = String::new();
    for attempt in 0..MAX_ATTEMPTS {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(500 * attempt as u64));
        }
        match client.get(PARQUET_URL).send() {
            Ok(resp) if resp.status().is_success() => match resp.bytes() {
                Ok(b) => return Ok(b),
                Err(e) => last = format!("{e}"),
            },
            Ok(resp) => last = format!("HTTP {}", resp.status()),
            Err(e) => last = format!("{e}"),
        }
    }
    bail!("arena download failed after {MAX_ATTEMPTS} attempts: {last}")
}

fn parse_parquet(bytes: Bytes) -> Result<Vec<Entry>> {
    let reader = SerializedFileReader::new(bytes).context("could not read LMArena parquet")?;
    let mut out = Vec::new();
    for row in reader.get_row_iter(None)? {
        let row = row?;
        let mut name = String::new();
        let mut rating = None;
        let mut rank = None;
        let mut votes = None;
        let mut overall = false;
        for (field, value) in row.get_column_iter() {
            match field.as_str() {
                "model_name" => name = as_str(value).unwrap_or_default().to_string(),
                "rating" => rating = as_f64(value),
                "rank" => rank = as_f64(value),
                "vote_count" => votes = as_f64(value),
                "category" => overall = as_str(value) == Some("overall"),
                _ => {}
            }
        }
        if overall && !name.is_empty() {
            out.push(Entry {
                model_name: name,
                rating: rating.unwrap_or(0.0),
                rank: rank.unwrap_or(0.0),
                vote_count: votes.unwrap_or(0.0),
            });
        }
    }
    if out.is_empty() {
        bail!("arena leaderboard contained no 'overall' rows");
    }
    Ok(out)
}

fn as_str(field: &Field) -> Option<&str> {
    match field {
        Field::Str(s) => Some(s),
        _ => None,
    }
}

fn as_f64(field: &Field) -> Option<f64> {
    match field {
        Field::Double(v) => Some(*v),
        Field::Float(v) => Some(*v as f64),
        Field::Long(v) => Some(*v as f64),
        Field::Int(v) => Some(*v as f64),
        _ => None,
    }
}

pub struct ArenaIndex {
    entries: Vec<Entry>,
    names: Vec<String>,
}

impl ArenaIndex {
    pub fn new(entries: Vec<Entry>) -> Self {
        let names = entries.iter().map(|e| e.model_name.clone()).collect();
        Self { entries, names }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Conservative fuzzy match of an Azure (model, product) pair to an arena entry.
    pub fn lookup(&self, model: &str, product: &str) -> Option<Intel> {
        let (i, _) = crate::matching::best_index(&self.names, model, product)?;
        let e = &self.entries[i];
        Some(Intel {
            name: e.model_name.clone(),
            rating: e.rating,
            rank: e.rank.round() as u32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, rating: f64, rank: f64) -> Entry {
        Entry {
            model_name: name.into(),
            rating,
            rank,
            vote_count: 100.0,
        }
    }

    #[test]
    fn exact_and_date_suffix() {
        let idx = ArenaIndex::new(vec![
            entry("gpt-4.1-2025-04-14", 1382.8, 157.0),
            entry("deepseek-v4-pro", 1450.6, 43.0),
        ]);
        assert_eq!(
            idx.lookup("gpt-4.1", "Azure OpenAI").unwrap().name,
            "gpt-4.1-2025-04-14"
        );
        assert_eq!(
            idx.lookup("v4-pro", "Azure Deepseek Models").unwrap().name,
            "deepseek-v4-pro"
        );
    }

    #[test]
    fn rejects_version_bump() {
        let idx = ArenaIndex::new(vec![entry("grok-4.5", 1450.1, 42.0)]);
        assert!(idx.lookup("grok-4", "Azure Grok Models").is_none());
    }

    #[test]
    fn rejects_distinct_variant() {
        let idx = ArenaIndex::new(vec![
            entry("gpt-5.1", 1422.6, 100.0),
            entry("o1-preview", 1352.7, 191.0),
        ]);
        assert!(idx.lookup("5.1-codex", "Azure OpenAI GPT5").is_none());
        assert!(idx.lookup("o1", "Azure OpenAI").is_none());
    }

    #[test]
    fn accepts_effort_variant() {
        let idx = ArenaIndex::new(vec![entry("gpt-5.4-mini-high", 1412.1, 127.0)]);
        assert_eq!(
            idx.lookup("5.4-mini", "Azure OpenAI GPT5").unwrap().name,
            "gpt-5.4-mini-high"
        );
    }

    #[test]
    fn accepts_variant_suffix() {
        let idx = ArenaIndex::new(vec![entry("deepseek-v3.2", 1424.8, 96.0)]);
        assert_eq!(
            idx.lookup("v3.2-sp", "Azure Deepseek Models").unwrap().name,
            "deepseek-v3.2"
        );
    }

    #[test]
    fn no_match_for_unknown() {
        let idx = ArenaIndex::new(vec![entry("gpt-5.1", 1422.6, 100.0)]);
        assert!(idx.lookup("codestral", "Azure Mistral Models").is_none());
    }
}
