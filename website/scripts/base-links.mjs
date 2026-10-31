// Markdown links are authored from the documentation root. Apply the configured
// deployment prefix once, for both Markdown and MDX, including generated docs.
export default function baseLinks(base = "/") {
  const prefix = base.replace(/\/$/, "");
  const rewrite = (node, context) => {
    if (node.url.startsWith("/") && !node.url.startsWith("//")) context.setProperty(node, "url", `${prefix}${node.url}`);
  };
  return { name: "docs-base-links", link: rewrite, image: rewrite, definition: rewrite };
}
