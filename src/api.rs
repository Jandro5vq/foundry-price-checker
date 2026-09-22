use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};
use serde::Deserialize;

use crate::model::Item;

const API_URL: &str = "https://prices.azure.com/api/retail/prices";

#[derive(Debug, Deserialize)]
struct ApiResponse {
    #[serde(rename = "Items", default)]
    items: Vec<Item>,
    #[serde(rename = "NextPageLink")]
    next_page_link: Option<String>,
}

pub enum FetchMsg {
    Progress { page: usize, count: usize },
    Done(Vec<Item>),
    Error(String),
    Arena(Vec<crate::arena::Entry>),
    ArenaError(String),
    Caps(Vec<crate::openrouter::Entry>),
    CapsError(String),
}

pub fn spawn_fetch(region: String, currency: String, services: Vec<String>, tx: Sender<FetchMsg>) {
    thread::spawn(move || {
        let result = fetch_items(&region, &currency, &services, &|page, count| {
            let _ = tx.send(FetchMsg::Progress { page, count });
        });
        match result {
            Ok(items) => {
                let _ = tx.send(FetchMsg::Done(items));
            }
            Err(e) => {
                let _ = tx.send(FetchMsg::Error(format!("{e:#}")));
            }
        }
    });
}

pub fn fetch_items(
    region: &str,
    currency: &str,
    services: &[String],
    on_progress: &dyn Fn(usize, usize),
) -> Result<Vec<Item>> {
    if services.is_empty() {
        bail!("no services selected");
    }
    let service_filter = services
        .iter()
        .map(|s| format!("serviceName eq '{s}'"))
        .collect::<Vec<_>>()
        .join(" or ");
    let filter = format!("armRegionName eq '{region}' and ({service_filter})");

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let mut items = Vec::new();
    let mut page = 0usize;
    let mut next = Some(API_URL.to_string());
    let mut first = true;

    while let Some(url) = next {
        page += 1;
        let req = if first {
            first = false;
            client.get(&url).query(&[
                ("currencyCode", format!("'{currency}'")),
                ("$filter", filter.clone()),
            ])
        } else {
            client.get(&url)
        };
        let resp = req.send()?;
        if !resp.status().is_success() {
            bail!("HTTP {} from Azure Retail Prices API", resp.status());
        }
        let data: ApiResponse = resp.json()?;
        items.extend(data.items);
        on_progress(page, items.len());
        next = data.next_page_link;
    }
    Ok(items)
}
