import { describe, it, expect } from "vitest";
import { stripEditorMarkers, parseAiResponse } from "./markdown";

// ── stripEditorMarkers ───────────────────────────────────────

describe("stripEditorMarkers", () => {
  const OPEN = "<<<EDITOR_UPDATE>>>";
  const CLOSE = "<<<END_EDITOR_UPDATE>>>";

  it("returns plain text unchanged when no markers present", () => {
    const result = stripEditorMarkers("Hello, world!");
    expect(result).toEqual({ chat: "Hello, world!", isEditorStreaming: false });
  });

  it("detects partial open marker at the tail and trims it", () => {
    const result = stripEditorMarkers("Hello<<<EDITOR");
    expect(result.chat).toBe("Hello");
    // Partial marker is trimmed; isEditorStreaming false because marker isn't confirmed
    expect(result.isEditorStreaming).toBe(false);
  });

  it("detects very short partial marker (<<<)", () => {
    const result = stripEditorMarkers("Chat text<<<");
    expect(result.chat).toBe("Chat text");
    expect(result.isEditorStreaming).toBe(false);
  });

  it("handles full open marker with no close marker (editor streaming)", () => {
    const content = `Some chat text\n\n${OPEN}\n<table><tr><td>data</td></tr></table>`;
    const result = stripEditorMarkers(content);
    expect(result.chat).toBe("Some chat text");
    expect(result.isEditorStreaming).toBe(true);
  });

  it("handles both markers — returns only chat portions", () => {
    const content = `Before text\n\n${OPEN}\n<table></table>\n${CLOSE}\n\nAfter text`;
    const result = stripEditorMarkers(content);
    expect(result.chat).toBe("Before text\n\nAfter text");
    expect(result.isEditorStreaming).toBe(false);
  });

  it("handles both markers with no chat text", () => {
    const content = `${OPEN}\n<table></table>\n${CLOSE}`;
    const result = stripEditorMarkers(content);
    expect(result.chat).toBe("");
    expect(result.isEditorStreaming).toBe(false);
  });

  it("detects partial close marker after full block", () => {
    const content = `Chat\n${OPEN}\n<table></table>\n${CLOSE}\nMore text<<<END`;
    const result = stripEditorMarkers(content);
    expect(result.chat).toBe("Chat\n\nMore text");
    expect(result.isEditorStreaming).toBe(false);
  });

  // ── HTML leak detection (new behavior) ──────────────────────

  it("detects table HTML outside markers and strips it", () => {
    const content = "Here's a plan:\n<table><tr><td>Art</td></tr></table>";
    const result = stripEditorMarkers(content);
    expect(result.chat).toBe("Here's a plan:");
    expect(result.isEditorStreaming).toBe(true);
  });

  it("detects orphan table tags during streaming (no closing </table>)", () => {
    const content = "Check this out:\n<table><tr><td>10:00</td><td>Art Project";
    const result = stripEditorMarkers(content);
    expect(result.chat).not.toContain("<td>");
    expect(result.chat).not.toContain("<tr>");
    expect(result.chat).not.toContain("<table>");
    expect(result.isEditorStreaming).toBe(true);
  });

  it("strips HTML from pre-marker chat text", () => {
    const content = `Some text\n<table><tr><td>oops</td></tr></table>\n${OPEN}\n<table></table>`;
    const result = stripEditorMarkers(content);
    expect(result.chat).not.toContain("<table>");
    expect(result.chat).not.toContain("<td>");
    expect(result.isEditorStreaming).toBe(true);
  });

  it("strips HTML from post-marker chat text", () => {
    const content = `${OPEN}\n<table></table>\n${CLOSE}\nSummary\n<table><tr><td>leak</td></tr></table>`;
    const result = stripEditorMarkers(content);
    expect(result.chat).not.toContain("<table>");
    expect(result.chat).not.toContain("<td>");
  });

  it("handles AI output that is pure HTML with no markers", () => {
    const content =
      "<table><tr><th>Time</th><th>Monday</th></tr><tr><td>10:00</td><td>Art Project</td></tr></table>";
    const result = stripEditorMarkers(content);
    expect(result.chat).toBe("");
    expect(result.isEditorStreaming).toBe(true);
  });

  it("does not false-positive on normal markdown chat text", () => {
    const result = stripEditorMarkers(
      "I recommend adding a 10-minute warm-up. Here are some ideas:\n- Drawing exercise\n- Quick sketch"
    );
    expect(result.chat).toContain("I recommend");
    expect(result.isEditorStreaming).toBe(false);
  });
});

// ── parseAiResponse ──────────────────────────────────────────

describe("parseAiResponse", () => {
  const OPEN = "<<<EDITOR_UPDATE>>>";
  const CLOSE = "<<<END_EDITOR_UPDATE>>>";

  it("returns full content as chat when no markers", () => {
    const result = parseAiResponse("Just some chat text");
    expect(result.chatContent).toBe("Just some chat text");
    expect(result.editorHtml).toBeNull();
  });

  it("extracts editor HTML and chat content", () => {
    const raw = `Chat summary\n\n${OPEN}\n<table><tr><td>data</td></tr></table>\n${CLOSE}`;
    const result = parseAiResponse(raw);
    expect(result.chatContent).toBe("Chat summary");
    expect(result.editorHtml).toContain("<table>");
  });

  it("provides default chat content when only editor content present", () => {
    const raw = `${OPEN}\n<table></table>\n${CLOSE}`;
    const result = parseAiResponse(raw);
    expect(result.chatContent).toBe("(Updated the lesson plan in the editor.)");
    expect(result.editorHtml).toContain("<table>");
  });

  it("handles markers with content before and after", () => {
    const raw = `Before\n${OPEN}\n<p>html</p>\n${CLOSE}\nAfter`;
    const result = parseAiResponse(raw);
    expect(result.chatContent).toBe("Before\n\nAfter");
    expect(result.editorHtml).toBe("<p>html</p>");
  });
});
