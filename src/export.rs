use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::model::Row;

pub fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Format a price for CSV output (no thousands separators, trailing zeros trimmed).
pub fn fmt_value(v: Option<f64>) -> String {
    match v {
        None => String::new(),
        Some(x) => {
            let s = format!("{x:.4}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        }
    }
}

pub fn export(rows: &[&Row], path: &str) -> Result<usize> {
    let mut wtr = csv::Writer::from_path(path)?;
    wtr.write_record([
        "model",
        "developer",
        "deployment",
        "mode",
        "input_per_1M",
        "cached_input_per_1M",
        "output_per_1M",
        "context_window",
        "capabilities",
        "product",
    ])?;
    for r in rows {
        let flags = r
            .caps
            .as_ref()
            .map(|c| {
                let mut f = String::new();
                for (on, ch) in [
                    (c.vision, 'V'),
                    (c.tools, 'T'),
                    (c.reasoning, 'R'),
                    (c.audio, 'A'),
                    (c.json, 'J'),
                ] {
                    if on {
                        f.push(ch);
                    }
                }
                f
            })
            .unwrap_or_default();
        wtr.write_record([
            r.model.clone(),
            r.developer.clone(),
            r.deployment.clone(),
            r.mode.clone(),
            fmt_value(r.input),
            fmt_value(r.cached),
            fmt_value(r.output),
            r.caps
                .as_ref()
                .map(|c| c.context.to_string())
                .unwrap_or_default(),
            flags,
            r.product.clone(),
        ])?;
    }
    wtr.flush()?;
    Ok(rows.len())
}
