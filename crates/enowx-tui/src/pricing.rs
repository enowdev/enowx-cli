//! Cost calculation and currency formatting for the sidebar readout.
//!
//! Prices in `ModelConfig` are USD per 1M tokens. This module converts a
//! running token count into a display string, applying the user's chosen
//! currency and rate (from `UiConfig`). No live FX lookup — the rate is a
//! manual multiplier so an offline session still computes something stable.

use enowx_core::Config;

/// Compute total USD spent so far on this session's turns.
pub fn cost_usd(config: &Config, tokens_in: u32, tokens_out: u32, cache_read: u32) -> f64 {
    let m = &config.model;
    let per_million = 1_000_000.0;
    tokens_in as f64 * m.price_input / per_million
        + tokens_out as f64 * m.price_output / per_million
        + cache_read as f64 * m.price_cache_read / per_million
}

/// Format a cost value according to the user's currency setting. Zero always
/// renders as `<sym>0.00` so the reader sees a stable width regardless of
/// whether pricing metadata is available yet.
pub fn format_cost(config: &Config, cost_usd: f64) -> String {
    let ui = &config.ui;
    let rate = if ui.currency_rate > 0.0 {
        ui.currency_rate
    } else {
        1.0
    };
    let converted = cost_usd * rate;
    format_currency(&ui.currency, converted)
}

fn format_currency(code: &str, value: f64) -> String {
    match code.to_ascii_uppercase().as_str() {
        "USD" => format!("${value:.4}"),
        "EUR" => format!("€{value:.4}"),
        "GBP" => format!("£{value:.4}"),
        // Zero-decimal currencies use whole units with thousands separators.
        "IDR" | "JPY" | "KRW" | "VND" => {
            let sym = match code.to_ascii_uppercase().as_str() {
                "IDR" => "Rp",
                "JPY" => "¥",
                "KRW" => "₩",
                "VND" => "₫",
                _ => "",
            };
            format!("{sym} {}", thousands(value.round() as i64))
        }
        "CNY" => format!("¥{value:.2}"),
        // Fallback: show the ISO code and 2 decimals.
        other => format!("{other} {value:.4}"),
    }
}

fn thousands(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    let mut rev: String = out.chars().rev().collect();
    if n < 0 {
        rev.insert(0, '-');
    }
    rev
}

#[cfg(test)]
mod tests {
    use super::*;
    use enowx_core::Config;

    #[test]
    fn usd_default_shows_four_decimals() {
        let mut c = Config::default();
        c.model.price_input = 3.0;
        c.model.price_output = 15.0;
        let cost = cost_usd(&c, 1_000_000, 100_000, 0);
        assert!((cost - (3.0 + 1.5)).abs() < 1e-6);
        assert_eq!(format_cost(&c, cost), "$4.5000");
    }

    #[test]
    fn idr_uses_thousands_and_symbol() {
        let mut c = Config::default();
        c.ui.currency = "IDR".into();
        c.ui.currency_rate = 15_800.0;
        assert_eq!(format_cost(&c, 1.0), "Rp 15.800");
    }

    #[test]
    fn zero_still_renders() {
        let c = Config::default();
        assert_eq!(format_cost(&c, 0.0), "$0.0000");
    }
}
