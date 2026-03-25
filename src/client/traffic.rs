use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde_json::{json, Value};
use url::Url;

use crate::error::{Result, SxmcError};

pub fn inspect_traffic_source(
    source: &Path,
    endpoint: Option<&str>,
    search: Option<&str>,
    compact: bool,
) -> Result<Value> {
    let contents = fs::read_to_string(source).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to read traffic source '{}': {}",
            source.display(),
            e
        ))
    })?;

    let (capture_kind, requests) = if let Ok(json_value) = serde_json::from_str::<Value>(&contents)
    {
        if json_value
            .get("log")
            .and_then(|log| log.get("entries"))
            .and_then(Value::as_array)
            .is_some()
        {
            ("har", collect_har_requests(source, &json_value)?)
        } else {
            ("curl", collect_curl_requests(&contents))
        }
    } else {
        ("curl", collect_curl_requests(&contents))
    };

    build_traffic_value(source, capture_kind, requests, endpoint, search, compact)
}

pub fn load_traffic_snapshot(path: &Path) -> Result<Value> {
    let contents = fs::read_to_string(path).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to read traffic snapshot '{}': {}",
            path.display(),
            e
        ))
    })?;
    let value: Value = serde_json::from_str(&contents).map_err(|e| {
        SxmcError::Other(format!(
            "Traffic snapshot '{}' is not valid JSON: {}",
            path.display(),
            e
        ))
    })?;
    if value["discovery_schema"] != "sxmc_discover_traffic_v1" || value["source_type"] != "traffic"
    {
        return Err(SxmcError::Other(format!(
            "Traffic snapshot '{}' is not a valid sxmc traffic discovery artifact.",
            path.display()
        )));
    }
    Ok(value)
}

pub fn diff_traffic_value(before: &Value, after: &Value) -> Value {
    json!({
        "discovery_schema": "sxmc_discover_traffic_diff_v1",
        "source_type": "traffic-diff",
        "before_source": before["source"],
        "after_source": after["source"],
        "before_capture_kind": before["capture_kind"],
        "after_capture_kind": after["capture_kind"],
        "request_count_changed": before["request_count"] != after["request_count"],
        "endpoint_count_changed": before["endpoint_count"] != after["endpoint_count"],
        "endpoints_added": endpoint_key_diff(after["endpoints"].as_array(), before["endpoints"].as_array()),
        "endpoints_removed": endpoint_key_diff(before["endpoints"].as_array(), after["endpoints"].as_array()),
        "status_codes_added": endpoint_status_diff(after["endpoints"].as_array(), before["endpoints"].as_array()),
        "status_codes_removed": endpoint_status_diff(before["endpoints"].as_array(), after["endpoints"].as_array()),
        "content_types_added": endpoint_content_type_diff(after["endpoints"].as_array(), before["endpoints"].as_array()),
        "content_types_removed": endpoint_content_type_diff(before["endpoints"].as_array(), after["endpoints"].as_array()),
    })
}

#[derive(Default)]
struct TrafficRequest {
    method: String,
    url: String,
    status: Option<u64>,
    content_type: Option<String>,
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

fn collect_har_requests(source: &Path, har: &Value) -> Result<Vec<TrafficRequest>> {
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

    let mut requests = Vec::new();
    for entry in entries {
        let request = entry.get("request").and_then(Value::as_object);
        let response = entry.get("response").and_then(Value::as_object);
        let Some(request) = request else {
            continue;
        };
        let url = request
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if url.is_empty() {
            continue;
        }
        requests.push(TrafficRequest {
            method: request
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("GET")
                .to_string(),
            url,
            status: response
                .and_then(|resp| resp.get("status"))
                .and_then(Value::as_u64),
            content_type: response
                .and_then(|resp| resp.get("content"))
                .and_then(|content| content.get("mimeType"))
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    Ok(requests)
}

fn collect_curl_requests(contents: &str) -> Vec<TrafficRequest> {
    let mut requests = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || !trimmed.contains("curl") {
            continue;
        }
        if let Some(request) = parse_curl_line(trimmed) {
            requests.push(request);
        }
    }
    requests
}

fn parse_curl_line(line: &str) -> Option<TrafficRequest> {
    let words = shlex::split(line)?;
    if words.is_empty() {
        return None;
    }
    let first = words.first()?.as_str();
    if first != "curl" && !first.ends_with("/curl") {
        return None;
    }

    let mut method = "GET".to_string();
    let mut url = None::<String>;
    let mut content_type = None::<String>;
    let mut i = 1usize;
    while i < words.len() {
        let word = &words[i];
        match word.as_str() {
            "-X" | "--request" => {
                if let Some(value) = words.get(i + 1) {
                    method = value.to_uppercase();
                    i += 1;
                }
            }
            "-H" | "--header" => {
                if let Some(value) = words.get(i + 1) {
                    if let Some((header, header_value)) = value.split_once(':') {
                        if header.trim().eq_ignore_ascii_case("content-type") {
                            content_type = Some(header_value.trim().to_string());
                        }
                    }
                    i += 1;
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-urlencode" => {
                if method == "GET" {
                    method = "POST".to_string();
                }
                if words.get(i + 1).is_some() {
                    i += 1;
                }
            }
            _ => {
                if url.is_none() && (word.starts_with("http://") || word.starts_with("https://")) {
                    url = Some(word.to_string());
                }
            }
        }
        i += 1;
    }

    url.map(|url| TrafficRequest {
        method,
        url,
        status: None,
        content_type,
    })
}

fn build_traffic_value(
    source: &Path,
    capture_kind: &str,
    requests: Vec<TrafficRequest>,
    endpoint: Option<&str>,
    search: Option<&str>,
    compact: bool,
) -> Result<Value> {
    let mut grouped = BTreeMap::<String, TrafficEndpointAccumulator>::new();
    let request_count = requests.len() as u64;

    for request in requests {
        let parsed_url = Url::parse(&request.url).ok();
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
        let key = format!("{} {} {}", request.method, host, path);

        let accumulator =
            grouped
                .entry(key.clone())
                .or_insert_with(|| TrafficEndpointAccumulator {
                    key: key.clone(),
                    method: request.method.clone(),
                    host: host.clone(),
                    path: path.clone(),
                    sample_url: request.url.clone(),
                    count: 0,
                    status_codes: BTreeSet::new(),
                    content_types: BTreeSet::new(),
                });
        accumulator.count += 1;
        if let Some(status) = request.status {
            accumulator.status_codes.insert(status);
        }
        if let Some(content_type) = request.content_type {
            accumulator.content_types.insert(content_type);
        }
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
        "capture_kind": capture_kind,
        "source": source.display().to_string(),
        "request_count": request_count,
        "endpoint_count": full_endpoints.len(),
        "endpoints": full_endpoints,
    });

    if compact {
        Ok(json!({
            "discovery_schema": value["discovery_schema"],
            "source_type": value["source_type"],
            "capture_kind": value["capture_kind"],
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

fn endpoint_key_diff(left: Option<&Vec<Value>>, right: Option<&Vec<Value>>) -> Value {
    let left = endpoint_key_set(left);
    let right = endpoint_key_set(right);
    Value::Array(
        left.difference(&right)
            .cloned()
            .map(Value::String)
            .collect::<Vec<_>>(),
    )
}

fn endpoint_status_diff(left: Option<&Vec<Value>>, right: Option<&Vec<Value>>) -> Value {
    let left = endpoint_nested_set(left, "status_codes");
    let right = endpoint_nested_set(right, "status_codes");
    Value::Array(
        left.difference(&right)
            .cloned()
            .map(Value::String)
            .collect::<Vec<_>>(),
    )
}

fn endpoint_content_type_diff(left: Option<&Vec<Value>>, right: Option<&Vec<Value>>) -> Value {
    let left = endpoint_nested_set(left, "content_types");
    let right = endpoint_nested_set(right, "content_types");
    Value::Array(
        left.difference(&right)
            .cloned()
            .map(Value::String)
            .collect::<Vec<_>>(),
    )
}

fn endpoint_key_set(values: Option<&Vec<Value>>) -> BTreeSet<String> {
    values
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("key").and_then(Value::as_str))
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default()
}

fn endpoint_nested_set(values: Option<&Vec<Value>>, field: &str) -> BTreeSet<String> {
    values
        .map(|items| {
            let mut entries = BTreeSet::new();
            for item in items {
                let key = item["key"].as_str().unwrap_or("<unknown>");
                if let Some(values) = item[field].as_array() {
                    for nested in values {
                        let rendered = if let Some(string) = nested.as_str() {
                            string.to_string()
                        } else if let Some(number) = nested.as_u64() {
                            number.to_string()
                        } else {
                            continue;
                        };
                        entries.insert(format!("{key}: {rendered}"));
                    }
                }
            }
            entries
        })
        .unwrap_or_default()
}
