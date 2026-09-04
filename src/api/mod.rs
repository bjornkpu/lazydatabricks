//! The network boundary. Everything that talks to Databricks lives here.

mod auth;
pub mod models;

use std::time::{Duration, Instant};

use reqwest::{Client as Http, Method};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc};

use crate::app::{ApiCall, Message};
use crate::error::AppError;
use auth::Token;
use models::{Job, JobsList, Run, RunNowResponse, RunsList, ScimMe};

/// Page size sent to Databricks. A page size, not a cap: `list_jobs` follows `next_page_token`.
const PAGE_SIZE: &str = "25";
/// Mint a fresh token when the current one has less than this left. Tokens live an hour.
const TOKEN_REFRESH_MARGIN: Duration = Duration::from_secs(300);
/// Per-request timeout; surfaces as `AppError::Timeout`.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Authenticated HTTP client for one workspace. Every call is reported to the API log.
pub struct Client {
    host: String,
    profile: String,
    /// Refreshed in place before it expires; the lock serialises that refresh.
    token: Mutex<Token>,
    http: Http,
    log: mpsc::Sender<Message>,
}

impl Client {
    /// Reads the host for `profile` from `~/.databrickscfg` and mints a token via the CLI.
    pub fn from_profile(profile: &str, log: mpsc::Sender<Message>) -> Result<Self, AppError> {
        let host = auth::host(profile)?;
        let token = auth::mint(profile)?;
        let http = Http::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| AppError::from_reqwest(&error, "client setup"))?;
        Ok(Self {
            host,
            profile: profile.to_owned(),
            token: Mutex::new(token),
            http,
            log,
        })
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Lists jobs, following `next_page_token` until `max` jobs or the last page.
    pub async fn list_jobs(&self, max: usize) -> Result<Vec<Job>, AppError> {
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
    pub async fn list_runs(&self, job_id: i64) -> Result<Vec<Run>, AppError> {
        let job_id = job_id.to_string();
        let query = [("job_id", job_id.as_str()), ("limit", PAGE_SIZE)];
        let page: RunsList = self.get("/api/2.2/jobs/runs/list", &query).await?;
        Ok(page.runs)
    }

    /// The signed-in user's name (an email). Resolved once at startup for the "mine" filter.
    pub async fn me(&self) -> Result<String, AppError> {
        let me: ScimMe = self.get("/api/2.0/preview/scim/v2/Me", &[]).await?;
        Ok(me.user_name)
    }

    /// Starts a run of `job_id` and returns the new run's id.
    pub async fn run_now(&self, job_id: i64) -> Result<i64, AppError> {
        let started: RunNowResponse = self
            .post("/api/2.2/jobs/run-now", json!({ "job_id": job_id }))
            .await?;
        Ok(started.run_id)
    }

    /// Asks Databricks to cancel `run_id`. The run reaches TERMINATED a little later.
    pub async fn cancel_run(&self, run_id: i64) -> Result<(), AppError> {
        let _: Value = self
            .post("/api/2.2/jobs/runs/cancel", json!({ "run_id": run_id }))
            .await?;
        Ok(())
    }

    /// The current bearer token, minted anew via the CLI when it is about to expire.
    async fn bearer(&self) -> Result<String, AppError> {
        let mut token = self.token.lock().await;
        if token.expires_within(TOKEN_REFRESH_MARGIN) {
            // The CLI call blocks on a subprocess, so it leaves the async threads alone.
            let profile = self.profile.clone();
            *token = tokio::task::spawn_blocking(move || auth::mint(&profile))
                .await
                .map_err(|error| AppError::Internal(error.to_string()))??;
        }
        // Copied out so the lock is not held across the request.
        Ok(token.access_token.clone())
    }

    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, AppError> {
        self.send(Method::GET, path, query, None).await
    }

    async fn post<T: DeserializeOwned>(&self, path: &str, body: Value) -> Result<T, AppError> {
        self.send(Method::POST, path, &[], Some(body)).await
    }

    /// One request, timed and reported to the API log whether it succeeds or not. Failures come
    /// back classified: token, permission, timeout, network, or bad JSON.
    async fn send<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<T, AppError> {
        let mut builder = self
            .http
            .request(method.clone(), format!("{}{path}", self.host))
            .bearer_auth(self.bearer().await?)
            .query(query);
        if let Some(body) = body {
            builder = builder.json(&body);
        }
        let request = builder
            .build()
            .map_err(|error| AppError::from_reqwest(&error, path))?;
        let url = request.url();
        let logged_path = url
            .query()
            .map_or_else(|| url.path().to_owned(), |q| format!("{}?{q}", url.path()));
        let started = Instant::now();
        let response = self.http.execute(request).await;
        let call = ApiCall {
            method: method.to_string(),
            path: logged_path.clone(),
            status: response.as_ref().ok().map(|r| r.status().as_u16()),
            duration: started.elapsed(),
        };
        // Best effort: a closed channel means the app has already quit.
        let _ = self.log.send(Message::ApiCalled(call)).await;
        let response = response.map_err(|error| AppError::from_reqwest(&error, &logged_path))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::from_status(
                status.as_u16(),
                &logged_path,
                &body,
                &self.profile,
            ));
        }
        response
            .json()
            .await
            .map_err(|error| AppError::from_reqwest(&error, &logged_path))
    }
}
