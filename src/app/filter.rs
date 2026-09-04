//! Who "me" is, and which jobs are visible.

use crate::api::models::{Job, Pipeline};

/// The signed-in user, resolved once at startup from the SCIM `Me` endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Me {
    pub email: String,
    /// Value the platform's `dev` tag uses for this user: the email's local part with `.`
    /// replaced by `_`. A convention of this workspace; config can override it at M7.
    pub tag: String,
}

impl Me {
    #[must_use]
    pub fn from_email(email: &str) -> Self {
        let local = email.split_once('@').map_or(email, |(local, _)| local);
        Self {
            email: email.to_owned(),
            tag: local.replace('.', "_"),
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
}

impl Filter {
    /// Whether `job` is visible. With `mine_only` and no `me` yet, nothing matches: better an
    /// honest empty list than a silently unfiltered one.
    #[must_use]
    pub fn matches(&self, job: &Job, me: Option<&Me>) -> bool {
        let name_ok = self.text.is_empty()
            || job
                .settings
                .name
                .to_lowercase()
                .contains(&self.text.to_lowercase());
        let mine_ok = !self.mine_only
            || me.is_some_and(|me| {
                job.settings.tags.get("dev") == Some(&me.tag) || job.creator_user_name == me.email
            });
        name_ok && mine_ok
    }

    /// Pipelines carry no tags in the list response, so "mine" is the creator alone.
    #[must_use]
    pub fn matches_pipeline(&self, pipeline: &Pipeline, me: Option<&Me>) -> bool {
        let name_ok = self.text.is_empty()
            || pipeline
                .name
                .to_lowercase()
                .contains(&self.text.to_lowercase());
        let mine_ok =
            !self.mine_only || me.is_some_and(|me| pipeline.creator_user_name == me.email);
        name_ok && mine_ok
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
        };
        assert!(filter.matches(&job(1, "okonomi_gold"), None));
        assert!(!filter.matches(&job(1, "bronze_ingest"), None));
        assert!(Filter::default().matches(&job(1, "anything"), None));
    }

    #[test]
    fn mine_matches_tag_or_creator() {
        let filter = Filter {
            text: String::new(),
            mine_only: true,
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
        assert!(filter.matches(&by_tag, Some(&me())));
        assert!(filter.matches(&by_creator, Some(&me())));
        assert!(!filter.matches(&theirs, Some(&me())));
        assert!(!filter.matches(&by_tag, None), "unknown me matches nothing");
    }
}
