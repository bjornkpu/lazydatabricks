//! The network boundary. Everything that talks to Databricks lives here.

mod auth;
pub mod models;

use std::time::Duration;

use anyhow::Result;
use reqwest::blocking::Client as Http;

use models::{Job, JobsList};

/// Page size sent to Databricks. A page size, not a cap: `list_jobs` follows `next_page_token`.
const PAGE_SIZE: &str = "25";

/// Authenticated HTTP client for one workspace.
pub struct Client {
    host: String,
    token: String,
    http: Http,
}

impl Client {
    /// Reads the host for `profile` from `~/.databrickscfg` and mints a token via the CLI.
    pub fn from_profile(profile: &str) -> Result<Self> {
        let host = auth::host(profile)?;
        let token = auth::token(profile)?;
        let http = Http::builder().timeout(Duration::from_secs(30)).build()?;
        Ok(Self { host, token, http })
    }

    /// Lists jobs, following `next_page_token` until `max` jobs or the last page.
    pub fn list_jobs(&self, max: usize) -> Result<Vec<Job>> {
        let mut jobs = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut request = self
                .http
                .get(format!("{}/api/2.2/jobs/list", self.host))
                .bearer_auth(&self.token)
                .query(&[("limit", PAGE_SIZE)]);
            if let Some(token) = &page_token {
                request = request.query(&[("page_token", token)]);
            }
            let page: JobsList = request.send()?.error_for_status()?.json()?;
            jobs.extend(page.jobs);
            page_token = page.next_page_token.filter(|token| !token.is_empty());
            if jobs.len() >= max || page_token.is_none() {
                break;
            }
        }
        jobs.truncate(max);
        Ok(jobs)
    }
}
