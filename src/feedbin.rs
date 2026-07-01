//! Feedbin API client.
//!
//! A minimal, blocking client for the Feedbin v2 API. Feedbin authenticates
//! every request with HTTP Basic auth (the raw email + password) over HTTPS —
//! there are no API tokens — which is why the password is kept in the OS
//! keychain (see `config`). Async/tokio is deferred until the TUI lands
//! (TASK-6), so the proof-of-concept uses the blocking `reqwest` client.
//! API shape: `docs/tui_research.md` §4.1.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use reqwest::Method;
use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;

use crate::config::Credentials;

/// Base URL for the Feedbin v2 API. Every path is relative to this and ends in
/// `.json`.
const DEFAULT_BASE_URL: &str = "https://api.feedbin.com/v2";

/// Feedbin caps `entries.json?ids=` at 100 IDs per request.
const MAX_IDS_PER_REQUEST: usize = 100;

/// Feedbin caps the `unread_entries` write endpoints at 1,000 IDs per request.
const MAX_UNREAD_IDS_PER_REQUEST: usize = 1000;

const USER_AGENT: &str = concat!("roses/", env!("CARGO_PKG_VERSION"));

/// A hydrated Feedbin entry. Feedbin sends `title`, `url`, `author`,
/// `published`, `summary`, and `content` as nullable, so they are `Option` to
/// avoid panicking on real-world data (AC #5). `content` is HTML; the reader
/// pane renders it to text. Remaining fields beyond these are ignored by serde.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // `id` feeds the read/unread sync in TASK-7
pub struct Entry {
    pub id: i64,
    pub feed_id: i64,
    pub title: Option<String>,
    pub url: Option<String>,
    /// Display name of the author, when Feedbin provides one; shown in the
    /// reader header in place of the (redundant) feed name (TASK-18).
    pub author: Option<String>,
    pub published: Option<String>,
    /// Short plain-ish summary; falls back here when `content` is absent.
    pub summary: Option<String>,
    /// Full entry body as HTML (rendered to text by the reader pane).
    pub content: Option<String>,
}

/// One Feedbin subscription. Used only to map a `feed_id` to its display title;
/// the other fields (id, feed_url, site_url, created_at) are ignored.
#[derive(Debug, Clone, Deserialize)]
struct Subscription {
    feed_id: i64,
    title: Option<String>,
}

/// A blocking Feedbin v2 client bound to one set of credentials. Cloning is
/// cheap (the inner `reqwest` client is reference-counted) and lets the TUI
/// move a copy into a background `spawn_blocking` fetch.
#[derive(Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    base_url: String,
    email: String,
    password: String,
}

impl Client {
    /// Build a client for the live Feedbin API using the stored credentials.
    pub fn new(credentials: &Credentials) -> Result<Self> {
        Self::with_base_url(credentials, DEFAULT_BASE_URL)
    }

    /// Build a client pointed at an arbitrary base URL. Used by tests to target
    /// a local mock server instead of the live API.
    fn with_base_url(credentials: &Credentials, base_url: &str) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .context("building the HTTP client")?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            email: credentials.email.clone(),
            password: credentials.password.clone(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path)
    }

    /// Begin a GET request to `path` with HTTP Basic auth attached (AC #1).
    fn get(&self, path: &str) -> reqwest::blocking::RequestBuilder {
        self.http
            .get(self.url(path))
            .basic_auth(&self.email, Some(&self.password))
    }

    /// Validate the stored credentials: `Ok(())` on HTTP 200, a clear error on
    /// 401 (bad credentials) or any other failure (AC #2).
    pub fn authenticate(&self) -> Result<()> {
        let resp = self
            .get("authentication.json")
            .send()
            .context("sending the authentication request to Feedbin")?;
        check_status(resp).map(|_| ())
    }

    /// Fetch the IDs of all unread entries — Feedbin's source of truth for
    /// read/unread state (AC #3).
    pub fn unread_entry_ids(&self) -> Result<Vec<i64>> {
        let resp = self
            .get("unread_entries.json")
            .send()
            .context("requesting unread entry IDs from Feedbin")?;
        check_status(resp)?
            .json::<Vec<i64>>()
            .context("parsing the unread entry ID list")
    }

    /// Hydrate entries by ID into typed structs, batching at the 100-ID limit
    /// (AC #4). An empty `ids` slice makes no request.
    pub fn entries(&self, ids: &[i64]) -> Result<Vec<Entry>> {
        let mut entries = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(MAX_IDS_PER_REQUEST) {
            let csv = chunk
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let resp = self
                .get("entries.json")
                .query(&[("ids", csv.as_str())])
                .send()
                .context("requesting entries from Feedbin")?;
            let batch = check_status(resp)?
                .json::<Vec<Entry>>()
                .context("parsing the entries response")?;
            entries.extend(batch);
        }
        Ok(entries)
    }

    /// Map each subscribed `feed_id` to its display title, so entries can be
    /// shown with the feed they came from. Feeds with no title are omitted and
    /// fall back to a placeholder at render time.
    pub fn feed_titles(&self) -> Result<HashMap<i64, String>> {
        let resp = self
            .get("subscriptions.json")
            .send()
            .context("requesting subscriptions from Feedbin")?;
        let subscriptions = check_status(resp)?
            .json::<Vec<Subscription>>()
            .context("parsing the subscriptions response")?;
        Ok(subscriptions
            .into_iter()
            .filter_map(|s| s.title.map(|title| (s.feed_id, title)))
            .collect())
    }

    /// Mark entries read by removing them from Feedbin's unread set
    /// (`DELETE /unread_entries.json`). Returns the IDs the server reports as
    /// actually changed. Batched at the 1,000-ID limit (AC #3).
    pub fn mark_read(&self, ids: &[i64]) -> Result<Vec<i64>> {
        self.write_unread(Method::DELETE, ids)
    }

    /// Restore entries to unread (`POST /unread_entries.json`) — the undo for
    /// [`Client::mark_read`]. Returns the IDs the server reports as changed.
    pub fn mark_unread(&self, ids: &[i64]) -> Result<Vec<i64>> {
        self.write_unread(Method::POST, ids)
    }

    /// Shared body for the unread-state writes: send `{"unread_entries": [...]}`
    /// in <=1,000-ID batches and collect the changed IDs the server echoes back.
    /// An empty `ids` slice makes no request.
    fn write_unread(&self, method: Method, ids: &[i64]) -> Result<Vec<i64>> {
        let mut changed = Vec::new();
        for chunk in ids.chunks(MAX_UNREAD_IDS_PER_REQUEST) {
            let csv = chunk
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let body = format!("{{\"unread_entries\":[{csv}]}}");
            let resp = self
                .http
                .request(method.clone(), self.url("unread_entries.json"))
                .basic_auth(&self.email, Some(&self.password))
                .header(CONTENT_TYPE, "application/json; charset=utf-8")
                .body(body)
                .send()
                .context("sending an unread-state write to Feedbin")?;
            let batch = check_status(resp)?
                .json::<Vec<i64>>()
                .context("parsing the unread-state write response")?;
            changed.extend(batch);
        }
        Ok(changed)
    }
}

/// Pass successful responses through; turn any non-2xx status into an
/// actionable error so external input never panics the client (AC #5).
fn check_status(resp: reqwest::blocking::Response) -> Result<reqwest::blocking::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(anyhow!(
            "Feedbin rejected the stored credentials (HTTP 401). Run `roses logout`, then log in again."
        ));
    }
    let body = resp.text().unwrap_or_default();
    let snippet: String = body.trim().chars().take(200).collect();
    if snippet.is_empty() {
        Err(anyhow!("Feedbin request failed (HTTP {status})"))
    } else {
        Err(anyhow!("Feedbin request failed (HTTP {status}): {snippet}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client(server: &mockito::Server) -> Client {
        let credentials = Credentials {
            email: "reader@example.com".to_string(),
            password: "swordfish".to_string(),
        };
        Client::with_base_url(&credentials, &format!("{}/v2", server.url())).unwrap()
    }

    #[test]
    fn authenticate_sends_basic_auth_and_succeeds_on_200() {
        let mut server = mockito::Server::new();
        // The mock only matches when an HTTP Basic Authorization header is
        // present, so a passing test proves AC #1 (Basic auth over the wire).
        let m = server
            .mock("GET", "/v2/authentication.json")
            .match_header(
                "authorization",
                mockito::Matcher::Regex("^Basic ".to_string()),
            )
            .with_status(200)
            .create();
        let client = test_client(&server);
        assert!(client.authenticate().is_ok());
        m.assert();
    }

    #[test]
    fn authenticate_errors_clearly_on_401() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/v2/authentication.json")
            .with_status(401)
            .create();
        let client = test_client(&server);
        let err = client.authenticate().unwrap_err().to_string();
        assert!(
            err.contains("401"),
            "error should name the 401 status: {err}"
        );
    }

    #[test]
    fn unread_entry_ids_parses_the_array() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/v2/unread_entries.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("[1013, 1014, 2000000]")
            .create();
        let client = test_client(&server);
        assert_eq!(
            client.unread_entry_ids().unwrap(),
            vec![1013, 1014, 2_000_000]
        );
    }

    #[test]
    fn entries_hydrates_and_tolerates_null_fields() {
        let mut server = mockito::Server::new();
        let body = r#"[
            {"id": 1, "feed_id": 7, "title": "Hello", "url": "https://example.com/a", "author": "Ada Lovelace", "published": "2026-06-29T00:00:00.000000Z", "content": "<p>ignored</p>"},
            {"id": 2, "feed_id": 7, "title": null, "url": null, "author": null, "published": null}
        ]"#;
        server
            .mock("GET", "/v2/entries.json")
            .match_query(mockito::Matcher::UrlEncoded("ids".into(), "1,2".into()))
            .with_status(200)
            .with_body(body)
            .create();
        let client = test_client(&server);
        let entries = client.entries(&[1, 2]).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].title.as_deref(), Some("Hello"));
        assert_eq!(entries[0].feed_id, 7);
        assert_eq!(entries[0].url.as_deref(), Some("https://example.com/a"));
        assert_eq!(entries[0].author.as_deref(), Some("Ada Lovelace"));
        assert_eq!(entries[1].title, None);
        assert_eq!(entries[1].url, None);
        assert_eq!(entries[1].author, None);
        assert_eq!(entries[1].published, None);
    }

    #[test]
    fn entries_batches_requests_over_the_100_id_limit() {
        let mut server = mockito::Server::new();
        // 150 IDs must split into two requests (100 + 50).
        let m = server
            .mock("GET", "/v2/entries.json")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_body("[]")
            .expect(2)
            .create();
        let client = test_client(&server);
        let ids: Vec<i64> = (1..=150).collect();
        assert!(client.entries(&ids).unwrap().is_empty());
        m.assert();
    }

    #[test]
    fn entries_with_no_ids_makes_no_request() {
        let mut server = mockito::Server::new();
        let m = server.mock("GET", "/v2/entries.json").expect(0).create();
        let client = test_client(&server);
        assert!(client.entries(&[]).unwrap().is_empty());
        m.assert();
    }

    #[test]
    fn feed_titles_maps_feed_id_to_title() {
        let mut server = mockito::Server::new();
        let body = r#"[
            {"id": 1, "feed_id": 7, "title": "Rust Blog", "feed_url": "https://blog.rust-lang.org/feed.xml", "site_url": "https://blog.rust-lang.org"},
            {"id": 2, "feed_id": 9, "title": null}
        ]"#;
        server
            .mock("GET", "/v2/subscriptions.json")
            .with_status(200)
            .with_body(body)
            .create();
        let client = test_client(&server);
        let titles = client.feed_titles().unwrap();
        assert_eq!(titles.get(&7).map(String::as_str), Some("Rust Blog"));
        // A null-titled feed is omitted (renders as a placeholder later).
        assert!(!titles.contains_key(&9));
    }

    #[test]
    fn mark_read_sends_delete_with_json_body() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("DELETE", "/v2/unread_entries.json")
            .match_header("content-type", "application/json; charset=utf-8")
            .match_body(r#"{"unread_entries":[5,6]}"#)
            .with_status(200)
            .with_body("[5,6]")
            .create();
        let client = test_client(&server);
        assert_eq!(client.mark_read(&[5, 6]).unwrap(), vec![5, 6]);
        m.assert();
    }

    #[test]
    fn mark_unread_sends_post_with_json_body() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("POST", "/v2/unread_entries.json")
            .match_body(r#"{"unread_entries":[42]}"#)
            .with_status(200)
            .with_body("[42]")
            .create();
        let client = test_client(&server);
        assert_eq!(client.mark_unread(&[42]).unwrap(), vec![42]);
        m.assert();
    }

    #[test]
    fn unread_writes_batch_at_the_1000_id_limit() {
        let mut server = mockito::Server::new();
        // 1500 IDs must split into two requests (1000 + 500).
        let m = server
            .mock("DELETE", "/v2/unread_entries.json")
            .match_body(mockito::Matcher::Any)
            .with_status(200)
            .with_body("[]")
            .expect(2)
            .create();
        let client = test_client(&server);
        let ids: Vec<i64> = (1..=1500).collect();
        assert!(client.mark_read(&ids).unwrap().is_empty());
        m.assert();
    }

    #[test]
    fn empty_unread_write_makes_no_request() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("DELETE", "/v2/unread_entries.json")
            .expect(0)
            .create();
        let client = test_client(&server);
        assert!(client.mark_read(&[]).unwrap().is_empty());
        m.assert();
    }
}
