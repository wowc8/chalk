//! HTML table parser — extracts table structures from Google Drive HTML export.
//!
//! The Drive export API returns a full HTML document. Tables appear as `<table>`
//! elements with `<tr>` rows and `<td>`/`<th>` cells. We use the `scraper` crate
//! to parse the HTML and extract text content from each cell.

use scraper::{ElementRef, Html, Node, Selector};

/// A parsed table from an HTML document.
#[derive(Debug, Clone)]
pub struct ParsedTable {
    pub rows: Vec<TableRow>,
}

/// A single row in a parsed table.
#[derive(Debug, Clone)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
}

/// A single cell in a table row, with both plain text and inner HTML.
#[derive(Debug, Clone, Default)]
pub struct TableCell {
    /// Plain-text content (whitespace-collapsed), used for header matching
    /// and structural detection.
    pub text: String,
    /// Inner HTML of the cell, preserving formatting such as bold, italic,
    /// colors, lists, and hyperlinks.
    pub html: String,
    /// Background color extracted from the cell's inline style attribute.
    pub bg_color: Option<String>,
    /// Number of columns this cell spans (from the `colspan` attribute).
    /// Defaults to 1 when no colspan is specified.
    pub colspan: usize,
    /// Number of rows this cell spans (from the `rowspan` attribute).
    /// Defaults to 1 when no rowspan is specified.
    pub rowspan: usize,
}

impl ParsedTable {
    /// Compute the true grid width of this table by summing colspan values.
    ///
    /// Returns the maximum effective column count across all rows. This
    /// correctly handles merged cells: a row with 1 cell that has `colspan="6"`
    /// contributes a width of 6, not 1.
    pub fn grid_width(&self) -> usize {
        self.rows
            .iter()
            .map(|row| row.cells.iter().map(|c| c.colspan).sum::<usize>())
            .max()
            .unwrap_or(0)
    }
}

impl TableRow {
    /// Compute the effective column count for this row by summing colspan values.
    pub fn effective_width(&self) -> usize {
        self.cells.iter().map(|c| c.colspan).sum()
    }
}

/// Extract all tables from an HTML document exported by Google Drive.
///
/// Finds every `<table>` element — including nested tables — and extracts
/// rows and cell text. Google Docs exports often wrap the entire document
/// in a layout table, with the actual schedule table nested inside a cell.
/// We extract both the outer and inner tables so the scoring/AI logic can
/// select the correct one.
pub fn extract_tables(html: &str) -> Vec<ParsedTable> {
    let document = Html::parse_document(html);
    let table_sel = Selector::parse("table").expect("valid selector");
    let tr_sel = Selector::parse("tr").expect("valid selector");
    let cell_sel = Selector::parse("td, th").expect("valid selector");

    let mut tables = Vec::new();

    for table_el in document.select(&table_sel) {
        let table_node_id = table_el.id();
        let mut rows = Vec::new();

        for tr in table_el.select(&tr_sel) {
            // Skip <tr> elements that belong to a nested/different table.
            let belongs_to = tr
                .ancestors()
                .filter_map(|a| {
                    a.value().as_element().and_then(|e| {
                        if e.name() == "table" {
                            Some(a.id())
                        } else {
                            None
                        }
                    })
                })
                .next();
            if belongs_to != Some(table_node_id) {
                continue;
            }

            let cells: Vec<TableCell> = tr
                .select(&cell_sel)
                .filter(|cell| {
                    // Skip cells that belong to a nested table inside this row.
                    cell.ancestors()
                        .filter_map(|a| {
                            a.value().as_element().and_then(|e| {
                                if e.name() == "tr" {
                                    Some(a.id())
                                } else {
                                    None
                                }
                            })
                        })
                        .next() == Some(tr.id())
                })
                .map(|cell| {
                    let text = cell_text(&cell);
                    let html = cell_inner_html(&cell);
                    let bg_color = cell.value().attr("style")
                        .and_then(extract_bg_color_from_style);
                    let colspan = cell.value().attr("colspan")
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(1)
                        .max(1);
                    let rowspan = cell.value().attr("rowspan")
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(1)
                        .max(1);
                    TableCell { text, html, bg_color, colspan, rowspan }
                })
                .collect();

            if !cells.is_empty() {
                rows.push(TableRow { cells });
            }
        }

        if !rows.is_empty() {
            tables.push(ParsedTable { rows });
        }
    }

    tables
}

/// Extract a background-color value from a CSS style string.
fn extract_bg_color_from_style(style: &str) -> Option<String> {
    let lower = style.to_lowercase();
    for prefix in &["background-color:", "background:"] {
        if let Some(pos) = lower.find(prefix) {
            let after = &lower[pos + prefix.len()..];
            let color = after
                .trim()
                .split(';')
                .next()?
                .split_whitespace()
                .next()?
                .trim()
                .to_string();
            if color == "transparent" || color == "inherit" || color == "none" || color == "initial" {
                return None;
            }
            return Some(color);
        }
    }
    None
}

/// Block-level element names that should introduce whitespace boundaries.
const BLOCK_ELEMENTS: &[&str] = &[
    "p", "div", "br", "li", "h1", "h2", "h3", "h4", "h5", "h6",
    "blockquote", "pre", "hr", "table", "tr", "td", "th",
];

/// Extract the plain-text content of an HTML element, inserting spaces at
/// block-element boundaries and collapsing whitespace.
fn cell_text(element: &scraper::ElementRef) -> String {
    let mut parts = Vec::new();
    collect_text(element, &mut parts);
    let raw = parts.join("");
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extract the inner HTML of an element, cleaning up Google Docs export
/// artifacts (e.g. class attributes) while preserving semantic formatting.
fn cell_inner_html(element: &scraper::ElementRef) -> String {
    let raw = element.inner_html();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed.to_string()
}

/// Recursively collect text, inserting a space before block-level elements.
fn collect_text(element: &ElementRef, out: &mut Vec<String>) {
    for child in element.children() {
        match child.value() {
            Node::Text(text) => {
                out.push(text.to_string());
            }
            Node::Element(el) => {
                if BLOCK_ELEMENTS.contains(&el.name()) {
                    out.push(" ".to_string());
                }
                if let Some(child_ref) = ElementRef::wrap(child) {
                    collect_text(&child_ref, out);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tables_empty_html() {
        let tables = extract_tables("");
        assert!(tables.is_empty());
    }

    #[test]
    fn test_extract_tables_no_tables() {
        let html = "<html><body><p>Hello world</p></body></html>";
        assert!(extract_tables(html).is_empty());
    }

    #[test]
    fn test_extract_single_table() {
        let html = r#"
            <html><body>
            <table>
                <tr><th>Header 1</th><th>Header 2</th></tr>
                <tr><td>Value 1</td><td>Value 2</td></tr>
            </table>
            </body></html>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].rows.len(), 2);
        assert_eq!(tables[0].rows[0].cells[0].text, "Header 1");
        assert_eq!(tables[0].rows[0].cells[1].text, "Header 2");
        assert_eq!(tables[0].rows[1].cells[0].text, "Value 1");
        assert_eq!(tables[0].rows[1].cells[1].text, "Value 2");
    }

    #[test]
    fn test_extract_multiple_tables() {
        let html = r#"
            <html><body>
            <table>
                <tr><td>A</td></tr>
                <tr><td>B</td></tr>
            </table>
            <p>Separator</p>
            <table>
                <tr><td>C</td></tr>
                <tr><td>D</td></tr>
            </table>
            </body></html>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables.len(), 2);
    }

    #[test]
    fn test_whitespace_collapsing() {
        let html = r#"
            <table><tr><td>  lots   of   spaces  </td></tr></table>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables[0].rows[0].cells[0].text, "lots of spaces");
    }

    #[test]
    fn test_multi_paragraph_cell() {
        let html = r#"
            <table><tr><td><p>Line 1</p><p>Line 2</p></td></tr></table>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables[0].rows[0].cells[0].text, "Line 1 Line 2");
    }

    #[test]
    fn test_empty_table_skipped() {
        let html = r#"<table></table>"#;
        assert!(extract_tables(html).is_empty());
    }

    #[test]
    fn test_nested_table_extracted_separately() {
        let html = r#"
            <table>
                <tr><td>
                    Outer
                    <table><tr><td>Inner</td></tr></table>
                </td></tr>
            </table>
        "#;
        let tables = extract_tables(html);
        // Both the outer and inner table should be extracted separately.
        assert_eq!(tables.len(), 2);
        // The outer table has 1 row with 1 cell.
        assert_eq!(tables[0].rows.len(), 1);
        // The inner table has 1 row with 1 cell containing "Inner".
        assert_eq!(tables[1].rows.len(), 1);
        assert_eq!(tables[1].rows[0].cells[0].text, "Inner");
    }

    #[test]
    fn test_th_and_td_both_parsed() {
        let html = r#"
            <table>
                <tr><th>H1</th><th>H2</th></tr>
                <tr><td>V1</td><td>V2</td></tr>
            </table>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables[0].rows[0].cells[0].text, "H1");
        assert_eq!(tables[0].rows[1].cells[0].text, "V1");
    }

    #[test]
    fn test_empty_cells() {
        let html = r#"
            <table><tr><td></td><td>Data</td></tr></table>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables[0].rows[0].cells[0].text, "");
        assert_eq!(tables[0].rows[0].cells[1].text, "Data");
    }

    #[test]
    fn test_styled_text_stripped() {
        let html = r#"
            <table><tr><td><span style="font-weight:bold">Bold</span> and <em>italic</em></td></tr></table>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables[0].rows[0].cells[0].text, "Bold and italic");
    }

    #[test]
    fn test_html_field_preserves_formatting() {
        let html = r#"
            <table><tr><td><span style="font-weight:bold">Bold</span> and <em>italic</em></td></tr></table>
        "#;
        let tables = extract_tables(html);
        let cell_html = &tables[0].rows[0].cells[0].html;
        assert!(cell_html.contains("<span"), "HTML should preserve span tags");
        assert!(cell_html.contains("<em>"), "HTML should preserve em tags");
        assert!(cell_html.contains("Bold"), "HTML should contain text");
    }

    #[test]
    fn test_html_field_preserves_links() {
        let html = r#"
            <table><tr><td><a href="https://example.com">Click here</a></td></tr></table>
        "#;
        let tables = extract_tables(html);
        let cell_html = &tables[0].rows[0].cells[0].html;
        assert!(cell_html.contains("<a "), "HTML should preserve anchor tags");
        assert!(cell_html.contains("href"), "HTML should preserve href attributes");
        assert_eq!(tables[0].rows[0].cells[0].text, "Click here");
    }

    #[test]
    fn test_html_field_preserves_lists() {
        let html = r#"
            <table><tr><td><ul><li>Item 1</li><li>Item 2</li></ul></td></tr></table>
        "#;
        let tables = extract_tables(html);
        let cell_html = &tables[0].rows[0].cells[0].html;
        assert!(cell_html.contains("<ul>"), "HTML should preserve ul tags");
        assert!(cell_html.contains("<li>"), "HTML should preserve li tags");
    }

    #[test]
    fn test_html_field_empty_cell() {
        let html = r#"
            <table><tr><td></td></tr></table>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables[0].rows[0].cells[0].html, "");
    }

    // ── Colspan / Rowspan Tests ─────────────────────────────────────

    #[test]
    fn test_colspan_parsed() {
        let html = r#"
            <table>
                <tr><td colspan="3">Wide</td></tr>
                <tr><td>A</td><td>B</td><td>C</td></tr>
            </table>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables[0].rows[0].cells[0].colspan, 3);
        assert_eq!(tables[0].rows[0].cells[0].rowspan, 1);
        assert_eq!(tables[0].rows[1].cells[0].colspan, 1);
    }

    #[test]
    fn test_rowspan_parsed() {
        let html = r#"
            <table>
                <tr><td rowspan="2">Tall</td><td>B</td></tr>
                <tr><td>C</td></tr>
            </table>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables[0].rows[0].cells[0].rowspan, 2);
        assert_eq!(tables[0].rows[0].cells[0].colspan, 1);
    }

    #[test]
    fn test_colspan_and_rowspan_combined() {
        let html = r#"
            <table>
                <tr><td colspan="2" rowspan="2">Big</td><td>C</td></tr>
                <tr><td>D</td></tr>
            </table>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables[0].rows[0].cells[0].colspan, 2);
        assert_eq!(tables[0].rows[0].cells[0].rowspan, 2);
    }

    #[test]
    fn test_default_colspan_rowspan() {
        let html = r#"
            <table><tr><td>Normal</td></tr></table>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables[0].rows[0].cells[0].colspan, 1);
        assert_eq!(tables[0].rows[0].cells[0].rowspan, 1);
    }

    #[test]
    fn test_grid_width_with_colspan() {
        let html = r#"
            <table>
                <tr><td colspan="5">Title</td></tr>
                <tr><td>A</td><td>B</td><td>C</td><td>D</td><td>E</td></tr>
            </table>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables[0].grid_width(), 5);
    }

    #[test]
    fn test_effective_width() {
        let html = r#"
            <table>
                <tr><td colspan="3">Wide</td><td>Normal</td></tr>
            </table>
        "#;
        let tables = extract_tables(html);
        assert_eq!(tables[0].rows[0].effective_width(), 4);
    }
}
