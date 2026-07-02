//! Feedbin API client.
//!
//! A minimal, blocking client for the Feedbin v2 API. Feedbin authenticates
//! every request with HTTP Basic auth (the raw email + password) over HTTPS —
//! there are no API tokens — which is why the password is kept in the OS
//! keychain (see `config`). Async/tokio is deferred until the TUI lands
//! (TASK-6), so the proof-of-concept uses the blocking `reqwest` client.
//! API shape: `docs/tui_research.md` §4.1.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::Method;
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};

use crate::config::Credentials;

/// Base URL for the Feedbin v2 API. Every path is relative to this and ends in
/// `.json`.
const DEFAULT_BASE_URL: &str = "https://api.feedbin.com/v2";

/// Feedbin caps `entries.json?ids=` at 100 IDs per request.
const MAX_IDS_PER_REQUEST: usize = 100;

/// Feedbin caps the `unread_entries` write endpoints at 1,000 IDs per request.
const MAX_UNREAD_IDS_PER_REQUEST: usize = 1000;

const USER_AGENT: &str = concat!("roses/", env!("CARGO_PKG_VERSION"));

/// Fail fast if a TCP connection to Feedbin can't be established.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Overall per-request ceiling. Without it a hung or half-open connection would
/// block a `spawn_blocking` pool thread indefinitely, and repeated reloads would
/// leak more stuck threads (the image client already caps its fetches at 10s).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// A hydrated Feedbin entry. Feedbin sends `title`, `url`, `author`,
/// `published`, `summary`, and `content` as nullable, so they are `Option` to
/// avoid panicking on real-world data (AC #5). `content` is HTML; the reader
/// pane renders it to text. The `images`/`enclosure`/`json_feed` objects are
/// only present with `mode=extended` (TASK-21/22/23). Remaining fields are
/// ignored by serde.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Feedbin's extracted lead image (`mode=extended`, TASK-21). Boxed so these
    /// optional extended-mode objects don't bloat `Entry` (which travels through
    /// the message channel, the entries Vec, and the undo stack).
    pub images: Option<Box<EntryImages>>,
    /// Podcast/media enclosure (`mode=extended`, TASK-22).
    pub enclosure: Option<Box<Enclosure>>,
    /// JSON Feed extras — notably `external_url` for link blogs
    /// (`mode=extended`, TASK-23).
    pub json_feed: Option<Box<JsonFeed>>,
}

/// The `images` object (extended mode): a curated lead image. Only the CDN URL
/// of the standard size (`size_1`) is used; the raw pixel dimensions Feedbin
/// also provides aren't needed because the half-block renderer decodes the CDN
/// image and sizes the art from its actual pixels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryImages {
    pub size_1: Option<ImageSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSize {
    pub cdn_url: Option<String>,
}

/// The `enclosure` object (extended mode): a podcast/media attachment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enclosure {
    pub enclosure_url: Option<String>,
    pub enclosure_type: Option<String>,
    pub itunes_duration: Option<String>,
}

/// The `json_feed` object (extended mode); `external_url` is the link a
/// link-blog entry points *out* to (distinct from the permalink `url`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFeed {
    pub external_url: Option<String>,
}

impl Entry {
    /// Feedbin's extracted lead-image CDN URL, if any (TASK-21).
    pub fn lead_image_url(&self) -> Option<&str> {
        self.images.as_ref()?.size_1.as_ref()?.cdn_url.as_deref()
    }

    /// The podcast/media enclosure URL, if any (TASK-22).
    pub fn enclosure_url(&self) -> Option<&str> {
        self.enclosure.as_ref()?.enclosure_url.as_deref()
    }

    /// The JSON Feed `external_url` (link-blog target), if any (TASK-23).
    pub fn external_url(&self) -> Option<&str> {
        self.json_feed.as_ref()?.external_url.as_deref()
    }
}

/// One Feedbin subscription. Drives both the `feed_id`→title map the TUI uses
/// and the OPML export (TASK-38), which needs the feed and site URLs. Feedbin
/// sends `title`/`feed_url`/`site_url` nullable, so they're `Option`; the `id`
/// and `created_at` fields are ignored (`Deserialize` drops unknown fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub feed_id: i64,
    pub title: Option<String>,
    /// The feed's URL (OPML `xmlUrl`).
    pub feed_url: Option<String>,
    /// The feed's website URL (OPML `htmlUrl`).
    pub site_url: Option<String>,
}

/// HTTP cache validators for a Feedbin GET response (TASK-42). Replayed as
/// `If-None-Match` / `If-Modified-Since` on the next request so an unchanged
/// endpoint returns `304 Not Modified` instead of the full body.
#[derive(Debug, Clone, Default)]
pub struct Validators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// The outcome of a conditional GET: unchanged (`304`), or fresh data plus the
/// new validators to store for next time.
pub enum Conditional<T> {
    NotModified,
    Modified { data: T, validators: Validators },
}

/// An OPML import job (TASK-38). Feedbin processes uploads asynchronously, so a
/// `create_import` starts one and `import_status` polls it until `complete`.
#[derive(Debug, Clone, Deserialize)]
pub struct Import {
    pub id: i64,
    pub complete: bool,
    /// Per-feed results; absent on the list endpoint, so default to empty.
    #[serde(default)]
    pub import_items: Vec<ImportItem>,
}

/// One feed's result within an [`Import`]. `status` is `"pending"`, `"complete"`,
/// or `"failed"` (kept as a string — Feedbin owns the vocabulary). Feedbin also
/// sends a `title`, but the summary keys off the feed URL, so it's not modelled
/// (`Deserialize` drops unknown fields).
#[derive(Debug, Clone, Deserialize)]
pub struct ImportItem {
    pub feed_url: Option<String>,
    pub status: String,
}

/// A count of an import's per-feed outcomes, plus the URLs that failed — the
/// data `roses import` prints as its summary. Pure over an [`Import`] so the
/// CLI's formatting stays testable without hitting the network.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportTally {
    pub complete: usize,
    pub pending: usize,
    pub failed: usize,
    pub failed_urls: Vec<String>,
}

impl Import {
    /// Tally the import's items by status, collecting the feed URLs that failed.
    pub fn tally(&self) -> ImportTally {
        let mut tally = ImportTally::default();
        for item in &self.import_items {
            match item.status.as_str() {
                "complete" => tally.complete += 1,
                "failed" => {
                    tally.failed += 1;
                    if let Some(url) = &item.feed_url {
                        tally.failed_urls.push(url.clone());
                    }
                }
                // "pending" or any unfamiliar status Feedbin adds later.
                _ => tally.pending += 1,
            }
        }
        tally
    }
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
        Self::with_base_url_and_timeout(credentials, base_url, REQUEST_TIMEOUT)
    }

    /// Like [`Client::with_base_url`] but with an explicit overall request
    /// timeout, so a test can force a fast timeout against a non-responding
    /// socket rather than waiting out the production ceiling.
    fn with_base_url_and_timeout(
        credentials: &Credentials,
        base_url: &str,
        request_timeout: Duration,
    ) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(request_timeout)
            .build()
            .context("building the HTTP client")?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            email: credentials.email.clone(),
            password: credentials.password.clone(),
        })
    }

    /// Build a client pointed at a mock server, for tests in *other* modules
    /// (e.g. the `tui` reconcile orchestration) that need to exercise the client
    /// against `mockito` without the private `with_base_url` seam.
    #[cfg(test)]
    pub(crate) fn for_test(base_url: &str) -> Self {
        let credentials = Credentials {
            email: "reader@example.com".to_string(),
            password: "swordfish".to_string(),
        };
        Self::with_base_url(&credentials, base_url).unwrap()
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
    /// 401 (bad credentials) or any other failure (AC #2). No longer on the
    /// startup path — the offline-first TUI validates lazily via the background
    /// load (TASK-41) — but retained as a client capability (and to keep the
    /// 401-mapping under test) for an explicit "verify login" use.
    #[allow(dead_code)]
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

    /// Like [`Client::unread_entry_ids`] but conditional (TASK-42): replays the
    /// stored `ETag`/`Last-Modified` so an unchanged unread set comes back as a
    /// cheap `304 Not Modified`; otherwise returns the ids plus the fresh
    /// validators to persist. (`roses list` uses the plain variant above.)
    pub fn unread_entry_ids_conditional(
        &self,
        validators: &Validators,
    ) -> Result<Conditional<Vec<i64>>> {
        self.conditional_ids("unread_entries.json", validators)
    }

    /// Like [`Client::unread_entry_ids_conditional`] but for the *updated*-entries
    /// queue (TASK-44): the ids of entries whose content Feedbin refreshed since
    /// we last saw them. Replays the stored `updated.*` validators; the ids are
    /// drained via [`Client::delete_updated_entries`] once re-hydrated.
    pub fn updated_entry_ids_conditional(
        &self,
        validators: &Validators,
    ) -> Result<Conditional<Vec<i64>>> {
        self.conditional_ids("updated_entries.json", validators)
    }

    /// Shared conditional GET of a JSON `[i64]` id list at `endpoint`, replaying
    /// the stored `ETag`/`Last-Modified` so an unchanged list returns a cheap
    /// `304 Not Modified`; otherwise the ids plus the fresh validators to persist
    /// (TASK-42/44).
    fn conditional_ids(
        &self,
        endpoint: &str,
        validators: &Validators,
    ) -> Result<Conditional<Vec<i64>>> {
        use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};

        let mut req = self.get(endpoint);
        if let Some(etag) = &validators.etag {
            req = req.header(IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = &validators.last_modified {
            req = req.header(IF_MODIFIED_SINCE, last_modified);
        }
        let resp = req
            .send()
            .context("requesting an entry ID list from Feedbin")?;
        if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(Conditional::NotModified);
        }
        let resp = check_status(resp)?;
        let validators = Validators {
            etag: header_string(resp.headers(), ETAG),
            last_modified: header_string(resp.headers(), LAST_MODIFIED),
        };
        let data = resp
            .json::<Vec<i64>>()
            .context("parsing the entry ID list")?;
        Ok(Conditional::Modified { data, validators })
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
                // `mode=extended` adds the images/enclosure/json_feed objects
                // used by the reader (TASK-21/22/23).
                .query(&[("ids", csv.as_str()), ("mode", "extended")])
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
        Ok(self
            .subscriptions()?
            .into_iter()
            .filter_map(|s| s.title.map(|title| (s.feed_id, title)))
            .collect())
    }

    /// Fetch the full subscription list (`GET /subscriptions.json`) — feed id,
    /// title, and the feed/site URLs. Drives both `feed_titles()` and the OPML
    /// export (TASK-38).
    pub fn subscriptions(&self) -> Result<Vec<Subscription>> {
        let resp = self
            .get("subscriptions.json")
            .send()
            .context("requesting subscriptions from Feedbin")?;
        check_status(resp)?
            .json::<Vec<Subscription>>()
            .context("parsing the subscriptions response")
    }

    /// Start an OPML import by uploading the raw OPML file
    /// (`POST /imports.json`, `Content-Type: text/xml`). Feedbin parses the OPML
    /// server-side and processes it asynchronously, returning the new import's id
    /// and initial per-feed status (TASK-38).
    pub fn create_import(&self, opml: &[u8]) -> Result<Import> {
        let resp = self
            .http
            .post(self.url("imports.json"))
            .basic_auth(&self.email, Some(&self.password))
            .header(CONTENT_TYPE, "text/xml")
            .body(opml.to_vec())
            .send()
            .context("uploading the OPML import to Feedbin")?;
        check_status(resp)?
            .json::<Import>()
            .context("parsing the import response")
    }

    /// Poll one import's status (`GET /imports/{id}.json`) for its `complete`
    /// flag and per-feed results (TASK-38).
    pub fn import_status(&self, id: i64) -> Result<Import> {
        let resp = self
            .get(&format!("imports/{id}.json"))
            .send()
            .context("requesting import status from Feedbin")?;
        check_status(resp)?
            .json::<Import>()
            .context("parsing the import status response")
    }

    /// Mark entries read by removing them from Feedbin's unread set
    /// (`DELETE /unread_entries.json`). Returns the IDs the server reports as
    /// actually changed. Batched at the 1,000-ID limit (AC #3).
    pub fn mark_read(&self, ids: &[i64]) -> Result<Vec<i64>> {
        self.write_entry_ids(Method::DELETE, "unread_entries.json", "unread_entries", ids)
    }

    /// Restore entries to unread (`POST /unread_entries.json`) — the undo for
    /// [`Client::mark_read`]. Returns the IDs the server reports as changed.
    pub fn mark_unread(&self, ids: &[i64]) -> Result<Vec<i64>> {
        self.write_entry_ids(Method::POST, "unread_entries.json", "unread_entries", ids)
    }

    /// Drain ids from the updated-entries queue (`DELETE /updated_entries.json`)
    /// once their content has been re-hydrated, so Feedbin doesn't return them
    /// again (TASK-44). Batched at the 1,000-id limit; returns the changed ids.
    pub fn delete_updated_entries(&self, ids: &[i64]) -> Result<Vec<i64>> {
        self.write_entry_ids(
            Method::DELETE,
            "updated_entries.json",
            "updated_entries",
            ids,
        )
    }

    /// Shared body for the entry-id writes (unread + updated queues): send
    /// `{"<key>": [...]}` in <=1,000-id batches and collect the changed ids the
    /// server echoes back. An empty `ids` slice makes no request.
    fn write_entry_ids(
        &self,
        method: Method,
        endpoint: &str,
        key: &str,
        ids: &[i64],
    ) -> Result<Vec<i64>> {
        let mut changed = Vec::new();
        for chunk in ids.chunks(MAX_UNREAD_IDS_PER_REQUEST) {
            let csv = chunk
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let body = format!("{{\"{key}\":[{csv}]}}");
            let resp = self
                .http
                .request(method.clone(), self.url(endpoint))
                .basic_auth(&self.email, Some(&self.password))
                .header(CONTENT_TYPE, "application/json; charset=utf-8")
                .body(body)
                .send()
                .context("sending an entry-id write to Feedbin")?;
            let batch = check_status(resp)?
                .json::<Vec<i64>>()
                .context("parsing the entry-id write response")?;
            changed.extend(batch);
        }
        Ok(changed)
    }
}

/// Read a response header as an owned `String`, if present and valid UTF-8.
fn header_string(
    headers: &reqwest::header::HeaderMap,
    name: reqwest::header::HeaderName,
) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
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
    fn conditional_unread_captures_validators_then_304s() {
        let mut server = mockito::Server::new();
        // First request (no validators): 200 with an ETag + Last-Modified.
        let m200 = server
            .mock("GET", "/v2/unread_entries.json")
            .match_header("if-none-match", mockito::Matcher::Missing)
            .with_status(200)
            .with_header("etag", "\"abc123\"")
            .with_header("last-modified", "Sat, 02 Feb 2013 15:20:46 GMT")
            .with_body("[1, 2]")
            .create();
        let client = test_client(&server);
        let validators = match client
            .unread_entry_ids_conditional(&Validators::default())
            .unwrap()
        {
            Conditional::Modified { data, validators } => {
                assert_eq!(data, vec![1, 2]);
                validators
            }
            Conditional::NotModified => panic!("first request should be a 200, not a 304"),
        };
        assert_eq!(validators.etag.as_deref(), Some("\"abc123\""));
        assert_eq!(
            validators.last_modified.as_deref(),
            Some("Sat, 02 Feb 2013 15:20:46 GMT")
        );
        m200.assert();

        // Replaying the ETag yields a 304 with no body — the fast path.
        let m304 = server
            .mock("GET", "/v2/unread_entries.json")
            .match_header("if-none-match", "\"abc123\"")
            .with_status(304)
            .create();
        assert!(matches!(
            client.unread_entry_ids_conditional(&validators).unwrap(),
            Conditional::NotModified
        ));
        m304.assert();
    }

    #[test]
    fn conditional_updated_captures_validators_then_304s() {
        let mut server = mockito::Server::new();
        // The updated-entries queue shares the conditional machinery (TASK-44):
        // a first 200 yields ids + validators; replaying the ETag 304s.
        let m200 = server
            .mock("GET", "/v2/updated_entries.json")
            .match_header("if-none-match", mockito::Matcher::Missing)
            .with_status(200)
            .with_header("etag", "\"upd9\"")
            .with_body("[7, 8]")
            .create();
        let client = test_client(&server);
        let validators = match client
            .updated_entry_ids_conditional(&Validators::default())
            .unwrap()
        {
            Conditional::Modified { data, validators } => {
                assert_eq!(data, vec![7, 8]);
                validators
            }
            Conditional::NotModified => panic!("first request should be a 200, not a 304"),
        };
        assert_eq!(validators.etag.as_deref(), Some("\"upd9\""));
        m200.assert();

        let m304 = server
            .mock("GET", "/v2/updated_entries.json")
            .match_header("if-none-match", "\"upd9\"")
            .with_status(304)
            .create();
        assert!(matches!(
            client.updated_entry_ids_conditional(&validators).unwrap(),
            Conditional::NotModified
        ));
        m304.assert();
    }

    #[test]
    fn delete_updated_entries_sends_delete_with_json_body() {
        let mut server = mockito::Server::new();
        // Draining the queue is a DELETE with the `updated_entries` key (TASK-44).
        let m = server
            .mock("DELETE", "/v2/updated_entries.json")
            .match_header("content-type", "application/json; charset=utf-8")
            .match_body(r#"{"updated_entries":[7,8]}"#)
            .with_status(200)
            .with_body("[7,8]")
            .create();
        let client = test_client(&server);
        assert_eq!(client.delete_updated_entries(&[7, 8]).unwrap(), vec![7, 8]);
        m.assert();
    }

    #[test]
    fn empty_updated_delete_makes_no_request() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("DELETE", "/v2/updated_entries.json")
            .expect(0)
            .create();
        let client = test_client(&server);
        assert!(client.delete_updated_entries(&[]).unwrap().is_empty());
        m.assert();
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
    fn entries_request_uses_extended_mode_and_parses_extended_objects() {
        let mut server = mockito::Server::new();
        let body = r#"[
            {"id": 1, "feed_id": 7, "title": "Ep",
             "images": {"size_1": {"cdn_url": "https://cdn/lead.jpg"}},
             "enclosure": {"enclosure_url": "https://cdn/ep.mp3", "enclosure_type": "audio/mpeg", "itunes_duration": "2823"},
             "json_feed": {"external_url": "https://example.com/linked"}},
            {"id": 2, "feed_id": 7, "title": "Plain"}
        ]"#;
        server
            .mock("GET", "/v2/entries.json")
            // Assert both the ids and mode=extended are sent.
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("ids".into(), "1,2".into()),
                mockito::Matcher::UrlEncoded("mode".into(), "extended".into()),
            ]))
            .with_status(200)
            .with_body(body)
            .create();
        let client = test_client(&server);
        let entries = client.entries(&[1, 2]).unwrap();
        assert_eq!(entries[0].lead_image_url(), Some("https://cdn/lead.jpg"));
        assert_eq!(entries[0].enclosure_url(), Some("https://cdn/ep.mp3"));
        assert_eq!(
            entries[0].external_url(),
            Some("https://example.com/linked")
        );
        // Absent extended objects on a plain entry degrade to None.
        assert_eq!(entries[1].lead_image_url(), None);
        assert_eq!(entries[1].enclosure_url(), None);
        assert_eq!(entries[1].external_url(), None);
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
    fn subscriptions_parse_feed_and_site_urls_for_export() {
        let mut server = mockito::Server::new();
        let body = r#"[
            {"id": 1, "feed_id": 7, "title": "Rust Blog", "feed_url": "https://blog.rust-lang.org/feed.xml", "site_url": "https://blog.rust-lang.org"},
            {"id": 2, "feed_id": 9, "title": "No Site", "feed_url": "https://example.com/feed.xml", "site_url": null}
        ]"#;
        server
            .mock("GET", "/v2/subscriptions.json")
            .with_status(200)
            .with_body(body)
            .create();
        let client = test_client(&server);
        let subs = client.subscriptions().unwrap();
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].feed_id, 7);
        assert_eq!(
            subs[0].feed_url.as_deref(),
            Some("https://blog.rust-lang.org/feed.xml")
        );
        assert_eq!(
            subs[0].site_url.as_deref(),
            Some("https://blog.rust-lang.org")
        );
        assert_eq!(subs[1].site_url, None);
    }

    #[test]
    fn create_import_posts_opml_as_text_xml() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("POST", "/v2/imports.json")
            .match_header("content-type", "text/xml")
            .match_body("<opml/>")
            .with_status(200)
            .with_body(
                r#"{"id": 6, "complete": false, "import_items": [
                {"title": "A", "feed_url": "https://a/feed", "status": "pending"}
            ]}"#,
            )
            .create();
        let client = test_client(&server);
        let import = client.create_import(b"<opml/>").unwrap();
        m.assert();
        assert_eq!(import.id, 6);
        assert!(!import.complete);
        assert_eq!(import.import_items.len(), 1);
    }

    #[test]
    fn import_status_reports_completion_and_tally() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/v2/imports/6.json")
            .with_status(200)
            .with_body(
                r#"{"id": 6, "complete": true, "import_items": [
                {"title": "A", "feed_url": "https://a/feed", "status": "complete"},
                {"title": "B", "feed_url": "https://b/feed", "status": "complete"},
                {"title": "C", "feed_url": "https://dead/feed", "status": "failed"}
            ]}"#,
            )
            .create();
        let client = test_client(&server);
        let import = client.import_status(6).unwrap();
        assert!(import.complete);
        let tally = import.tally();
        assert_eq!(
            tally,
            ImportTally {
                complete: 2,
                pending: 0,
                failed: 1,
                failed_urls: vec!["https://dead/feed".to_string()],
            }
        );
    }

    #[test]
    fn import_tally_counts_pending_and_unknown_statuses() {
        // A still-processing import: pending items (and any status Feedbin might
        // add later) fall into `pending`, with no failed URLs collected yet.
        let import = Import {
            id: 1,
            complete: false,
            import_items: vec![
                ImportItem {
                    feed_url: Some("https://a/feed".to_string()),
                    status: "pending".to_string(),
                },
                ImportItem {
                    feed_url: None,
                    status: "queued".to_string(),
                },
            ],
        };
        let tally = import.tally();
        assert_eq!(tally.complete, 0);
        assert_eq!(tally.failed, 0);
        assert_eq!(tally.pending, 2);
        assert!(tally.failed_urls.is_empty());
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

    #[test]
    fn request_times_out_instead_of_hanging() {
        use std::net::TcpListener;

        // A server that accepts the connection but never sends a response,
        // holding the socket open past the client's short timeout — so the
        // client times out rather than observing a closed connection. (mockito
        // has no response-delay API, hence the raw listener.)
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _server = std::thread::spawn(move || {
            if let Ok((_stream, _)) = listener.accept() {
                std::thread::sleep(Duration::from_millis(1000));
            }
        });

        let credentials = Credentials {
            email: "reader@example.com".to_string(),
            password: "swordfish".to_string(),
        };
        let client = Client::with_base_url_and_timeout(
            &credentials,
            &format!("http://{addr}/v2"),
            Duration::from_millis(250),
        )
        .unwrap();

        let start = std::time::Instant::now();
        let err = client.authenticate().unwrap_err();
        let elapsed = start.elapsed();
        let chain = format!("{err:#}");

        // The 250ms timeout must fire well before the server closes at 1s (and
        // far below the 30s production ceiling), proving the timeout is applied
        // rather than the request hanging.
        assert!(
            elapsed < Duration::from_millis(750),
            "request should time out promptly, took {elapsed:?}"
        );
        assert!(
            chain.to_lowercase().contains("time"),
            "error should indicate a timeout: {chain}"
        );
    }
}
