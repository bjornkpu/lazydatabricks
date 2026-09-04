//! The network boundary. Everything that talks to Databricks lives here.

mod auth;
pub mod models;

use std::time::{Duration, Instant};

use anyhow::Result;
use reqwest::Client as Http;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;

use crate::app::{ApiCall, Message};
use models::{Job, JobsList, Run, RunsList};

/// Page size sent to Databricks. A page size, not a cap: `list_jobs` follows `next_page_token`.
const PAGE_SIZE: &str = "25";

/// Authenticated HTTP client for one workspace. Every call is reported to the API log.
pub struct Client {
    host: String,
    token: String,
    http: Http,
    log: mpsc::Sender<Message>,
}

impl Client {
    /// Reads the host for `profile` from `~/.databrickscfg` and mints a token via the CLI.
    pub fn from_profile(profile: &str, log: mpsc::Sender<Message>) -> Result<Self> {
        let host = auth::host(profile)?;
        let token = auth::token(profile)?;
        let http = Http::builder().timeout(Duration::from_secs(30)).build()?;
        Ok(Self {
            host,
            token,
            http,
            log,
        })
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Lists jobs, following `next_page_token` until `max` jobs or the last page.
    pub async fn list_jobs(&self, max: usize) -> Result<Vec<Job>> {
        let mut jobs = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut query = vec![("limit", PAGE_SIZE)];
            if let Some(token) = &page_token {
                query.push(("page_token", token.as_str()));
            }
            let page: JobsList = self.get("/api/2.2/jobs/list", &query).await?;
            jobs.extend(page.jobs);
            page_token = page.next_page_token.filter(|token| !token.is_empty());
            if jobs.len() >= max || page_token.is_none() {
                break;
            }
        }
        jobs.truncate(max);
        Ok(jobs)
    }

    /// The most recent runs of one job, newest first as Databricks returns them. One page.
    pub async fn list_runs(&self, job_id: i64) -> Result<Vec<Run>> {
        let job_id = job_id.to_string();
        let query = [("job_id", job_id.as_str()), ("limit", PAGE_SIZE)];
        let page: RunsList = self.get("/api/2.2/jobs/runs/list", &query).await?;
        Ok(page.runs)
    }

    /// One GET, timed and reported to the API log whether it succeeds or not.
    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let request = self
            .http
            .get(format!("{}{path}", self.host))
            .bearer_auth(&self.token)
            .query(query)
            .build()?;
        let url = request.url();
        let logged_path = url
            .query()
            .map_or_else(|| url.path().to_owned(), |q| format!("{}?{q}", url.path()));
        let started = Instant::now();
        let response = self.http.execute(request).await;
        let call = ApiCall {
            method: "GET",
            path: logged_path,
            status: response.as_ref().ok().map(|r| r.status().as_u16()),
            duration: started.elapsed(),
        };
        // Best effort: a closed channel means the app has already quit.
        let _ = self.log.send(Message::ApiCalled(call)).await;
        Ok(response?.error_for_status()?.json().await?)
    }
}
