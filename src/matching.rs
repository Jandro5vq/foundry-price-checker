//! Shared conservative name matching between Azure model rows and external
//! catalogues (LMArena, OpenRouter). Azure meter parsing strips family prefixes
//! and adds product-specific suffixes, so we rebuild candidate names from the
//! product and score them against catalogue names.

/// Reasoning-effort variants are the same model.
const EFFORT: &[&str] = &["high", "xhigh", "max", "low", "medium", "minimal"];
/// Suffixes that do not change the underlying model identity.
const VARIANT_SUFFIX: &[&str] = &["sp", "ch", "in", "inp", "ft", "dev"];

pub fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            cur.push(c.to_ascii_lowercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Candidate names for an Azure (model, product) pair.
pub fn candidates(model: &str, product: &str) -> Vec<String> {
    let base = model.strip_prefix("fw-").unwrap_or(model).to_string();
    let mut out = vec![base.clone()];
    let p = product.to_lowercase();
    for (key, prefix) in [
        ("deepseek", "deepseek"),
        ("llama", "llama"),
        ("grok", "grok"),
        ("kimi", "kimi"),
        ("mistral", "mistral"),
        ("qwen", "qwen"),
        ("phi", "phi"),
        ("cohere", "command"),
        ("glm", "glm"),
        ("nvidia", "nvidia"),
        ("minimax", "minimax"),
        ("mai", "mai"),
    ] {
        if p.contains(key) {
            if base.starts_with(prefix) {
                out.push(base.clone());
            } else {
                out.push(format!("{prefix}-{base}"));
            }
        }
    }
    if p.contains("openai") || p.contains("gpt") {
        if base.starts_with("gpt") {
            out.push(base.clone());
        } else {
            out.push(format!("gpt-{base}"));
        }
    }
    if base.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.push(format!("gpt-{base}"));
    }
    out.sort();
    out.dedup();
    out
}

/// Score a candidate token list against a catalogue token list. `None` = no acceptable match.
fn score_match(ct: &[String], bt: &[String]) -> Option<i32> {
    if ct == bt {
        return Some(1000);
    }
    if bt.len() > ct.len() && bt[..ct.len()] == *ct {
        let extra = &bt[ct.len()..];
        let first = &extra[0];
        // Allow reasoning-effort variants and date suffixes (year, or month + year),
        // but not a bare version bump like `grok-4` -> `grok-4.20`.
        let ok = EFFORT.contains(&first.as_str())
            || is_year_like(first)
            || (is_two_digit(first) && extra.get(1).is_some_and(|t| is_year_like(t)));
        return ok.then(|| 900 - bt.len() as i32);
    }
    if ct.len() > bt.len() && ct[..bt.len()] == *bt {
        let extra = &ct[bt.len()..];
        let ok = extra
            .iter()
            .all(|t| VARIANT_SUFFIX.contains(&t.as_str()) || is_date(t));
        return ok.then(|| 800 - ct.len() as i32);
    }
    None
}

fn is_date(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) && (t.len() >= 4 || t.len() == 2)
}

fn is_two_digit(t: &str) -> bool {
    t.len() == 2 && t.chars().all(|c| c.is_ascii_digit())
}

fn is_year_like(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) && (t.len() == 4 || t.len() == 8)
}

/// Find the best matching index into `names` for an Azure (model, product) pair.
///
/// Falls back to stripping trailing date tokens (e.g. `gpt-4o-0806` -> `gpt-4o`)
/// when no direct match is found, with slightly reduced confidence.
pub fn best_index(names: &[String], model: &str, product: &str) -> Option<(usize, i32)> {
    let mut best: Option<(i32, usize)> = None;
    for cand in candidates(model, product) {
        let ct = tokens(&cand);
        if ct.is_empty() {
            continue;
        }
        for (i, name) in names.iter().enumerate() {
            if let Some(s) = score_match(&ct, &tokens(name)) {
                if best.as_ref().is_none_or(|(bs, _)| s > *bs) {
                    best = Some((s, i));
                }
            }
        }
    }
    if best.is_none() {
        for cand in candidates(model, product) {
            let mut ct = tokens(&cand);
            while ct.len() > 1 && is_year_like(ct.last().unwrap()) {
                ct.pop();
            }
            if ct.is_empty() {
                continue;
            }
            for (i, name) in names.iter().enumerate() {
                if let Some(s) = score_match(&ct, &tokens(name)) {
                    let s = s - 1;
                    if best.as_ref().is_none_or(|(bs, _)| s > *bs) {
                        best = Some((s, i));
                    }
                }
            }
        }
    }
    best.map(|(s, i)| (i, s))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn exact_match() {
        let n = names(&["gpt-4.1-2025-04-14", "deepseek-v4-pro"]);
        assert_eq!(best_index(&n, "gpt-4.1", "Azure OpenAI").unwrap().0, 0);
        assert_eq!(
            best_index(&n, "v4-pro", "Azure Deepseek Models").unwrap().0,
            1
        );
    }

    #[test]
    fn year_fallback() {
        let n = names(&["gpt-4o", "gpt-4o-mini"]);
        assert_eq!(best_index(&n, "gpt-4o-0806", "Azure OpenAI").unwrap().0, 0);
        assert_eq!(best_index(&n, "4o-mini-0718", "Azure OpenAI").unwrap().0, 1);
    }

    #[test]
    fn rejects_version_bump() {
        let n = names(&["grok-4.5"]);
        assert!(best_index(&n, "grok-4", "Azure Grok Models").is_none());
        let n = names(&["grok-4.20"]);
        assert!(best_index(&n, "grok-4", "Azure Grok Models").is_none());
    }

    #[test]
    fn accepts_month_year_suffix() {
        let n = names(&["command-a-03-2025"]);
        assert_eq!(best_index(&n, "command-a", "Cohere Models").unwrap().0, 0);
    }

    #[test]
    fn rejects_distinct_variant() {
        let n = names(&["gpt-5.1", "o1-preview"]);
        assert!(best_index(&n, "5.1-codex", "Azure OpenAI GPT5").is_none());
        assert!(best_index(&n, "o1", "Azure OpenAI").is_none());
    }

    #[test]
    fn accepts_effort_variant() {
        let n = names(&["gpt-5.4-mini-high"]);
        assert_eq!(
            best_index(&n, "5.4-mini", "Azure OpenAI GPT5").unwrap().0,
            0
        );
    }
}
