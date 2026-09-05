//! Who "me" is, and which jobs are visible.

use serde::Deserialize;

use crate::api::models::{
    Cluster, ClusterState, ComputeKind, Job, Pipeline, PipelineState, Run, UpdateState,
};

/// The signed-in user, resolved once at startup from the SCIM `Me` endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Me {
    pub email: String,
    /// Value the platform's `dev` tag uses for this user: the email's local part with `.`
    /// replaced by `_`. A convention of this workspace; config can override it at M7.
    pub tag: String,
    /// Other names that count as me: service principals that deploy my bundles. From config.
    pub aliases: Vec<String>,
}

impl Me {
    #[must_use]
    pub fn from_email(email: &str) -> Self {
        let local = email.split_once('@').map_or(email, |(local, _)| local);
        Self {
            email: email.to_owned(),
            tag: local.replace('.', "_"),
            aliases: Vec::new(),
        }
    }

    /// Mine by name or ownership: a `[<tag>]` prefix (what asset bundles write in development
    /// mode), me as creator or run-as, or any alias in either role.
    fn owns(&self, name: &str, creator: &str, run_as: &str) -> bool {
        name.starts_with(&format!("[{}]", self.tag))
            || [creator, run_as]
                .iter()
                .any(|who| *who == self.email || self.aliases.iter().any(|alias| alias == who))
    }
}

/// Case-insensitive substring match of `needle` in any of `haystacks`.
fn contains_any<'a>(needle: &str, haystacks: impl IntoIterator<Item = &'a str>) -> bool {
    let needle = needle.to_lowercase();
    haystacks
        .into_iter()
        .any(|hay| hay.to_lowercase().contains(&needle))
}

/// Which rows a list shows by the state of their newest run or update. `f` cycles it; `status`
/// in config starts it. The same rule the row glyph uses, so `failed` is exactly the `✗` rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    All,
    /// Newest run ended in anything but success.
    Failed,
    /// Newest run is still going.
    Active,
}

impl Status {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::All => Self::Failed,
            Self::Failed => Self::Active,
            Self::Active => Self::All,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Failed => "failed",
            Self::Active => "active",
        }
    }

    /// A job's newest known run, or `None` when none is known. Unknown passes only `All`.
    #[must_use]
    pub fn allows_run(self, latest: Option<&Run>) -> bool {
        match self {
            Self::All => true,
            Self::Failed => latest.is_some_and(|run| run.state.is_failure()),
            Self::Active => latest.is_some_and(|run| run.state.life_cycle_state.is_active()),
        }
    }

    /// Clusters: active is anything billing; failed is `ERROR`.
    #[must_use]
    pub const fn allows_cluster(self, cluster: &Cluster) -> bool {
        match self {
            Self::All => true,
            Self::Failed => matches!(cluster.state, ClusterState::Error),
            Self::Active => cluster.state.is_active(),
        }
    }

    /// The latest update decides; a pipeline without updates falls back to its own state.
    #[must_use]
    pub fn allows_pipeline(self, pipeline: &Pipeline) -> bool {
        let latest = pipeline.latest_updates.first().map(|update| update.state);
        match self {
            Self::All => true,
            Self::Failed => latest.map_or_else(
                || pipeline.state == PipelineState::Failed,
                |state| matches!(state, UpdateState::Failed | UpdateState::Canceled),
            ),
            Self::Active => {
                latest.map_or_else(|| pipeline.state.is_active(), |state| !state.is_done())
            }
        }
    }
}

/// Client-side job filter. The jobs API's `name` parameter is an exact match, not a substring
/// search, so filtering happens here; a few hundred rows makes that free.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Case-insensitive substring of the job name.
    pub text: String,
    /// Only jobs tagged `dev=<my tag>` or created by me.
    pub mine_only: bool,
    /// All, failed only, or active only.
    pub status: Status,
}

impl Filter {
    /// Whether `job` is visible given its newest known run. The text matches the name, the
    /// creator or the run-as user, so `/olav` is also "everything Olav left behind". With
    /// `mine_only` and no `me` yet, nothing matches: better an honest empty list than a silently
    /// unfiltered one.
    #[must_use]
    pub fn matches(&self, job: &Job, latest: Option<&Run>, me: Option<&Me>) -> bool {
        let name_ok = self.text.is_empty()
            || contains_any(
                &self.text,
                [
                    job.settings.name.as_str(),
                    job.creator_user_name.as_str(),
                    job.run_as_user_name.as_str(),
                ],
            );
        let mine_ok = !self.mine_only
            || me.is_some_and(|me| {
                job.settings.tags.get("dev") == Some(&me.tag)
                    || me.owns(
                        &job.settings.name,
                        &job.creator_user_name,
                        &job.run_as_user_name,
                    )
            });
        name_ok && mine_ok && self.status.allows_run(latest)
    }

    /// Compute: name or creator text; clusters are mine by creator or alias, warehouses are
    /// shared infrastructure and always pass `mine`, so a serverless workspace never shows an
    /// empty panel behind `m`.
    #[must_use]
    pub fn matches_cluster(&self, cluster: &Cluster, me: Option<&Me>) -> bool {
        let name_ok = self.text.is_empty()
            || contains_any(
                &self.text,
                [cluster.name.as_str(), cluster.creator_user_name.as_str()],
            );
        let mine_ok = !self.mine_only
            || cluster.kind == ComputeKind::Warehouse
            || me.is_some_and(|me| {
                me.owns(
                    &cluster.name,
                    &cluster.creator_user_name,
                    &cluster.creator_user_name,
                )
            });
        name_ok && mine_ok && self.status.allows_cluster(cluster)
    }

    /// Pipelines carry no tags in the list response, so "mine" is the name prefix or the creator.
    #[must_use]
    pub fn matches_pipeline(&self, pipeline: &Pipeline, me: Option<&Me>) -> bool {
        let name_ok = self.text.is_empty()
            || contains_any(
                &self.text,
                [pipeline.name.as_str(), pipeline.creator_user_name.as_str()],
            );
        let mine_ok = !self.mine_only
            || me.is_some_and(|me| {
                me.owns(
                    &pipeline.name,
                    &pipeline.creator_user_name,
                    &pipeline.creator_user_name,
                )
            });
        name_ok && mine_ok && self.status.allows_pipeline(pipeline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::job;

    fn me() -> Me {
        Me::from_email("bjorn.punsvik@example.com")
    }

    #[test]
    fn tag_is_local_part_with_underscores() {
        assert_eq!(me().tag, "bjorn_punsvik");
        assert_eq!(Me::from_email("nobody").tag, "nobody");
    }

    #[test]
    fn pipelines_match_by_name_and_creator() {
        let filter = Filter {
            text: "gold".to_owned(),
            mine_only: true,
            status: Status::All,
        };
        let mine = crate::app::tests::pipeline("p1", "felles_gold", "bjorn.punsvik@example.com");
        let theirs = crate::app::tests::pipeline("p2", "felles_gold", "other@example.com");
        assert!(filter.matches_pipeline(&mine, Some(&me())));
        assert!(!filter.matches_pipeline(&theirs, Some(&me())));
        assert!(!filter.matches_pipeline(&mine, None));
    }

    #[test]
    fn name_filter_is_case_insensitive_substring() {
        let filter = Filter {
            text: "GOLD".to_owned(),
            mine_only: false,
            status: Status::All,
        };
        assert!(filter.matches(&job(1, "okonomi_gold"), None, None));
        assert!(!filter.matches(&job(1, "bronze_ingest"), None, None));
        assert!(Filter::default().matches(&job(1, "anything"), None, None));
    }

    #[test]
    fn mine_matches_tag_or_creator() {
        let filter = Filter {
            text: String::new(),
            mine_only: true,
            status: Status::All,
        };
        let mut by_tag = job(1, "a");
        by_tag.creator_user_name = "other@example.com".to_owned();
        by_tag
            .settings
            .tags
            .insert("dev".to_owned(), "bjorn_punsvik".to_owned());
        let mut by_creator = job(2, "b");
        by_creator.creator_user_name = "bjorn.punsvik@example.com".to_owned();
        let theirs = job(3, "c");
        assert!(filter.matches(&by_tag, None, Some(&me())));
        assert!(filter.matches(&by_creator, None, Some(&me())));
        assert!(!filter.matches(&theirs, None, Some(&me())));
        assert!(
            !filter.matches(&by_tag, None, None),
            "unknown me matches nothing"
        );
    }

    #[test]
    fn text_also_matches_owners() {
        let filter = Filter {
            text: "OTHER@".to_owned(),
            ..Filter::default()
        };
        assert!(filter.matches(&crate::app::tests::theirs(1, "x"), None, None));
        assert!(!filter.matches(&job(1, "x"), None, None));
        let pipeline = crate::app::tests::pipeline("p", "x", "other@example.com");
        assert!(filter.matches_pipeline(&pipeline, None));
    }

    #[test]
    fn warehouses_are_shared_so_mine_keeps_them() {
        let filter = Filter {
            mine_only: true,
            ..Filter::default()
        };
        let mut theirs = crate::app::tests::cluster("c1", "shared", ClusterState::Running);
        theirs.creator_user_name = "other@example.com".to_owned();
        assert!(
            !filter.matches_cluster(&theirs, Some(&me())),
            "their cluster"
        );
        theirs.kind = ComputeKind::Warehouse;
        assert!(
            filter.matches_cluster(&theirs, Some(&me())),
            "their warehouse"
        );
        assert!(
            filter.matches_cluster(&theirs, None),
            "even before me is known"
        );
    }

    #[test]
    fn mine_includes_bundle_prefix_and_aliases() {
        let filter = Filter {
            mine_only: true,
            ..Filter::default()
        };
        let mut with_alias = me();
        with_alias.aliases = vec!["sp-1234".to_owned()];
        let mut prefixed = crate::app::tests::theirs(1, "[bjorn_punsvik] churn");
        prefixed.creator_user_name = "sp-1234".to_owned();
        assert!(filter.matches(&prefixed, None, Some(&with_alias)), "prefix");
        let mut by_alias = crate::app::tests::theirs(2, "churn");
        by_alias.run_as_user_name = "sp-1234".to_owned();
        assert!(
            filter.matches(&by_alias, None, Some(&with_alias)),
            "alias as run-as"
        );
        assert!(
            !filter.matches(&by_alias, None, Some(&me())),
            "no alias configured, not mine"
        );
        let pipeline = crate::app::tests::pipeline("p", "[bjorn_punsvik] gold", "sp-1234");
        assert!(filter.matches_pipeline(&pipeline, Some(&with_alias)));
        assert!(
            filter.matches_pipeline(&pipeline, Some(&me())),
            "prefix alone is enough"
        );
    }

    #[test]
    fn status_follows_the_newest_run() {
        use crate::api::models::ResultState;
        use crate::app::tests::run;
        let failed = run(1, 1000, 2000, Some(ResultState::Failed));
        let running = run(2, 1000, 0, None);
        let ok = run(3, 1000, 2000, Some(ResultState::Success));
        assert!(Status::All.allows_run(None));
        assert!(Status::Failed.allows_run(Some(&failed)));
        assert!(!Status::Failed.allows_run(Some(&ok)));
        assert!(!Status::Failed.allows_run(None), "never run is not failed");
        assert!(Status::Active.allows_run(Some(&running)));
        assert!(!Status::Active.allows_run(Some(&failed)));
        assert_eq!(Status::Active.next(), Status::All);
        let filter = Filter {
            status: Status::Failed,
            ..Filter::default()
        };
        assert!(filter.matches(&job(1, "a"), Some(&failed), None));
        assert!(!filter.matches(&job(1, "a"), Some(&ok), None));
    }

    #[test]
    fn pipeline_status_prefers_the_latest_update() {
        let mut pipeline = crate::app::tests::pipeline("p", "x", "me@example.com");
        assert!(!Status::Failed.allows_pipeline(&pipeline), "completed");
        pipeline.latest_updates[0].state = UpdateState::Running;
        assert!(Status::Active.allows_pipeline(&pipeline));
        pipeline.latest_updates[0].state = UpdateState::Failed;
        assert!(Status::Failed.allows_pipeline(&pipeline));
        pipeline.latest_updates.clear();
        pipeline.state = PipelineState::Failed;
        assert!(
            Status::Failed.allows_pipeline(&pipeline),
            "no updates: own state"
        );
        pipeline.state = PipelineState::Running;
        assert!(Status::Active.allows_pipeline(&pipeline));
    }
}
