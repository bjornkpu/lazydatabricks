# Releasing

Merging the release PR is the whole ceremony. Everything else is generated.

```
push to main
    |
    +-> release-plz.yml, job "release-pr"
            reads conventional commits since the last tag, derives the next version,
            updates CHANGELOG.md and Cargo.toml, opens or refreshes one PR:
            "chore(release): v0.2.0"        <- sits and waits, refreshes as you merge more work
                |
                |  you merge it
                v
        release-plz.yml, job "release"
            pushes tag v0.2.0 and runs cargo publish
                |
                v
        release.yml (dist), triggered by the tag
            builds 5 targets, creates the GitHub Release, uploads the installers
```

Version comes from commit types: any `feat:` bumps the minor, `fix:`/`build:`/`chore:` bump the
patch, and `feat!:` or a `BREAKING CHANGE:` footer bumps the minor while below 1.0. Override by
editing the version in the release PR before merging.

A package manager never hosts the binary. It hosts a small text file holding a URL and a sha256
pointing back at the GitHub Release. That is why adding Homebrew, Scoop, winget or the AUR later
is cheap: each one is another pointer file generated from the same release.

## Cutting a release

1. Merge the open `chore(release): ...` PR.
2. Watch Actions. Three workflow runs follow, in order: release-plz release, then dist.

There is nothing to do by hand. To release without waiting for the PR, or to fix a botched
release, the manual path still works:

```
# edit version in Cargo.toml
cargo check                       # refreshes Cargo.lock; dist builds --locked, so this matters
git commit -am "chore(release): v0.2.0" && git push
git tag -a v0.2.0 -m "v0.2.0" && git push --tags
cargo publish
```

## Configuration

| File | Owns |
| --- | --- |
| `release-plz.toml` | versioning, changelog, crates.io publish |
| `.github/workflows/release-plz.yml` | the two release-plz jobs |
| `dist-workspace.toml` | targets, installers, install path |
| `.github/workflows/release.yml` | **generated** by `dist generate`, never hand-edit |
| `[profile.dist]` in `Cargo.toml` | release profile used by the CI builds |

`dist plan` prints exactly what a release would produce, without building anything.

Upgrading dist: `cargo install cargo-dist --locked`, then `dist init --yes`, which bumps
`cargo-dist-version` and regenerates the workflow.

## Secrets

| Secret | Used by | Why |
| --- | --- | --- |
| `RELEASE_PLZ_TOKEN` | release-plz | A tag pushed with the default `GITHUB_TOKEN` does not trigger other workflows, so dist would never run. Fine-grained PAT on this repo, Contents and Pull requests read/write. |
| `CARGO_REGISTRY_TOKEN` | release-plz | `cargo publish` |

Both expire. When releases start silently doing nothing, or the publish step 401s, check these
first.

## Not done yet, in rough order of value

**Homebrew tap.** Create the public repo `bjornkpu/homebrew-tap`, add a fine-grained token
scoped to it with Contents: read/write as the Actions secret `HOMEBREW_TAP_TOKEN`, then add
`"homebrew"` to `installers`, plus `tap = "bjornkpu/homebrew-tap"` and
`publish-jobs = ["homebrew"]` to `dist-workspace.toml`. Gives
`brew install bjornkpu/tap/lazydatabricks`.

**Scoop, winget, AUR.** Pointer files, as described at the top. Add when someone asks. dist has
no Debian package generator; Debian users take the shell installer or the archive.
