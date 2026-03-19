import { marked } from "marked";

// Configure marked for safe, synchronous rendering.
marked.setOptions({
  async: false,
  gfm: true,
  breaks: true,
});

/**
 * Pre-process AI responses to prevent HTML blocks from being treated as code.
 *
 * Two common failure modes:
 * 1. The AI indents HTML with 4+ spaces, which marked treats as a code block.
 * 2. The AI wraps HTML in a code fence (```html ... ```).
 *
 * This function normalises both cases so marked renders the HTML correctly.
 */
function preprocessHtmlBlocks(md: string): string {
  // Strip HTML out of code fences (```html ... ``` or ``` ... ```)
  let result = md.replace(
    /```(?:html)?\s*\n([\s\S]*?)```/g,
    (_match, inner: string) => {
      // Only unwrap if the fenced content looks like HTML
      if (/<[a-z][\s\S]*>/i.test(inner)) {
        return inner.trim();
      }
      return _match; // Leave non-HTML code fences alone
    }
  );

  // Remove leading indentation from lines that start an HTML block element.
  // This prevents marked from treating indented HTML as a code block.
  const htmlBlockTags =
    /^[ \t]{4,}(<\/?(?:table|thead|tbody|tfoot|tr|th|td|div|ul|ol|li|p|h[1-6]|blockquote|pre|hr|br|b|strong|em|i|u|a|span|section|article|header|footer)[\s>])/gim;
  result = result.replace(htmlBlockTags, "$1");

  return result;
}

/** Convert a markdown string to HTML. Returns the input unchanged if conversion fails. */
export function markdownToHtml(md: string): string {
  try {
    const preprocessed = preprocessHtmlBlocks(md);
    return marked.parse(preprocessed) as string;
  } catch {
    return md;
  }
}
