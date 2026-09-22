use std::collections::HashMap;

use regex::Regex;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Item {
    #[serde(rename = "retailPrice", default)]
    pub retail_price: f64,
    #[serde(rename = "meterName", default)]
    pub meter_name: String,
    #[serde(rename = "productName", default)]
    pub product_name: String,
    #[serde(rename = "skuName", default)]
    pub sku_name: String,
    #[serde(rename = "unitOfMeasure", default)]
    pub unit_of_measure: String,
    #[serde(rename = "type", default)]
    pub meter_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub model: String,
    pub developer: String,
    pub deployment: String,
    pub mode: String,
    pub input: Option<f64>,
    pub cached: Option<f64>,
    pub output: Option<f64>,
    pub product: String,
    pub arena: Option<crate::arena::Intel>,
    pub caps: Option<crate::openrouter::Caps>,
}

/// Best-effort developer/vendor for a model, derived from the product and model name.
pub fn developer(model: &str, product: &str) -> &'static str {
    let m = model.to_lowercase();
    let p = product.to_lowercase();

    // Aggregator products host third-party models; refine by model prefix.
    if p.contains("fireworks") {
        return if m.starts_with("fw-deepseek") || m.starts_with("fw-ds") {
            "DeepSeek"
        } else if m.starts_with("fw-glm") {
            "Zhipu AI"
        } else if m.starts_with("fw-kimi") || m.starts_with("fw-k2") {
            "Moonshot AI"
        } else if m.starts_with("fw-minimax") {
            "MiniMax"
        } else if m.starts_with("fw-nvidia") || m.contains("nemotron") {
            "NVIDIA"
        } else if m.starts_with("fw-gpt-oss") {
            "OpenAI"
        } else {
            "Unknown"
        };
    }

    if p.contains("openai") || p.contains("gpt") {
        "OpenAI"
    } else if p.contains("deepseek") {
        "DeepSeek"
    } else if p.contains("llama") {
        "Meta"
    } else if p.contains("grok") {
        "xAI"
    } else if p.contains("kimi") {
        "Moonshot AI"
    } else if p.contains("mistral") {
        "Mistral AI"
    } else if p.contains("qwen") {
        "Alibaba"
    } else if p.contains("cohere") {
        "Cohere"
    } else if p.contains("phi") || p.contains("mai") {
        "Microsoft"
    } else {
        "Unknown"
    }
}

fn direction_word(w: &str) -> Option<&'static str> {
    Some(match w {
        "cached" | "cchd" | "cache" | "cd" => "cached",
        "inp" | "input" | "in" | "prompt" => "input",
        "outp" | "output" | "out" | "completion" => "output",
        _ => return None,
    })
}

fn deployment_word(w: &str) -> Option<&'static str> {
    Some(match w {
        "glbl" | "global" | "gl" => "Global",
        "dzone" | "dz" | "datazone" => "Data Zone",
        "regnl" | "regional" | "rgnl" | "reg" => "Regional",
        _ => return None,
    })
}

fn is_noise(w: &str) -> bool {
    matches!(
        w,
        "tokens" | "token" | "tok" | "1k" | "1m" | "1" | "units" | "unit" | "data" | "zone"
    )
}

/// How many tokens one billing unit represents, or None if not a token meter.
pub fn tokens_per_unit(unit: &str) -> Option<f64> {
    let u = unit.replace(' ', "").to_uppercase();
    let re = Regex::new(r"^(\d+(?:\.\d+)?)([KM]?)$").unwrap();
    let caps = re.captures(&u)?;
    let n: f64 = caps.get(1)?.as_str().parse().ok()?;
    let mult = match caps.get(2).map(|m| m.as_str()).unwrap_or("") {
        "" => 1.0,
        "K" => 1_000.0,
        "M" => 1_000_000.0,
        _ => return None,
    };
    Some(n * mult)
}

/// Split a meter name into (model, direction, deployment, is_batch).
pub fn parse_meter(meter_name: &str, sku_name: &str) -> (String, Option<String>, String, bool) {
    let text = format!("{meter_name} {sku_name}").to_lowercase();
    let is_batch = text.contains("batch");

    let mut direction: Option<String> = None;
    let mut deployment: Option<String> = None;
    let mut model_parts: Vec<String> = Vec::new();

    for w in meter_name
        .to_lowercase()
        .split(|c: char| c.is_whitespace() || "-_/".contains(c))
    {
        if w.is_empty() {
            continue;
        }
        if let Some(d) = direction_word(w) {
            let entering = direction.is_none() || direction.as_deref() == Some("input");
            if entering {
                if direction.is_none() || d == "cached" {
                    direction = Some(d.to_string());
                }
                continue;
            }
            // Direction already resolved to a more specific label (e.g. cached):
            // fall through and let the word join the model name, matching test.py.
        }
        if let Some(dep) = deployment_word(w) {
            deployment = Some(dep.to_string());
            continue;
        }
        if is_noise(w) || w == "batch" {
            continue;
        }
        model_parts.push(w.to_string());
    }

    if text.contains("data zone") || text.contains("datazone") {
        deployment = Some("Data Zone".to_string());
    } else if deployment.is_none() {
        deployment = Some(if text.contains("global") || text.contains("glbl") {
            "Global".to_string()
        } else if text.contains("regional") || text.contains("regnl") {
            "Regional".to_string()
        } else {
            "Standard".to_string()
        });
    }

    (
        model_parts.join("-"),
        direction,
        deployment.unwrap(),
        is_batch,
    )
}

#[derive(Default, Clone, Copy)]
struct Prices {
    input: Option<f64>,
    cached: Option<f64>,
    output: Option<f64>,
}

/// Group raw items into model rows. Returns (rows, count of skipped non-token meters).
pub fn build_rows(items: &[Item], model_filter: Option<&str>) -> (Vec<Row>, usize) {
    let mut grouped: HashMap<(String, String, String, String), Prices> = HashMap::new();
    let mut other = 0usize;
    let filter = model_filter.map(|s| s.to_lowercase());

    for it in items {
        if it.meter_type != "Consumption" {
            continue;
        }
        let (model, direction, deployment, is_batch) = parse_meter(&it.meter_name, &it.sku_name);
        if let Some(f) = &filter {
            let haystack = format!("{}{}", model, it.product_name.to_lowercase());
            if !haystack.contains(f.as_str()) {
                continue;
            }
        }
        let (per_unit, direction) = match (tokens_per_unit(&it.unit_of_measure), direction) {
            (Some(u), Some(d)) => (u, d),
            _ => {
                other += 1;
                continue;
            }
        };
        let mode = if is_batch { "Batch" } else { "Standard" };
        let price_per_1m = it.retail_price / per_unit * 1_000_000.0;
        let key = (model, deployment, mode.to_string(), it.product_name.clone());
        let entry = grouped.entry(key).or_default();
        match direction.as_str() {
            "input" => entry.input = Some(price_per_1m),
            "cached" => entry.cached = Some(price_per_1m),
            "output" => entry.output = Some(price_per_1m),
            _ => {}
        }
    }

    let mut rows: Vec<Row> = grouped
        .into_iter()
        .map(|((model, deployment, mode, product), p)| Row {
            developer: developer(&model, &product).to_string(),
            model,
            deployment,
            mode,
            input: p.input,
            cached: p.cached,
            output: p.output,
            product,
            arena: None,
            caps: None,
        })
        .collect();
    rows.sort_by(|a, b| {
        (&a.model, &a.deployment, &a.mode).cmp(&(&b.model, &b.deployment, &b.mode))
    });
    (rows, other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units() {
        assert_eq!(tokens_per_unit("1M"), Some(1_000_000.0));
        assert_eq!(tokens_per_unit("1K"), Some(1_000.0));
        assert_eq!(tokens_per_unit("1M Tokens"), None);
        assert_eq!(tokens_per_unit("1 Hour"), None);
        assert_eq!(tokens_per_unit("100"), Some(100.0));
        assert_eq!(tokens_per_unit("10K"), Some(10_000.0));
    }

    #[test]
    fn global_input() {
        let (model, dir, dep, batch) = parse_meter("gpt-4o inp Glbl 1M Tokens", "gpt-4o");
        assert_eq!(model, "gpt-4o");
        assert_eq!(dir.as_deref(), Some("input"));
        assert_eq!(dep, "Global");
        assert!(!batch);
    }

    #[test]
    fn cached_beats_input() {
        let (_, dir, _, _) = parse_meter("cached inp Dz 1M Tokens", "");
        assert_eq!(dir.as_deref(), Some("cached"));
    }

    #[test]
    fn data_zone_detection() {
        let (_, _, dep, _) = parse_meter("5.4 opt Dz 1M Tokens", "5.4 opt Dz");
        assert_eq!(dep, "Data Zone");
    }

    #[test]
    fn regional_detection() {
        let (_, _, dep, _) = parse_meter("gpt out Regnl 1M Tokens", "");
        assert_eq!(dep, "Regional");
    }

    #[test]
    fn default_standard() {
        let (_, _, dep, _) = parse_meter("gpt-4o inp 1M Tokens", "");
        assert_eq!(dep, "Standard");
    }

    #[test]
    fn batch_detection() {
        let (_, _, _, batch) = parse_meter("gpt-4o Batch inp 1M Tokens", "Batch");
        assert!(batch);
    }

    fn item(meter: &str, sku: &str, unit: &str, price: f64, ty: &str) -> Item {
        Item {
            retail_price: price,
            meter_name: meter.into(),
            product_name: "Azure OpenAI".into(),
            sku_name: sku.into(),
            unit_of_measure: unit.into(),
            meter_type: ty.into(),
        }
    }

    #[test]
    fn developers() {
        assert_eq!(developer("gpt-4.1", "Azure OpenAI"), "OpenAI");
        assert_eq!(developer("v4-pro", "Azure Deepseek Models"), "DeepSeek");
        assert_eq!(developer("fw-glm-5", "Azure Fireworks Models"), "Zhipu AI");
        assert_eq!(
            developer("fw-deepseek-v4-pro", "Azure Fireworks Models"),
            "DeepSeek"
        );
        assert_eq!(developer("large-3", "Azure Mistral Models"), "Mistral AI");
        assert_eq!(developer("llama-3.3-70b", "Azure Llama Models"), "Meta");
        assert_eq!(developer("4.6", "Azure Grok Models"), "xAI");
        assert_eq!(developer("code-1.1-flash", "MAI Models"), "Microsoft");
    }

    #[test]
    fn groups_input_cached_output() {
        let items = vec![
            item(
                "gpt-4o inp Glbl 1M Tokens",
                "gpt-4o Glbl",
                "1M",
                2.5,
                "Consumption",
            ),
            item(
                "gpt-4o cached Glbl 1M Tokens",
                "gpt-4o Glbl",
                "1M",
                1.25,
                "Consumption",
            ),
            item(
                "gpt-4o outp Glbl 1M Tokens",
                "gpt-4o Glbl",
                "1M",
                10.0,
                "Consumption",
            ),
            item("gpt-4o image", "gpt-4o", "1 Hour", 5.0, "Consumption"),
            item(
                "gpt-4o inp Glbl 1M Tokens",
                "gpt-4o Glbl",
                "1M",
                99.0,
                "Reservation",
            ),
        ];
        let (rows, other) = build_rows(&items, None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].input, Some(2.5));
        assert_eq!(rows[0].cached, Some(1.25));
        assert_eq!(rows[0].output, Some(10.0));
        assert_eq!(other, 1);
    }
}
