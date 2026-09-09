# GitHub Pages

The public site is https://ctoth.github.io/z-core/ and the debugger is at
https://ctoth.github.io/z-core/demo/.

`.github/workflows/pages.yml` builds the browser WASM package, assembles only
the public runtime assets, and exercises the staged site in Chromium under
`/z-core/`. Pull requests build and test without deploying. Pushes to `master`
and manual workflow runs on `master` deploy to the `github-pages` environment.
The repository's Pages publishing source must be **GitHub Actions**.

To reproduce the site locally, build the web package as described in the
[binding README](../crates/z180-wasm/README.md), then from the repository root:

```powershell
node tools/build-pages.mjs
cd crates/z180-wasm
npm ci
npx playwright install chromium
$env:PAGES_ROOT = '../../target/pages'
npm run test:browser
Remove-Item Env:PAGES_ROOT
```

The staging script replaces `target/pages/`. Serve that directory with any
static HTTP server to preview the landing page and debugger. Relative asset
URLs support both project subpaths and local hosting. To run the same smoke
test against the deployed site, set `PAGES_URL` to its URL with a trailing slash.

Edit `docs/site/index.html` for the landing page and `crates/z180-wasm/demo/`
for the workbench. Generated WebAssembly stays in ignored build directories;
GitHub Actions uploads it as a Pages artifact. No ROM uploads, secrets, server,
or custom domain are needed.
