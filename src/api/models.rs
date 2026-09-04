//! Serde mirrors of the Databricks REST shapes. Only the fields we use, and `#[serde(default)]`
//! on everything optional, because Databricks omits empty fields rather than nulling them.

use serde::Deserialize;

/// `GET /api/2.2/jobs/list` response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JobsList {
    #[serde(default)]
    pub jobs: Vec<Job>,
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// One job. Ids are `i64` end to end and are never used as indices.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Job {
    pub job_id: i64,
    #[serde(default)]
    pub settings: JobSettings,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct JobSettings {
    #[serde(default)]
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOBS_LIST: &str = include_str!("../../tests/fixtures/jobs_list.json");

    #[test]
    fn parses_jobs_list_fixture() {
        let page: JobsList = serde_json::from_str(JOBS_LIST).unwrap();
        assert_eq!(page.jobs.len(), 2);
        assert_eq!(page.jobs[0].job_id, 1_025_322_370_191_789);
        assert_eq!(page.jobs[0].settings.name, "[someone] okonomi_gold");
        assert_eq!(page.jobs[1].settings.name, "nightly_bronze_ingest");
        assert_eq!(
            page.next_page_token.as_deref(),
            Some("CAIo0JeenYM0Sg80MzEwMTU5NzM2MjUyMDA=")
        );
    }

    #[test]
    fn missing_fields_default() {
        let page: JobsList = serde_json::from_str(r#"{"jobs":[{"job_id":7}]}"#).unwrap();
        assert_eq!(page.jobs[0].settings.name, "");
        assert_eq!(page.next_page_token, None);
    }

    #[test]
    fn empty_object_is_empty_page() {
        let page: JobsList = serde_json::from_str("{}").unwrap();
        assert!(page.jobs.is_empty());
    }
}
