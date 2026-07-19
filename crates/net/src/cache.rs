//! Small bounded response cache owned by the direct/network-process loader.

use std::collections::VecDeque;

use http::{
    Method,
    header::{AUTHORIZATION, CACHE_CONTROL, COOKIE, SET_COOKIE},
};

use crate::{Request, Response};

const DEFAULT_MAX_CACHE_ENTRIES: usize = 64;
const DEFAULT_MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;
const MAX_CACHEABLE_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetworkCacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub stores: u64,
    pub evictions: u64,
    pub entries: usize,
    pub bytes: usize,
}

#[derive(Debug, Default)]
pub(crate) struct ResponseCache {
    entries: VecDeque<CacheEntry>,
    bytes: usize,
    hits: u64,
    misses: u64,
    stores: u64,
    evictions: u64,
}

#[derive(Clone, Debug)]
struct CacheEntry {
    key: String,
    response: Response,
    bytes: usize,
}

impl ResponseCache {
    pub(crate) fn get(&mut self, request: &Request) -> Option<Response> {
        let key = cache_key(request)?;
        let Some(index) = self.entries.iter().position(|entry| entry.key == key) else {
            self.misses = self.misses.saturating_add(1);
            return None;
        };
        let entry = self
            .entries
            .remove(index)
            .expect("located cache entry exists");
        let response = entry.response.clone();
        self.entries.push_front(entry);
        self.hits = self.hits.saturating_add(1);
        Some(response)
    }

    pub(crate) fn store(&mut self, request: &Request, response: &Response) {
        let Some(key) = cache_key(request) else {
            return;
        };
        if !response.status.is_success()
            || response.body.len() > MAX_CACHEABLE_RESPONSE_BYTES
            || response.headers.contains_key(SET_COOKIE)
            || response
                .headers
                .get(CACHE_CONTROL)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| {
                    let value = value.to_ascii_lowercase();
                    value.contains("no-store") || value.contains("private")
                })
        {
            return;
        }
        if let Some(index) = self.entries.iter().position(|entry| entry.key == key) {
            let old = self
                .entries
                .remove(index)
                .expect("located cache entry exists");
            self.bytes = self.bytes.saturating_sub(old.bytes);
        }
        let bytes = response.body.len();
        self.entries.push_front(CacheEntry {
            key,
            response: response.clone(),
            bytes,
        });
        self.bytes = self.bytes.saturating_add(bytes);
        self.stores = self.stores.saturating_add(1);
        while self.entries.len() > DEFAULT_MAX_CACHE_ENTRIES || self.bytes > DEFAULT_MAX_CACHE_BYTES
        {
            let Some(entry) = self.entries.pop_back() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(entry.bytes);
            self.evictions = self.evictions.saturating_add(1);
        }
    }

    pub(crate) fn metrics(&self) -> NetworkCacheMetrics {
        NetworkCacheMetrics {
            hits: self.hits,
            misses: self.misses,
            stores: self.stores,
            evictions: self.evictions,
            entries: self.entries.len(),
            bytes: self.bytes,
        }
    }
}

fn cache_key(request: &Request) -> Option<String> {
    if request.method != Method::GET
        || !request.body.is_empty()
        || request.headers.contains_key(AUTHORIZATION)
        || request.headers.contains_key(COOKIE)
    {
        return None;
    }
    let accept = request
        .headers
        .get(http::header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    Some(format!("{}\n{accept}", request.url.as_str()))
}
