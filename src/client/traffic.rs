use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde_json::{json, Value};
use url::Url;

use crate::error::{Result, SxmcError};

pub fn inspect_har(
    source: &Path,
    endpoint: Option<&str>,
    search: Option<&str>,
    compact: bool,
) -> Result<Value> {
    let contents = fs::read_to_string(source).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to read HAR file '{}': {}",
            source.display(),
            e
        ))
    })?;
    let har: Value = serde_json::from_str(&contents).map_err(|e| {
        SxmcError::Other(format!(
            "HAR file '{}' is not valid JSON: {}",
            source.display(),
            e
        ))
    })?;

    let entries = har
        .get("log")
        .and_then(|log| log.get("entries"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            SxmcError::Other(format!(
                "HAR file '{}' is missing log.entries.",
                source.display()
            ))
        })?;

    let mut grouped = BTreeMap::<String, TrafficEndpointAccumulator>::new();
    let mut request_count = 0u64;

    for entry in entries {
        let request = entry.get("request").and_then(Value::as_object);
        let response = entry.get("response").and_then(Value::as_object);
        let Some(request) = request else {
            continue;
        };
        let url = request
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let parsed_url = Url::parse(url).ok();
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("GET")
            .to_string();
        let host = parsed_url
            .as_ref()
            .and_then(Url::host_str)
            .unwrap_or("<unknown>")
            .to_string();
        let path = parsed_url
            .as_ref()
            .map(|value| value.path().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "/".to_string());
        let key = format!("{method} {host} {path}");

        let status = response
            .and_then(|resp| resp.get("status"))
            .and_then(Value::as_u64);
        let content_type = response
            .and_then(|resp| resp.get("content"))
            .and_then(|content| content.get("mimeType"))
            .and_then(Value::as_str)
            .map(str::to_string);

        let accumulator =
            grouped
                .entry(key.clone())
                .or_insert_with(|| TrafficEndpointAccumulator {
                    key: key.clone(),
                    method: method.clone(),
                    host: host.clone(),
                    path: path.clone(),
                    sample_url: url.to_string(),
                    count: 0,
                    status_codes: BTreeSet::new(),
                    content_types: BTreeSet::new(),
                });
        accumulator.count += 1;
        if let Some(status) = status {
            accumulator.status_codes.insert(status);
        }
        if let Some(content_type) = content_type {
            accumulator.content_types.insert(content_type);
        }
        request_count += 1;
    }

    let search = search.map(|value| value.to_ascii_lowercase());
    let endpoint = endpoint.map(str::to_string);
    let mut endpoints = grouped
        .into_values()
        .filter(|entry| {
            let matches_endpoint = endpoint.as_ref().is_none_or(|needle| {
                entry.key == *needle || entry.path == *needle || entry.host == *needle
            });
            let matches_search = search.as_ref().is_none_or(|needle| {
                entry.key.to_ascii_lowercase().contains(needle)
                    || entry.sample_url.to_ascii_lowercase().contains(needle)
                    || entry
                        .content_types
                        .iter()
                        .any(|content_type| content_type.to_ascii_lowercase().contains(needle))
            });
            matches_endpoint && matches_search
        })
        .collect::<Vec<_>>();

    endpoints.sort_by(|a, b| a.key.cmp(&b.key));

    let full_endpoints = endpoints
        .iter()
        .map(TrafficEndpointAccumulator::to_value)
        .collect::<Vec<_>>();

    let value = json!({
        "discovery_schema": "sxmc_discover_traffic_v1",
        "source_type": "traffic",
        "source": source.display().to_string(),
        "request_count": request_count,
        "endpoint_count": full_endpoints.len(),
        "endpoints": full_endpoints,
    });

    if compact {
        Ok(json!({
            "discovery_schema": value["discovery_schema"],
            "source_type": value["source_type"],
            "source": value["source"],
            "request_count": value["request_count"],
            "endpoint_count": value["endpoint_count"],
            "endpoint_keys": value["endpoints"].as_array().map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.get("key").and_then(Value::as_str))
                    .map(|key| Value::String(key.to_string()))
                    .collect::<Vec<_>>()
            }).unwrap_or_default(),
        }))
    } else {
        Ok(value)
    }
}

struct TrafficEndpointAccumulator {
    key: String,
    method: String,
    host: String,
    path: String,
    sample_url: String,
    count: u64,
    status_codes: BTreeSet<u64>,
    content_types: BTreeSet<String>,
}

impl TrafficEndpointAccumulator {
    fn to_value(&self) -> Value {
        json!({
            "key": self.key,
            "method": self.method,
            "host": self.host,
            "path": self.path,
            "count": self.count,
            "status_codes": self.status_codes.iter().copied().collect::<Vec<_>>(),
            "content_types": self.content_types.iter().cloned().collect::<Vec<_>>(),
            "sample_url": self.sample_url,
        })
    }
}
