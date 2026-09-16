use super::ModelInfo;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn parse_models(value: &Value) -> Result<Vec<ModelInfo>> {
    let data = value
        .as_array()
        .or_else(|| value["data"].as_array())
        .or_else(|| value["models"].as_array())
        .context("Expected a JSON model array, or an object with data/models array")?;
    let mut models = BTreeMap::new();
    for model in data {
        let Some(id) = model["id"]
            .as_str()
            .or_else(|| model["name"].as_str())
            .filter(|id| !id.trim().is_empty())
        else {
            continue;
        };
        let number = |paths: &[&str]| {
            paths.iter().find_map(|path| {
                let value = model.pointer(path)?;
                value
                    .as_u64()
                    .or_else(|| value.as_str()?.parse().ok())
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|n| *n > 0)
            })
        };
        models.entry(id.to_owned()).or_insert_with(|| ModelInfo {
            id: id.to_owned(),
            name: model["displayName"]
                .as_str()
                .or_else(|| model["name"].as_str())
                .map(str::to_owned),
            context_window: number(&[
                "/context_window",
                "/context_length",
                "/inputTokenLimit",
                "/top_provider/context_length",
                "/limits/context",
            ]),
            max_output_tokens: number(&[
                "/max_output_tokens",
                "/max_completion_tokens",
                "/outputTokenLimit",
                "/top_provider/max_completion_tokens",
                "/limits/output",
            ]),
        });
    }
    anyhow::ensure!(!models.is_empty(), "Provider returned no model IDs");
    Ok(models.into_values().collect())
}
