import TableHeader from "@tiptap/extension-table-header";

/**
 * Custom TableHeader extension that preserves `background-color` on `<th>` elements.
 *
 * Same rationale as CustomTableCell — TipTap's default strips inline styles.
 * This ensures header cell colors from the teacher's Google Doc are preserved.
 */
export const CustomTableHeader = TableHeader.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      backgroundColor: {
        default: null,
        parseHTML: (element: HTMLElement) => element.style.backgroundColor || null,
        renderHTML: (attributes: Record<string, unknown>) => {
          if (!attributes.backgroundColor) {
            return {};
          }
          const color = attributes.textColor ? `; color: ${attributes.textColor}` : '';
          return {
            style: `background-color: ${attributes.backgroundColor}${color}`,
          };
        },
      },
      textColor: {
        default: null,
        parseHTML: (element: HTMLElement) => element.style.color || null,
        renderHTML: (attributes: Record<string, unknown>) => {
          // Rendered via backgroundColor's renderHTML to keep a single style attr
          if (!attributes.textColor || attributes.backgroundColor) {
            return {};
          }
          return {
            style: `color: ${attributes.textColor}`,
          };
        },
      },
    };
  },
});
