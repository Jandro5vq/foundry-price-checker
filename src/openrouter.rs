use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};
use serde::Deserialize;

const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
const MAX_ATTEMPTS: usize = 4;

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<Model>,
}

#[derive(Debug, Deserialize)]
struct Model {
    #[serde(default)]
    id: String,
    #[serde(default)]
    context_length: u64,
    #[serde(default)]
    architecture: Architecture,
    #[serde(default)]
    supported_parameters: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Architecture {
    #[serde(default)]
    input_modalities: Vec<String>,
    #[serde(default)]
    output_modalities: Vec<String>,
}

/// Capabilities + context window for a model, from OpenRouter.
#[derive(Debug, Clone, PartialEq)]
pub struct Caps {
    pub context: u64,
    pub vision: bool,
    pub audio: bool,
    pub tools: bool,
    pub reasoning: bool,
    pub json: bool,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub slug: String,
    pub context: u64,
    pub vision: bool,
    pub audio: bool,
    pub tools: bool,
    pub reasoning: bool,
    pub json: bool,
}

pub fn spawn_fetch(tx: Sender<crate::api::FetchMsg>) {
    thread::spawn(move || match fetch_blocking() {
        Ok(entries) => {
            let _ = tx.send(crate::api::FetchMsg::Caps(entries));
        }
        Err(e) => {
            let _ = tx.send(crate::api::FetchMsg::CapsError(format!("{e:#}")));
        }
    });
}

pub fn fetch_blocking() -> Result<Vec<Entry>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let mut last = String::new();
    for attempt in 0..MAX_ATTEMPTS {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(500 * attempt as u64));
        }
        match client.get(MODELS_URL).send() {
            Ok(resp) if resp.status().is_success() => match resp.json::<ModelsResponse>() {
                Ok(body) => return Ok(build_entries(body.data)),
                Err(e) => last = format!("bad json: {e}"),
            },
            Ok(resp) => last = format!("HTTP {}", resp.status()),
            Err(e) => last = format!("{e}"),
        }
    }
    bail!("openrouter request failed after {MAX_ATTEMPTS} attempts: {last}")
}

fn build_entries(models: Vec<Model>) -> Vec<Entry> {
    // Prefer base ids (no `:variant`) when slugs collide, so sort those first.
    let mut models = models;
    models.sort_by_key(|m| m.id.contains(':'));
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for m in models {
        let slug = m.id.rsplit('/').next().unwrap_or(&m.id);
        let slug = slug.split(':').next().unwrap_or(slug).to_string();
        if slug.is_empty() || !seen.insert(slug.clone()) {
            continue;
        }
        let has = |list: &[String], v: &str| list.iter().any(|x| x == v);
        out.push(Entry {
            context: m.context_length,
            vision: has(&m.architecture.input_modalities, "image")
                || has(&m.architecture.output_modalities, "image"),
            audio: has(&m.architecture.input_modalities, "audio")
                || has(&m.architecture.output_modalities, "audio"),
            tools: has(&m.supported_parameters, "tools"),
            reasoning: has(&m.supported_parameters, "reasoning")
                || has(&m.supported_parameters, "include_reasoning"),
            json: has(&m.supported_parameters, "structured_outputs")
                || has(&m.supported_parameters, "response_format"),
            slug,
        });
    }
    out
}

pub struct CapsIndex {
    entries: Vec<Entry>,
    names: Vec<String>,
}

impl CapsIndex {
    pub fn new(entries: Vec<Entry>) -> Self {
        let names = entries.iter().map(|e| e.slug.clone()).collect();
        Self { entries, names }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn lookup(&self, model: &str, product: &str) -> Option<Caps> {
        let (i, _) = crate::matching::best_index(&self.names, model, product)?;
        let e = &self.entries[i];
        Some(Caps {
            context: e.context,
            vision: e.vision,
            audio: e.audio,
            tools: e.tools,
            reasoning: e.reasoning,
            json: e.json,
            source: e.slug.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_dedupes() {
        let json = r#"{"data":[
            {"id":"openai/gpt-5.1:batch","context_length":400000,
             "architecture":{"input_modalities":["image","text"],"output_modalities":["text"]},
             "supported_parameters":["tools","reasoning","structured_outputs"]},
            {"id":"openai/gpt-5.1","context_length":400000,
             "architecture":{"input_modalities":["image","text"],"output_modalities":["text"]},
             "supported_parameters":["tools","reasoning"]},
            {"id":"deepseek/deepseek-v4-pro","context_length":1048576,
             "architecture":{"input_modalities":["text"],"output_modalities":["text"]},
             "supported_parameters":["tools"]}
        ]}"#;
        let body: ModelsResponse = serde_json::from_str(json).unwrap();
        let entries = build_entries(body.data);
        assert_eq!(entries.len(), 2);
        let idx = CapsIndex::new(entries);
        let c = idx.lookup("gpt-5.1", "Azure OpenAI GPT5").unwrap();
        assert_eq!(c.context, 400_000);
        assert!(c.vision && c.tools && c.reasoning);
        assert!(!c.audio);
        let d = idx.lookup("v4-pro", "Azure Deepseek Models").unwrap();
        assert_eq!(d.context, 1_048_576);
    }
}
