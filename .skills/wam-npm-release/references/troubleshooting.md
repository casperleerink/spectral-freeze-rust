# WAM npm release troubleshooting

## Trusted Publishing configuration

The npm package `spectral-freeze-wam` must have a GitHub Actions trusted publisher configured exactly as:

- Organization or user: `casperleerink`
- Repository: `spectral-freeze-rust`
- Workflow filename: `npm-release.yml`
- Environment name: blank, unless the workflow is changed to use a GitHub environment

The workflow file path in the repo is `.github/workflows/npm-release.yml`, but npm's field wants only the filename.

## No NPM_TOKEN

Do not use `NODE_AUTH_TOKEN` or an `NPM_TOKEN` secret. Trusted Publishing requires:

```yaml
permissions:
  contents: write
  id-token: write
```

and the publish step should be:

```yaml
- name: Publish to npm
  if: startsWith(github.ref, 'refs/tags/')
  run: npm publish --access public
  working-directory: wam-shell
```

## Known errors

### EOTP: one-time password required

Cause: token-based publishing with 2FA, or stale `NODE_AUTH_TOKEN` usage.

Fix: remove token usage and use Trusted Publishing.

### E404 during npm publish

Cause: unauthenticated publish, commonly because Trusted Publishing does not match owner/repo/workflow/package.

Fix: verify trusted publisher settings and that the workflow has `id-token: write`.

### E422 repository.url mismatch

Cause: npm provenance expects package `repository.url` to match the GitHub repository.

Fix: `wam-shell/package.json` should include:

```json
"repository": {
  "type": "git",
  "url": "git+https://github.com/casperleerink/spectral-freeze-rust.git",
  "directory": "wam-shell"
}
```

### Linux CI fails building x11/gl

Cause: the workflow is testing/building `clap-shell`, which depends on native GUI/OpenGL/X11 libraries.

Fix: for the WAM npm workflow, only run:

```sh
cargo test -p dsp -p spectral-freeze-wam
```

### npm self-update fails on GitHub Actions

Cause: hosted npm install corruption/bug encountered with `npm install -g npm@latest`.

Fix: do not self-update npm. Use `actions/setup-node` with Node 24.
