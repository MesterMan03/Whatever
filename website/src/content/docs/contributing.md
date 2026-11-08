---
title: Contributing to the docs
description: Where to edit guides, canonical reference sources, and the generated message catalogue.
---

## Run the site

From the repository root:

```sh
npm run docs:install
npm run docs:dev
```

The site uses [Astro Starlight](https://starlight.astro.build/). It builds static HTML with local Pagefind search; no documentation server or external search account is required in production.

## Edit the source of truth

| Content | Edit |
| --- | --- |
| Tutorials and guides | `website/src/content/docs/getting-started/` and `guides/` |
| Full scripting API | `docs/SCRIPTING.md` |
| Editor / physics / shaders / sandbox | Their existing `docs/*.md` references |
| Wire contract and examples | `docs/IPC.md` |
| Native message fields | The actual types in `engine/src/script/ipc.rs` and related native DTOs |
| Navigation and theme | `website/astro.config.mjs`, `website/src/styles/docs.css` |

The generator divides the scripting reference into focused pages and renders the native message catalogue from Rust definitions. Generated pages are ignored by Git. Don't edit their output. Regenerate after changing canonical files; restart the dev server or run `npm run docs:generate` while it is running.

New scripting reference sections must be mapped in `website/scripts/generate-docs.mjs`. The build fails if a section is unaccounted for or a native message uses syntax the generator doesn't understand. The legacy IPC summary in `SCRIPTING.md` is replaced on the site by the dedicated protocol specification and generated catalogue.

## Verify a change

```sh
npm run docs:build
npm run docs:preview
```

With the root development dependencies installed, also run `npm run docs:check-examples`.
It rebuilds the API and type-checks the tutorial and homepage examples against its
declarations. CI requires this check before deployment.

The build generates the reference, renders all pages, creates the search index, and checks local links, anchors, and assets. Set `DOCS_BASE` to a repository subpath when checking a project-site deployment. See `website/README.md` for Windows and POSIX examples.

Keep runnable examples aligned with the current API and the embedded VM's capability policy.

## GitHub Pages

The docs workflow builds and checks relevant pull requests. On `main` and manual runs, it uploads and deploys the static output to GitHub Pages. The workflow derives its origin and base path from Pages configuration, so both project sites and custom domains work.

Repository owners must select **GitHub Actions** under **Settings → Pages → Build and deployment → Source** once. The workflow does not commit built output or require a deployment branch.
