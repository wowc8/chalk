import TableCell from "@tiptap/extension-table-cell";

/**
 * Custom TableCell extension that preserves `background-color` on `<td>` elements.
 *
 * TipTap's default TableCell strips inline styles. This extension adds a
 * `backgroundColor` attribute that is parsed from and rendered as
 * `style="background-color: ..."`, so cell colors from Google Docs (and
 * AI-generated plans) survive the HTML round-trip.
 */
export const CustomTableCell = TableCell.extend({
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
          return {
            style: `background-color: ${attributes.backgroundColor}`,
          };
        },
      },
    };
  },
});
