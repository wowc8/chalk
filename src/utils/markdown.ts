import { marked } from "marked";

// Configure marked for safe, synchronous rendering.
marked.setOptions({
  async: false,
  gfm: true,
  breaks: true,
});

/** Convert a markdown string to HTML. Returns the input unchanged if conversion fails. */
export function markdownToHtml(md: string): string {
  try {
    return marked.parse(md) as string;
  } catch {
    return md;
  }
}
