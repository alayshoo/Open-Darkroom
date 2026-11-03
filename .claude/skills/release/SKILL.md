---
name: release
description: A skill to be used when prompted only. Merges main into the release branch, which triggers the GitHub release build of the Windows and macOS installers.
---

# Release

Day to day work merges into `main`, the nightly build. A release is `main` merged into `release`: every push to `release` runs `.github/workflows/release.yml`, which builds the Windows `.msi` and a universal macOS `.dmg` and publishes them as a GitHub release tagged `v<version>`.

## When to use

ONLY use this skill when the user prompts you. Invoking it is explicit permission to bump the version, push the bump to `main` and merge into `release`.

## Steps

1. **Sync.** `git fetch origin main release`. Stop and report if `main` has nothing `release` does not (`git log origin/release..origin/main` is empty).

2. **Check the version.** Read `version` from `src-tauri/tauri.conf.json` and see whether a `v<version>` tag already exists (`git ls-remote --tags origin v<version>`). The release workflow skips a version that is already released, so if it exists the version must be bumped.

   Unless the user already named a version, ask them which kind of release this is before bumping anything, and show what each choice would produce from the current version `X.Y.Z`:
   - **Major** (big or breaking changes): `X` goes up, `Y` and `Z` reset to 0, e.g. `1.4.2` → `2.0.0`.
   - **Minor** (new features): `Y` goes up, `Z` resets to 0, e.g. `1.4.2` → `1.5.0`.
   - **Patch** (fixes only): `Z` goes up, e.g. `1.4.2` → `1.4.3`.

   If the current version has no release yet, also offer to release it as it is.

3. **Bump, if needed**, on `main`. The version lives in four places and they must agree:
   - `package.json`
   - `src-tauri/tauri.conf.json` (the release tag is read from here)
   - `src-tauri/Cargo.toml`
   - the `open-darkroom` entry in `src-tauri/Cargo.lock`

   MSI needs a plain `major.minor.patch` with numbers only, no `-beta` style suffix. Commit as `Bump version to X.Y.Z` and push to `main`.

4. **Check CI.** The Tests workflow must be green on the `main` head. If it is red, stop and report rather than releasing.

5. **Merge.** Open a pull request from `main` into `release` titled `Release vX.Y.Z`, with a short list of what changed since the last release (from `git log origin/release..origin/main`). Merge it with a merge commit, never squash or rebase, so `main` stays an ancestor of `release` and the next release merges cleanly.

6. **Report.** Give the user the PR link and point them at the Actions tab for the release build. Do not wait on the build unless asked.

## Limits

Never force push either branch. Never delete or move an existing release tag. If the merge conflicts, stop and ask the user rather than resolving it on `release`.
