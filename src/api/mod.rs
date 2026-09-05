//! The network boundary. Everything that talks to Databricks lives here.

mod auth;
pub mod models;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use reqwest::{Client as Http, Method};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc};

use crate::app::{ApiCall, Message};
use crate::error::AppError;
use auth::Token;
use models::{
    Cluster, ClustersList, Job, JobsList, Pipeline, PipelinesList, Run, RunNowResponse, RunOutput,
    RunsList, ScimMe, UpdateStartResponse, WarehousesList,
};

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

    /// Lists clusters, following `next_page_token` until `max` clusters or the last page.
    pub async fn list_clusters(&self, max: usize) -> Result<Vec<Cluster>, AppError> {
        let mut clusters = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut query = vec![("page_size", PAGE_SIZE)];
            if let Some(token) = &page_token {
                query.push(("page_token", token.as_str()));
            }
            let page: ClustersList = self.get("/api/2.1/clusters/list", &query).await?;
            clusters.extend(page.clusters);
            page_token = page.next_page_token.filter(|token| !token.is_empty());
            if clusters.len() >= max || page_token.is_none() {
                break;
            }
        }
        clusters.truncate(max);
        Ok(clusters)
    }

    /// Clusters and SQL warehouses as one list. A serverless workspace has no clusters; its
    /// warehouses are the compute it can see.
    pub async fn list_compute(&self, max: usize) -> Result<Vec<Cluster>, AppError> {
        let mut compute = self.list_clusters(max).await?;
        let page: WarehousesList = self.get("/api/2.0/sql/warehouses", &[]).await?;
        compute.extend(page.warehouses.into_iter().map(Cluster::from));
        Ok(compute)
    }

    /// Starts a stopped SQL warehouse.
    pub async fn start_warehouse(&self, warehouse_id: &str) -> Result<(), AppError> {
        let _: Value = self
            .post(
                &format!("/api/2.0/sql/warehouses/{warehouse_id}/start"),
                json!({}),
            )
            .await?;
        Ok(())
    }

    /// Stops a running SQL warehouse.
    pub async fn stop_warehouse(&self, warehouse_id: &str) -> Result<(), AppError> {
        let _: Value = self
            .post(
                &format!("/api/2.0/sql/warehouses/{warehouse_id}/stop"),
                json!({}),
            )
            .await?;
        Ok(())
    }

    /// Starts a terminated cluster.
    pub async fn start_cluster(&self, cluster_id: &str) -> Result<(), AppError> {
        let _: Value = self
            .post(
                "/api/2.1/clusters/start",
                json!({ "cluster_id": cluster_id }),
            )
            .await?;
        Ok(())
    }

    /// Terminates a cluster. Databricks calls this `delete`; the cluster stays listed.
    pub async fn terminate_cluster(&self, cluster_id: &str) -> Result<(), AppError> {
        let _: Value = self
            .post(
                "/api/2.1/clusters/delete",
                json!({ "cluster_id": cluster_id }),
            )
            .await?;
        Ok(())
    }

    /// Lists pipelines, following `next_page_token` until `max` pipelines or the last page.
    pub async fn list_pipelines(&self, max: usize) -> Result<Vec<Pipeline>, AppError> {
        let mut pipelines = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut query = vec![("max_results", PAGE_SIZE)];
            if let Some(token) = &page_token {
                query.push(("page_token", token.as_str()));
            }
            let page: PipelinesList = self.get("/api/2.0/pipelines", &query).await?;
            pipelines.extend(page.statuses);
            page_token = page.next_page_token.filter(|token| !token.is_empty());
            if pipelines.len() >= max || page_token.is_none() {
                break;
            }
        }
        pipelines.truncate(max);
        Ok(pipelines)
    }

    /// The most recent runs across the whole workspace, newest first, up to `max`, plus every
    /// active run however old. Gives every job its latest run in a handful of calls instead of
    /// one call per job, and keeps a weeks-old streaming run from falling out of the window.
    pub async fn list_recent_runs(&self, max: usize) -> Result<Vec<Run>, AppError> {
        let mut runs = self.runs_pages(&[], max).await?;
        let active = self.runs_pages(&[("active_only", "true")], max).await?;
        let seen: std::collections::HashSet<i64> = runs.iter().map(|run| run.id).collect();
        runs.extend(active.into_iter().filter(|run| !seen.contains(&run.id)));
        Ok(runs)
    }

    /// Pages of `runs/list` with `extra` query parameters, until `max` runs or the last page.
    async fn runs_pages(&self, extra: &[(&str, &str)], max: usize) -> Result<Vec<Run>, AppError> {
        let mut runs = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut query = vec![("limit", PAGE_SIZE)];
            query.extend_from_slice(extra);
            if let Some(token) = &page_token {
                query.push(("page_token", token.as_str()));
            }
            let page: RunsList = self.get("/api/2.2/jobs/runs/list", &query).await?;
            runs.extend(page.runs);
            page_token = page.next_page_token.filter(|token| !token.is_empty());
            if runs.len() >= max || page_token.is_none() {
                break;
            }
        }
        runs.truncate(max);
        Ok(runs)
    }

    /// The most recent runs of one job, newest first as Databricks returns them. One page.
    pub async fn list_runs(&self, job_id: i64) -> Result<Vec<Run>, AppError> {
        let job_id = job_id.to_string();
        let query = [("job_id", job_id.as_str()), ("limit", PAGE_SIZE)];
        let page: RunsList = self.get("/api/2.2/jobs/runs/list", &query).await?;
        Ok(page.runs)
    }

    /// One job in full: schedule, deployment, tasks and their clusters.
    pub async fn get_job(&self, job_id: i64) -> Result<Job, AppError> {
        let job_id = job_id.to_string();
        self.get("/api/2.2/jobs/get", &[("job_id", job_id.as_str())])
            .await
    }

    /// One run in full: state message, page URL and its tasks.
    pub async fn get_run(&self, run_id: i64) -> Result<Run, AppError> {
        let run_id = run_id.to_string();
        self.get("/api/2.2/jobs/runs/get", &[("run_id", run_id.as_str())])
            .await
    }

    /// The error and traceback of one task run. Takes a task's `run_id`, not the job run's.
    pub async fn get_run_output(&self, run_id: i64) -> Result<RunOutput, AppError> {
        let run_id = run_id.to_string();
        self.get(
            "/api/2.2/jobs/runs/get-output",
            &[("run_id", run_id.as_str())],
        )
        .await
    }

    /// The signed-in user's name (an email). Resolved once at startup for the "mine" filter.
    pub async fn me(&self) -> Result<String, AppError> {
        let me: ScimMe = self.get("/api/2.0/preview/scim/v2/Me", &[]).await?;
        Ok(me.user_name)
    }

    /// Starts a run of `job_id`, with `job_parameters` when any are given, and returns the new
    /// run's id.
    pub async fn run_now(
        &self,
        job_id: i64,
        params: &BTreeMap<String, String>,
    ) -> Result<i64, AppError> {
        let body = if params.is_empty() {
            json!({ "job_id": job_id })
        } else {
            json!({ "job_id": job_id, "job_parameters": params })
        };
        let started: RunNowResponse = self.post("/api/2.2/jobs/run-now", body).await?;
        Ok(started.run_id)
    }

    /// Re-runs every failed task of `run_id` inside the same run.
    pub async fn repair_run(&self, run_id: i64) -> Result<(), AppError> {
        let _: Value = self
            .post(
                "/api/2.2/jobs/runs/repair",
                json!({ "run_id": run_id, "rerun_all_failed_tasks": true }),
            )
            .await?;
        Ok(())
    }

    /// Asks Databricks to cancel `run_id`. The run reaches TERMINATED a little later.
    pub async fn cancel_run(&self, run_id: i64) -> Result<(), AppError> {
        let _: Value = self
            .post("/api/2.2/jobs/runs/cancel", json!({ "run_id": run_id }))
            .await?;
        Ok(())
    }

    /// Starts an update of `pipeline_id` and returns the new update's id.
    pub async fn start_update(&self, pipeline_id: &str) -> Result<String, AppError> {
        let started: UpdateStartResponse = self
            .post(
                &format!("/api/2.0/pipelines/{pipeline_id}/updates"),
                json!({}),
            )
            .await?;
        Ok(started.update_id)
    }

    /// Stops the running update of `pipeline_id`.
    pub async fn stop_pipeline(&self, pipeline_id: &str) -> Result<(), AppError> {
        let _: Value = self
            .post(&format!("/api/2.0/pipelines/{pipeline_id}/stop"), json!({}))
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
        tracing::debug!(
            method = %method,
            path = %logged_path,
            status = ?call.status,
            ms = call.duration.as_millis(),
            "request"
        );
        // Best effort: a closed channel means the app has already quit.
        let _ = self.log.send(Message::ApiCalled(call)).await;
        let response = response.map_err(|error| {
            tracing::warn!(path = %logged_path, %error, "transport failure");
            AppError::from_reqwest(&error, &logged_path)
        })?;
        let status = response.status();
        if !status.is_success() {
            if status == reqwest::StatusCode::UNAUTHORIZED {
                // Revoked, not expired: forget it so the next call mints a fresh one.
                self.token.lock().await.expire();
            }
            let body = response.text().await.unwrap_or_default();
            tracing::warn!(path = %logged_path, %status, body = %body, "error response");
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
