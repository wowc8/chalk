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

// ── Editor-update markers ──────────────────────────────────

const EDITOR_OPEN = "<<<EDITOR_UPDATE>>>";
const EDITOR_CLOSE = "<<<END_EDITOR_UPDATE>>>";

export interface ParsedAiResponse {
  /** Content to display in the chat bubble (markdown string). */
  chatContent: string;
  /** HTML to write directly into the TipTap editor, or null if no editor update. */
  editorHtml: string | null;
}

/**
 * Split an AI response into chat-visible text and editor-bound HTML.
 *
 * The AI is instructed to wrap any editor content with:
 *   <<<EDITOR_UPDATE>>> ... <<<END_EDITOR_UPDATE>>>
 *
 * Everything outside those markers is treated as the chat summary.
 */
export function parseAiResponse(raw: string): ParsedAiResponse {
  const openIdx = raw.indexOf(EDITOR_OPEN);
  const closeIdx = raw.indexOf(EDITOR_CLOSE);

  if (openIdx === -1 || closeIdx === -1 || closeIdx <= openIdx) {
    // No valid markers — treat entire response as chat content.
    return { chatContent: raw, editorHtml: null };
  }

  const editorHtml = raw
    .slice(openIdx + EDITOR_OPEN.length, closeIdx)
    .trim();

  // Chat = everything before the opening marker + everything after the closing marker.
  const before = raw.slice(0, openIdx).trim();
  const after = raw.slice(closeIdx + EDITOR_CLOSE.length).trim();
  const chatContent = [before, after].filter(Boolean).join("\n\n");

  return {
    chatContent: chatContent || "(Updated the lesson plan in the editor.)",
    editorHtml: editorHtml || null,
  };
}

/**
 * Strip editor-update markers from a streaming string so the chat bubble
 * only shows the conversational portion while streaming is in progress.
 */
export function stripEditorMarkers(streaming: string): string {
  // If we haven't seen the open marker yet, everything is chat content.
  const openIdx = streaming.indexOf(EDITOR_OPEN);
  if (openIdx === -1) return streaming;

  const before = streaming.slice(0, openIdx).trim();

  // If the close marker hasn't arrived yet, just show what's before the open marker.
  const closeIdx = streaming.indexOf(EDITOR_CLOSE);
  if (closeIdx === -1) return before || "Writing to editor...";

  // Both markers present — show chat portions only.
  const after = streaming.slice(closeIdx + EDITOR_CLOSE.length).trim();
  return [before, after].filter(Boolean).join("\n\n") || "Writing to editor...";
}
