# Whatever modding docs

An [Astro Starlight](https://starlight.astro.build/) static documentation website.
Requires Node.js 22.23.4+ (Node 24 LTS recommended) and Bun for dependency installation.
It is independent of the native engine build and does not launch mod scripts.

From the repository root:

```sh
bun run docs:install
bun run docs:dev
bun run docs:build
bun run docs:preview
```

The dev server defaults to `http://127.0.0.1:4321`. Search is generated during the
production build; use `docs:preview` to test it. Build output is `website/dist/`.
Dependencies have their own `website/bun.lock`; no native toolchain is needed.

## Content

- Write tutorials and guides in `src/content/docs/`.
- Edit the existing canonical references in `../docs/`: `SCRIPTING.md`, `EDITOR.md`,
  `PHYSICS.md`, `SHADER.md`, `SANDBOX.md`, and `IPC.md`.
- `scripts/generate-docs.mjs` splits the scripting reference into focused pages and
  generates a native message catalogue from `engine/src/script/ipc.rs` and shared DTOs.
  Generated pages are gitignored and recreated before dev/build; don't edit them.
- Every scripting H2 must be mapped to a page. New native enum syntax must be
  handled explicitly. Generation fails instead of silently dropping reference content.
- After changing canonical files while dev is running, run `bun run docs:generate`
  or restart the dev server. Authored guide edits reload normally.
- Links in Markdown use root-relative documentation routes, such as
  `/reference/api/scenes/`. The Markdown plugin adds the configured deployment base.
  Astro components should use `import.meta.env.BASE_URL`.

`docs:build` checks internal routes, anchors, and asset references after rendering.
It doesn't fetch external websites. The native IPC specification distinguishes
declared variants from inactive/unimplemented behavior.

`bun run docs:check-examples` rebuilds the API and type-checks the homepage and
tutorial snippets against its declarations. This separate check needs the root
development dependencies (`bun install --frozen-lockfile`), but no native toolchain.
CI runs it before allowing a Pages deployment.

## GitHub Pages

`.github/workflows/docs.yml` builds relevant pull requests and deploys pushes to
`main` plus manual runs on `main`. It uses the standard GitHub Pages artifact and
deployment actions with job-scoped permissions. PRs never receive deployment
permissions and don't require Pages to be enabled.

Once, set **Settings → Pages → Build and deployment → Source → GitHub Actions**
in the repository. The default project URL will be
`https://mesterman03.github.io/Whatever/`; configure-pages supplies the actual
origin/base path, including a custom domain if one is configured.

This change prepares the workflow locally. It takes effect after the changes are
pushed to GitHub. No built files or deployment branch are committed.

To verify the project base locally in PowerShell:

```powershell
$env:DOCS_SITE = "https://mesterman03.github.io"
$env:DOCS_BASE = "/Whatever"
bun run docs:build
bun run docs:preview
```

Open `http://127.0.0.1:4321/Whatever/`. Remove the overrides for a root preview:

```powershell
Remove-Item Env:DOCS_SITE, Env:DOCS_BASE -ErrorAction SilentlyContinue
```

On POSIX shells:

```sh
DOCS_SITE=https://mesterman03.github.io DOCS_BASE=/Whatever bun run docs:build
DOCS_SITE=https://mesterman03.github.io DOCS_BASE=/Whatever bun run docs:preview
```
