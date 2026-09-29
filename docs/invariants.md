# Invariants

Rules that must always hold. Each has an id, one sentence, and the test that pins it. Never
break one. An invariant without a test is a bug: write the test or delete the invariant.
Ids are never reused.

| Id | Invariant | Pinned by |
| --- | --- | --- |
| INV-1 | Nothing outside `src/main.rs` imports crossterm. | `src/main.rs::tests::only_main_imports_crossterm` |
| INV-2 | A read-only session refuses an action at Enter and sends no command. | `src/app/mod.rs::tests::read_only_refuses_at_enter` |
| INV-3 | `q` and `ctrl+c` quit even while something is loading. | `src/app/mod.rs::tests::q_and_ctrl_c_quit_even_while_loading` |
| INV-4 | Deleting a job needs its name typed back. | `src/app/mod.rs::tests::deleting_a_job_takes_its_name_typed_back` |
| INV-5 | A reply for one job never shows under another. | `src/app/mod.rs::tests::replies_for_other_jobs_are_not_shown` |
| INV-6 | A failed refresh keeps the list it had: stale beats blank. | `src/app/mod.rs::tests::failed_refresh_keeps_the_list_and_waits_a_ttl` |
| INV-7 | Every fixture in `tests/fixtures/` parses into its model. | `src/api/models.rs::tests::parses_*_fixture`, `warehouses_become_compute_rows` |
| INV-8 | An unknown state from the API never breaks parsing. | `src/api/models.rs::tests::unknown_states_do_not_break_parsing` |
| INV-9 | A typo in the config is an error, never silently ignored. | `src/config.rs::tests::typos_are_errors` |
