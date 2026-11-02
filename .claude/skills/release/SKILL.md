---
name: release
description: A skill to be used when prompted only. Merges night-build into main, which triggers the GitHub release build of the Windows installer.
---

# Release

Day to day work merges into `night-build`. A release is `night-build` merged into `main`: every push to `main` runs `.github/workflows/release.yml`, which builds the `.msi` and publishes it as a GitHub release tagged `v<version>`.

## When to use

ONLY use this skill when the user prompts you. Invoking it is explicit permission to bump the version, push to `night-build` and merge into `main`.

## Steps

1. **Sync.** `git fetch origin main night-build`. Stop and report if `night-build` has nothing `main` does not (`git log origin/main..origin/night-build` is empty).

2. **Check the version.** Read `version` from `src-tauri/tauri.conf.json` and see whether a `v<version>` tag already exists (`git ls-remote --tags origin v<version>`). The release workflow skips a version that is already released, so if it exists the version must be bumped. If the user named a version, use it; otherwise ask whether this is a patch, minor or major release.

3. **Bump, if needed**, on `night-build`. The version lives in four places and they must agree:
   - `package.json`
   - `src-tauri/tauri.conf.json` (the release tag is read from here)
   - `src-tauri/Cargo.toml`
   - the `open-darkroom` entry in `src-tauri/Cargo.lock`

   MSI needs a plain `major.minor.patch` with numbers only, no `-beta` style suffix. Commit as `Bump version to X.Y.Z` and push to `night-build`.

4. **Check CI.** The Tests workflow must be green on the `night-build` head. If it is red, stop and report rather than releasing.

5. **Merge.** Open a pull request from `night-build` into `main` titled `Release vX.Y.Z`, with a short list of what changed since the last release (from `git log origin/main..origin/night-build`). Merge it with a merge commit, never squash or rebase, so `night-build` stays an ancestor of `main` and the next release merges cleanly.

6. **Report.** Give the user the PR link and point them at the Actions tab for the release build. Do not wait on the build unless asked.

## Limits

Never force push either branch. Never delete or move an existing release tag. If the merge conflicts, stop and ask the user rather than resolving it on `main`.
