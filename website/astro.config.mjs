import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import { satteri } from "@astrojs/markdown-satteri";
import baseLinks from "./scripts/base-links.mjs";

// Pages passes its real origin and project path. Local preview defaults to /.
export default defineConfig({
  site: process.env.DOCS_SITE || "https://mesterman03.github.io",
  base: process.env.DOCS_BASE || "/",
  trailingSlash: "always",
  markdown: { processor: satteri({ mdastPlugins: [baseLinks(process.env.DOCS_BASE || "/")] }) },
  integrations: [
    starlight({
      title: "Whatever",
      description: "Modding guides, scripting API, editor workflows, and the native IPC protocol for Whatever Engine.",
      logo: { src: "./src/assets/mark.svg" },
      favicon: "/favicon.svg",
      social: [
        { icon: "github", label: "GitHub", href: "https://github.com/MesterMan03/Whatever" },
      ],
      customCss: ["./src/styles/docs.css"],
      components: { SiteTitle: "./src/components/SiteTitle.astro" },
      sidebar: [
        { label: "Start here", items: [
          { slug: "getting-started/introduction" },
          { slug: "getting-started/installation" },
          { slug: "getting-started/first-mod" },
        ] },
        { label: "Build your mod", items: [
          { slug: "guides/editor" },
          { slug: "guides/components" },
          { slug: "guides/scenes-and-assets" },
          { slug: "guides/client-and-server" },
          { slug: "guides/debugging" },
          { slug: "guides/shipping" },
        ] },
        { label: "Scripting API", items: [{ autogenerate: { directory: "reference/api" } }] },
        { label: "Engine reference", items: [
          { slug: "reference/editor" },
          { slug: "reference/physics" },
          { slug: "reference/shaders" },
          { slug: "reference/sandbox" },
        ] },
        { label: "IPC protocol", items: [
          { slug: "protocol/specification", label: "Wire protocol" },
          { slug: "protocol/messages", label: "Message catalogue" },
        ] },
        { label: "Contributing to the docs", slug: "contributing" },
      ],
      expressiveCode: { themes: ["github-dark", "github-light"] },
      pagination: true,
      credits: false,
    }),
  ],
});
