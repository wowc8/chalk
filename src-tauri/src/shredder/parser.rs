//! Google Docs JSON parser — extracts table structures from the Documents API response.
//!
//! The Google Docs API returns a document with `body.content[]` containing
//! structural elements. Tables appear as elements with a `table` key containing
//! `tableRows`, each with `tableCells`, each containing `content` paragraphs.

/// A parsed table from a Google Doc.
#[derive(Debug, Clone)]
pub struct ParsedTable {
    pub rows: Vec<TableRow>,
}

/// A single row in a parsed table.
#[derive(Debug, Clone)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
}

/// A single cell in a table row, with extracted plain text.
#[derive(Debug, Clone)]
pub struct TableCell {
    pub text: String,
}

/// Extract all tables from a Google Docs API JSON document.
///
/// Walks `body.content[]` looking for elements with a `table` key,
/// then recursively extracts text from each cell's content.
pub fn extract_tables(doc_json: &serde_json::Value) -> Vec<ParsedTable> {
    let content = match doc_json
        .get("body")
        .and_then(|b| b.get("content"))
        .and_then(|c| c.as_array())
    {
        Some(c) => c,
        None => return Vec::new(),
    };

    let mut tables = Vec::new();

    for element in content {
        if let Some(table) = element.get("table") {
            if let Some(parsed) = parse_table(table) {
                tables.push(parsed);
            }
        }
    }

    tables
}

/// Parse a single `table` object into a `ParsedTable`.
fn parse_table(table: &serde_json::Value) -> Option<ParsedTable> {
    let table_rows = table.get("tableRows")?.as_array()?;

    let rows: Vec<TableRow> = table_rows
        .iter()
        .filter_map(|row| parse_table_row(row))
        .collect();

    if rows.is_empty() {
        return None;
    }

    Some(ParsedTable { rows })
}

/// Parse a single `tableRow` into a `TableRow`.
fn parse_table_row(row: &serde_json::Value) -> Option<TableRow> {
    let cells_json = row.get("tableCells")?.as_array()?;

    let cells: Vec<TableCell> = cells_json
        .iter()
        .map(|cell| {
            let text = extract_cell_text(cell);
            TableCell { text }
        })
        .collect();

    Some(TableRow { cells })
}

/// Recursively extract plain text from a table cell's content.
///
/// Handles paragraphs, text runs, and nested tables within cells.
fn extract_cell_text(cell: &serde_json::Value) -> String {
    let content = match cell.get("content").and_then(|c| c.as_array()) {
        Some(c) => c,
        None => return String::new(),
    };

    let mut parts: Vec<String> = Vec::new();

    for element in content {
        if let Some(paragraph) = element.get("paragraph") {
            let para_text = extract_paragraph_text(paragraph);
            if !para_text.is_empty() {
                parts.push(para_text);
            }
        }

        // Handle nested tables within cells.
        if let Some(nested_table) = element.get("table") {
            if let Some(parsed) = parse_table(nested_table) {
                for row in &parsed.rows {
                    let row_text: Vec<&str> = row
                        .cells
                        .iter()
                        .map(|c| c.text.trim())
                        .filter(|t| !t.is_empty())
                        .collect();
                    if !row_text.is_empty() {
                        parts.push(row_text.join(" | "));
                    }
                }
            }
        }
    }

    parts.join("\n")
}

/// Extract plain text from a paragraph element.
fn extract_paragraph_text(paragraph: &serde_json::Value) -> String {
    let elements = match paragraph.get("elements").and_then(|e| e.as_array()) {
        Some(e) => e,
        None => return String::new(),
    };

    let mut text = String::new();

    for element in elements {
        if let Some(text_run) = element.get("textRun") {
            if let Some(content) = text_run.get("content").and_then(|c| c.as_str()) {
                text.push_str(content);
            }
        }
    }

    // Trim trailing newline that Google Docs adds to each paragraph.
    text.trim_end_matches('\n').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_text_cell(text: &str) -> serde_json::Value {
        json!({
            "content": [{
                "paragraph": {
                    "elements": [{
                        "textRun": {"content": format!("{}\n", text)}
                    }]
                }
            }]
        })
    }

    fn make_table_row(cells: Vec<serde_json::Value>) -> serde_json::Value {
        json!({"tableCells": cells})
    }

    fn make_table(rows: Vec<serde_json::Value>) -> serde_json::Value {
        json!({
            "rows": rows.len(),
            "columns": if rows.is_empty() { 0 } else {
                rows[0].get("tableCells").and_then(|c| c.as_array()).map(|a| a.len()).unwrap_or(0)
            },
            "tableRows": rows
        })
    }

    fn make_doc(content: Vec<serde_json::Value>) -> serde_json::Value {
        json!({
            "title": "Test Document",
            "body": {"content": content}
        })
    }

    #[test]
    fn test_extract_tables_empty_doc() {
        let doc = make_doc(vec![]);
        assert!(extract_tables(&doc).is_empty());
    }

    #[test]
    fn test_extract_tables_no_body() {
        let doc = json!({"title": "No Body"});
        assert!(extract_tables(&doc).is_empty());
    }

    #[test]
    fn test_extract_tables_only_paragraphs() {
        let doc = make_doc(vec![json!({
            "paragraph": {
                "elements": [{"textRun": {"content": "Hello\n"}}]
            }
        })]);
        assert!(extract_tables(&doc).is_empty());
    }

    #[test]
    fn test_extract_single_table() {
        let table = make_table(vec![
            make_table_row(vec![make_text_cell("Header 1"), make_text_cell("Header 2")]),
            make_table_row(vec![make_text_cell("Value 1"), make_text_cell("Value 2")]),
        ]);
        let doc = make_doc(vec![json!({"table": table})]);

        let tables = extract_tables(&doc);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].rows.len(), 2);
        assert_eq!(tables[0].rows[0].cells[0].text, "Header 1");
        assert_eq!(tables[0].rows[1].cells[1].text, "Value 2");
    }

    #[test]
    fn test_extract_multiple_tables() {
        let table1 = make_table(vec![
            make_table_row(vec![make_text_cell("A")]),
            make_table_row(vec![make_text_cell("B")]),
        ]);
        let table2 = make_table(vec![
            make_table_row(vec![make_text_cell("C")]),
            make_table_row(vec![make_text_cell("D")]),
        ]);
        let doc = make_doc(vec![
            json!({"table": table1}),
            json!({"paragraph": {"elements": [{"textRun": {"content": "Separator\n"}}]}}),
            json!({"table": table2}),
        ]);

        let tables = extract_tables(&doc);
        assert_eq!(tables.len(), 2);
    }

    #[test]
    fn test_multi_paragraph_cell() {
        let cell = json!({
            "content": [
                {"paragraph": {"elements": [{"textRun": {"content": "Line 1\n"}}]}},
                {"paragraph": {"elements": [{"textRun": {"content": "Line 2\n"}}]}}
            ]
        });

        let text = extract_cell_text(&cell);
        assert_eq!(text, "Line 1\nLine 2");
    }

    #[test]
    fn test_empty_cell() {
        let cell = json!({"content": []});
        assert_eq!(extract_cell_text(&cell), "");
    }

    #[test]
    fn test_cell_no_content() {
        let cell = json!({});
        assert_eq!(extract_cell_text(&cell), "");
    }

    #[test]
    fn test_nested_table_in_cell() {
        let inner_table = make_table(vec![make_table_row(vec![
            make_text_cell("Nested A"),
            make_text_cell("Nested B"),
        ])]);

        let cell = json!({
            "content": [
                {"paragraph": {"elements": [{"textRun": {"content": "Intro\n"}}]}},
                {"table": inner_table}
            ]
        });

        let text = extract_cell_text(&cell);
        assert!(text.contains("Intro"));
        assert!(text.contains("Nested A"));
        assert!(text.contains("Nested B"));
    }

    #[test]
    fn test_paragraph_with_multiple_text_runs() {
        let paragraph = json!({
            "elements": [
                {"textRun": {"content": "Bold text"}},
                {"textRun": {"content": " and normal text\n"}}
            ]
        });

        let text = extract_paragraph_text(&paragraph);
        assert_eq!(text, "Bold text and normal text");
    }

    #[test]
    fn test_paragraph_no_elements() {
        let paragraph = json!({});
        assert_eq!(extract_paragraph_text(&paragraph), "");
    }

    #[test]
    fn test_table_with_empty_rows() {
        let table = make_table(vec![
            make_table_row(vec![make_text_cell("H1")]),
            make_table_row(vec![json!({"content": []})]),
            make_table_row(vec![make_text_cell("V1")]),
        ]);
        let doc = make_doc(vec![json!({"table": table})]);

        let tables = extract_tables(&doc);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].rows.len(), 3);
        assert_eq!(tables[0].rows[1].cells[0].text, "");
    }

    #[test]
    fn test_extract_tables_malformed_table() {
        let doc = make_doc(vec![json!({"table": {"rows": 0}})]);
        assert!(extract_tables(&doc).is_empty());
    }

    #[test]
    fn test_text_run_preserves_whitespace() {
        let paragraph = json!({
            "elements": [
                {"textRun": {"content": "  indented text  \n"}}
            ]
        });

        let text = extract_paragraph_text(&paragraph);
        assert_eq!(text, "  indented text  ");
    }
}
