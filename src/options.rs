/// Curated list of Azure regions that offer Foundry models. First entry is the default.
pub const REGIONS: &[&str] = &[
    "spaincentral",
    "westeurope",
    "northeurope",
    "swedencentral",
    "germanywestcentral",
    "francecentral",
    "italynorth",
    "uksouth",
    "polandcentral",
    "switzerlandnorth",
    "norwayeast",
    "eastus",
    "eastus2",
    "westus",
    "westus2",
    "westus3",
    "centralus",
    "southcentralus",
    "northcentralus",
    "westcentralus",
    "canadaeast",
    "canadacentral",
    "brazilsouth",
    "japaneast",
    "japanwest",
    "koreacentral",
    "southeastasia",
    "eastasia",
    "centralindia",
    "southindia",
    "australiaeast",
    "uaenorth",
    "southafricanorth",
];

/// Curated list of currencies supported by the Retail Prices API. First entry is the default.
pub const CURRENCIES: &[&str] = &[
    "EUR", "USD", "GBP", "CHF", "SEK", "NOK", "DKK", "CAD", "AUD", "JPY", "INR", "BRL", "KRW",
    "NZD", "SGD", "HKD", "MXN", "ZAR", "AED", "TWD",
];

pub const SERVICES: &[&str] = &["Foundry Models", "Cognitive Services"];

pub const DEFAULT_REGION: &str = REGIONS[0];
pub const DEFAULT_CURRENCY: &str = CURRENCIES[0];

pub fn region_index(region: &str) -> usize {
    REGIONS
        .iter()
        .position(|r| r.eq_ignore_ascii_case(region))
        .unwrap_or(0)
}

pub fn currency_index(currency: &str) -> usize {
    CURRENCIES
        .iter()
        .position(|c| c.eq_ignore_ascii_case(currency))
        .unwrap_or(0)
}
