---
name: wam-npm-release
description: Release the Spectral Freeze headless WAM npm package. Use when bumping, packaging, publishing, debugging CI releases, or updating npm Trusted Publishing for this repo.
---

# WAM npm Release

Use this skill for releases of the `spectral-freeze-wam` npm package from this repository.

## Package and workflow

- npm package directory: `wam-shell/`
- npm package manifest: `wam-shell/package.json`
- package name: `spectral-freeze-wam`
- GitHub workflow: `.github/workflows/npm-release.yml`
- npm publishing uses **Trusted Publishing**. Do not add or use an `NPM_TOKEN` secret.

## Release checklist

1. Bump `wam-shell/package.json` `version`.
2. Add a short release note in root `README.md`.
3. Verify locally:
   ```sh
   cargo test -p dsp -p spectral-freeze-wam
   cd wam-shell
   npm run build
   npm pack --dry-run
   ```
4. Remove generated ignored files if present:
   ```sh
   rm -rf wam-shell/dist wam-shell/*.tgz
   ```
5. Commit and push.
6. Tag and push:
   ```sh
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

## Important gotchas

- npm versions are immutable. Never reuse a version after npm publish succeeds.
- The release workflow must not test the full workspace on Linux. `clap-shell` pulls GUI/OpenGL/X11 dependencies. Test only:
  ```sh
  cargo test -p dsp -p spectral-freeze-wam
  ```
- The workflow uses Node 24 for Trusted Publishing.
- `wam-shell/package.json` must include repository metadata matching the GitHub repo provenance.

If publishing fails, read `references/troubleshooting.md`.
