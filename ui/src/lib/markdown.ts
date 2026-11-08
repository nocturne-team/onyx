import DOMPurify from "dompurify";
import { marked } from "marked";

/**
 * Project descriptions are Markdown (with some HTML) written by anyone, and
 * this webview can call Tauri commands, so everything goes through
 * DOMPurify. No iframes, no inline handlers, no `javascript:` links.
 */
export function renderMarkdown(markdown: string): string {
  const html = marked.parse(markdown, { async: false, gfm: true, breaks: false });
  return DOMPurify.sanitize(html, {
    FORBID_TAGS: ["style", "iframe", "form", "input", "button", "textarea", "select"],
    FORBID_ATTR: ["style"],
  });
}
