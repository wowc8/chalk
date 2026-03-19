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
#[derive(Debug, Clone)]
pub struct TableCell {
    /// Plain-text content (whitespace-collapsed), used for header matching
    /// and structural detection.
    pub text: String,
    /// Inner HTML of the cell, preserving formatting such as bold, italic,
    /// colors, lists, and hyperlinks.
    pub html: String,
}

/// Extract all tables from an HTML document exported by Google Drive.
///
/// Finds every `<table>` element and extracts rows and cell text.
pub fn extract_tables(html: &str) -> Vec<ParsedTable> {
    let document = Html::parse_document(html);
    let table_sel = Selector::parse("table").expect("valid selector");
    let tr_sel = Selector::parse("tr").expect("valid selector");
    let cell_sel = Selector::parse("td, th").expect("valid selector");

    let mut tables = Vec::new();

    for table_el in document.select(&table_sel) {
        // Skip tables nested inside another table — they will be reached
        // when we process the outer table's cell text recursively.
        if table_el
            .ancestors()
            .any(|ancestor| {
                ancestor
                    .value()
                    .as_element()
                    .map_or(false, |e| e.name() == "table")
            })
        {
            continue;
        }

        let table_node_id = table_el.id();
        let mut rows = Vec::new();

        for tr in table_el.select(&tr_sel) {
            // Skip <tr> elements that belong to a nested table.
            let belongs_to_outer = tr
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
            if belongs_to_outer != Some(table_node_id) {
                continue;
            }

            let cells: Vec<TableCell> = tr
                .select(&cell_sel)
                .map(|cell| {
                    let text = cell_text(&cell);
                    let html = cell_inner_html(&cell);
                    TableCell { text, html }
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
    fn test_nested_table_not_double_counted() {
        let html = r#"
            <table>
                <tr><td>
                    Outer
                    <table><tr><td>Inner</td></tr></table>
                </td></tr>
            </table>
        "#;
        let tables = extract_tables(html);
        // Only the outer table should be counted as a top-level table.
        assert_eq!(tables.len(), 1);
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
}
