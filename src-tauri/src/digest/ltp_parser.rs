//! LTP (Long-Term Plan) HTML parser.
//!
//! Parses Google Sheets HTML exports of Long-Term Plan documents into
//! structured grid data. Uses the existing [`resolve_grid()`] algorithm
//! to handle colspan/rowspan, then maps the resolved grid into
//! [`LtpGridCell`] records for database storage.
//!
//! The parser handles Google Sheets export quirks:
//! - `<th class="row-headers-background">` for row-number columns (skipped)
//! - `<th class="freezebar-cell">` for frozen column separators (skipped)
//! - CSS classes (`.s0`, `.s1`, ...) that encode background colors
//! - Column header rows (A, B, C...) that should be skipped

use std::collections::HashMap;

use scraper::{Html, Selector};
use tracing::info;

use super::parser::{self, TableCell};
use super::template_extractor::resolve_grid;

/// Month names used to detect the header row.
const MONTH_NAMES: &[&str] = &[
    "august",
    "september",
    "october",
    "november",
    "december",
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
];

/// A parsed LTP grid cell ready for database insertion.
#[derive(Debug, Clone)]
pub struct ParsedLtpCell {
    pub row_index: i32,
    pub col_index: i32,
    pub subject: Option<String>,
    pub month: Option<String>,
    pub content_html: Option<String>,
    pub content_text: Option<String>,
    pub background_color: Option<String>,
    pub unit_name: Option<String>,
    pub unit_color: Option<String>,
}

/// Unit span info extracted from the units row.
#[derive(Debug, Clone)]
struct UnitSpan {
    name: String,
    color: Option<String>,
    /// Grid column range [start, end) that this unit covers.
    start_col: usize,
    end_col: usize,
}

/// Result of parsing an LTP HTML document.
#[derive(Debug)]
pub struct LtpParseResult {
    pub cells: Vec<ParsedLtpCell>,
    pub month_headers: Vec<String>,
    pub subject_labels: Vec<String>,
}

/// Extract CSS class → background-color mappings from a `<style>` block.
///
/// Google Sheets HTML exports define cell styles using classes like `.ritz .waffle .s0`,
/// `.ritz .waffle .s1`, etc. This function parses those definitions to extract
/// background colors.
fn extract_css_colors(html: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    let document = Html::parse_document(html);
    let style_sel = Selector::parse("style").expect("valid selector");

    for style_el in document.select(&style_sel) {
        let css = style_el.inner_html();
        // Parse rules like: .ritz .waffle .s3{...background-color:#ff9900...}
        for rule in css.split('}') {
            // Find the class name (e.g., "s3")
            let class_name = rule
                .rsplit('.')
                .next()
                .and_then(|s| s.split('{').next())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && s.starts_with('s') && s[1..].chars().all(|c| c.is_ascii_digit()));

            if let Some(class_name) = class_name {
                // Find background-color
                if let Some(color) = extract_bg_color_from_css_rule(rule) {
                    if color != "#ffffff" && color != "#fff" {
                        map.insert(class_name.to_string(), color);
                    }
                }
            }
        }
    }

    map
}

/// Extract a background-color from a CSS rule body.
fn extract_bg_color_from_css_rule(rule: &str) -> Option<String> {
    let lower = rule.to_lowercase();
    let prefix = "background-color:";
    let pos = lower.find(prefix)?;
    let after = &lower[pos + prefix.len()..];
    let color = after.trim().split(';').next()?.split_whitespace().next()?.trim().to_string();
    if color == "transparent" || color == "inherit" || color == "none" || color == "initial" {
        return None;
    }
    Some(color)
}


/// Check if a row contains month names (the month header row).
fn count_month_names(texts: &[&str]) -> usize {
    texts
        .iter()
        .filter(|t| {
            let lower = t.trim().to_lowercase();
            MONTH_NAMES.iter().any(|m| lower == *m)
        })
        .count()
}

/// Identify which row is the month header row.
///
/// Returns the row index with the most month-name matches (minimum 3).
fn find_month_header_row(grid: &[Vec<Option<&TableCell>>], width: usize) -> Option<usize> {
    let mut best = (0usize, 0usize); // (row_idx, count)
    for (row_idx, row) in grid.iter().enumerate() {
        let texts: Vec<&str> = row
            .iter()
            .take(width)
            .map(|c| c.map(|cell| cell.text.trim()).unwrap_or(""))
            .collect();
        let count = count_month_names(&texts);
        if count > best.1 {
            best = (row_idx, count);
        }
    }
    if best.1 >= 3 {
        Some(best.0)
    } else {
        None
    }
}

/// Find the first content column (skipping row-number headers and freezebar).
///
/// Google Sheets HTML exports have:
/// - Column 0: often a `<th>` row-number column
/// - Column 1: sometimes a freezebar-cell separator
/// - Column 2+: actual content
///
/// We detect the subject column as the first column where any data row
/// (after the month header) has non-empty non-numeric text.
fn find_subject_column(
    grid: &[Vec<Option<&TableCell>>],
    width: usize,
    month_row: usize,
) -> usize {
    // Try each column starting from 0.
    for col in 0..width.min(4) {
        let mut has_subject_text = false;
        for row_idx in (month_row + 1)..grid.len() {
            if let Some(Some(cell)) = grid.get(row_idx).and_then(|r| r.get(col)) {
                let text = cell.text.trim();
                // Subject labels are non-empty text that isn't just a row number.
                if !text.is_empty() && !text.chars().all(|c| c.is_ascii_digit()) {
                    has_subject_text = true;
                    break;
                }
            }
        }
        if has_subject_text {
            return col;
        }
    }
    // Default to column 0 if nothing found.
    0
}

/// Find the first data column (the column after the subject column, or the
/// first column where month headers appear).
fn find_first_data_column(
    grid: &[Vec<Option<&TableCell>>],
    width: usize,
    month_row: usize,
    subject_col: usize,
) -> usize {
    // The first data column is the first column after subject_col where the
    // month header row has a month name.
    for col in (subject_col + 1)..width {
        if let Some(Some(cell)) = grid.get(month_row).and_then(|r| r.get(col)) {
            let lower = cell.text.trim().to_lowercase();
            if MONTH_NAMES.iter().any(|m| lower == *m) {
                return col;
            }
        }
    }
    subject_col + 1
}

/// Build month header labels for each grid column.
///
/// Because month headers use colspan (e.g., October spans 2 columns), this
/// function maps each grid column to its month name. The resolved grid
/// already duplicates the cell reference across spanned columns.
fn build_month_map(
    grid: &[Vec<Option<&TableCell>>],
    month_row: usize,
    first_data_col: usize,
    width: usize,
) -> Vec<Option<String>> {
    let mut months: Vec<Option<String>> = vec![None; width];
    for col in first_data_col..width {
        if let Some(Some(cell)) = grid.get(month_row).and_then(|r| r.get(col)) {
            let text = cell.text.trim();
            let lower = text.to_lowercase();
            if MONTH_NAMES.iter().any(|m| lower == *m) {
                months[col] = Some(text.to_string());
            }
        }
    }
    months
}


/// Parse an LTP HTML document and extract structured grid cells.
///
/// This is the main entry point for LTP parsing. It:
/// 1. Extracts CSS class→color mappings from the `<style>` block
/// 2. Parses the HTML tables using the existing parser
/// 3. Finds the main "waffle" table (Google Sheets export)
/// 4. Runs `resolve_grid()` to handle colspan/rowspan
/// 5. Identifies the month header row, subject column, and unit row
/// 6. Extracts each content cell with subject, month, color, and unit info
pub fn parse_ltp_html(html: &str) -> LtpParseResult {
    let css_colors = extract_css_colors(html);
    let cell_class_colors = extract_css_bg_colors_for_cells(html, &css_colors);

    let tables = parser::extract_tables(html);
    if tables.is_empty() {
        return LtpParseResult {
            cells: Vec::new(),
            month_headers: Vec::new(),
            subject_labels: Vec::new(),
        };
    }

    // Find the main table — in Google Sheets exports, it's the table with
    // class "waffle" and the most rows. For our purpose, pick the largest table.
    let table = tables
        .iter()
        .max_by_key(|t| t.rows.len())
        .expect("tables is not empty");

    let resolved = resolve_grid(table);
    if resolved.height < 3 || resolved.width < 3 {
        return LtpParseResult {
            cells: Vec::new(),
            month_headers: Vec::new(),
            subject_labels: Vec::new(),
        };
    }

    // Find the month header row.
    let month_row = match find_month_header_row(&resolved.grid, resolved.width) {
        Some(r) => r,
        None => {
            info!("No month header row found in LTP document");
            return LtpParseResult {
                cells: Vec::new(),
                month_headers: Vec::new(),
                subject_labels: Vec::new(),
            };
        }
    };

    let subject_col = find_subject_column(&resolved.grid, resolved.width, month_row);
    let first_data_col = find_first_data_column(&resolved.grid, resolved.width, month_row, subject_col);

    // Build month labels for each column.
    let month_map = build_month_map(&resolved.grid, month_row, first_data_col, resolved.width);
    // Deduplicated month list preserving order (some months span multiple columns).
    let mut month_headers: Vec<String> = Vec::new();
    for m in month_map.iter().flatten() {
        if !month_headers.contains(m) {
            month_headers.push(m.clone());
        }
    }

    // Find the units row — typically the row right after "Instructional Days:" row,
    // or the first row below the month header that has colored cells with unit text.
    let unit_row = find_unit_row(&resolved.grid, resolved.width, month_row, first_data_col, &cell_class_colors);

    // Extract unit spans for column→unit mapping.
    let unit_spans = if let Some(ur) = unit_row {
        extract_unit_spans_from_grid(&resolved.grid, ur, first_data_col, resolved.width, &cell_class_colors)
    } else {
        Vec::new()
    };

    // First data row is the row after the last structural row.
    let first_content_row = match unit_row {
        Some(ur) => ur + 1,
        None => month_row + 1,
    };

    // Extract cells.
    let mut cells = Vec::new();
    let mut subject_labels = Vec::new();

    for row_idx in first_content_row..resolved.height {
        // Get subject label from the subject column.
        let subject = resolved
            .cell_at(row_idx, subject_col)
            .map(|c| c.text.trim().to_string())
            .filter(|s| !s.is_empty());

        if let Some(ref s) = subject {
            if !subject_labels.contains(s) {
                subject_labels.push(s.clone());
            }
        }

        for col_idx in first_data_col..resolved.width {
            let cell = match resolved.cell_at(row_idx, col_idx) {
                Some(c) => c,
                None => continue,
            };

            let month = month_map.get(col_idx).and_then(|m| m.clone());

            // Look up background color: try inline style, then CSS class.
            let bg_color = cell
                .bg_color
                .as_ref()
                .filter(|c| c.as_str() != "#ffffff" && c.as_str() != "#fff")
                .cloned()
                .or_else(|| cell_class_colors.get(&(row_idx, col_idx)).cloned());

            // Look up unit for this column.
            let (unit_name, unit_color) = find_unit_for_column(&unit_spans, col_idx);

            let content_text = cell.text.trim();
            let content_html = cell.html.trim();

            // Skip completely empty cells (no text, no color).
            if content_text.is_empty() && bg_color.is_none() {
                continue;
            }

            cells.push(ParsedLtpCell {
                row_index: row_idx as i32,
                col_index: col_idx as i32,
                subject: subject.clone(),
                month,
                content_html: if content_html.is_empty() {
                    None
                } else {
                    Some(content_html.to_string())
                },
                content_text: if content_text.is_empty() {
                    None
                } else {
                    Some(content_text.to_string())
                },
                background_color: bg_color,
                unit_name: unit_name.map(|s| s.to_string()),
                unit_color: unit_color.map(|s| s.to_string()),
            });
        }
    }

    info!(
        cells = cells.len(),
        months = month_headers.len(),
        subjects = subject_labels.len(),
        "LTP document parsed"
    );

    LtpParseResult {
        cells,
        month_headers,
        subject_labels,
    }
}

/// Find the unit row — the row with colored unit spans.
///
/// Looks for a row between `month_row+1` and `month_row+4` that has
/// multiple non-white background colors.
fn find_unit_row(
    grid: &[Vec<Option<&TableCell>>],
    _width: usize,
    month_row: usize,
    first_data_col: usize,
    cell_class_colors: &HashMap<(usize, usize), String>,
) -> Option<usize> {
    for row_idx in (month_row + 1)..=(month_row + 4).min(grid.len().saturating_sub(1)) {
        let mut color_count = 0;
        let mut has_unit_text = false;

        if let Some(row) = grid.get(row_idx) {
            for (col_idx, cell_opt) in row.iter().enumerate().skip(first_data_col) {
                if let Some(cell) = cell_opt {
                    // Check for colored background (inline or CSS).
                    let has_color = cell
                        .bg_color
                        .as_ref()
                        .map_or(false, |c| c != "#ffffff" && c != "#fff")
                        || cell_class_colors.contains_key(&(row_idx, col_idx));

                    if has_color {
                        color_count += 1;
                    }

                    // Check for unit-like text.
                    let text = cell.text.trim().to_lowercase();
                    if text.contains("unit") || text.contains("boy") || text.contains("winter") {
                        has_unit_text = true;
                    }
                }
            }
        }

        if has_unit_text && color_count >= 2 {
            return Some(row_idx);
        }
    }
    None
}

/// Extract unit spans from the grid using resolved cell references and CSS colors.
fn extract_unit_spans_from_grid(
    grid: &[Vec<Option<&TableCell>>],
    unit_row: usize,
    first_data_col: usize,
    width: usize,
    cell_class_colors: &HashMap<(usize, usize), String>,
) -> Vec<UnitSpan> {
    let mut units = Vec::new();
    let mut col = first_data_col;
    let mut last_cell_ptr: Option<*const TableCell> = None;

    while col < width {
        if let Some(Some(cell)) = grid.get(unit_row).and_then(|r| r.get(col)) {
            let ptr = *cell as *const TableCell;
            if last_cell_ptr == Some(ptr) {
                col += 1;
                continue;
            }
            last_cell_ptr = Some(ptr);

            let text = cell.text.trim();
            if text.is_empty() {
                col += 1;
                continue;
            }

            let start_col = col;
            while col < width {
                if let Some(Some(c)) = grid.get(unit_row).and_then(|r| r.get(col)) {
                    if *c as *const _ == ptr {
                        col += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            let color = cell
                .bg_color
                .as_ref()
                .filter(|c| c.as_str() != "#ffffff" && c.as_str() != "#fff")
                .cloned()
                .or_else(|| cell_class_colors.get(&(unit_row, start_col)).cloned());

            units.push(UnitSpan {
                name: text.to_string(),
                color,
                start_col,
                end_col: col,
            });
        } else {
            col += 1;
        }
    }
    units
}

/// Find the unit that covers a given column.
fn find_unit_for_column(units: &[UnitSpan], col: usize) -> (Option<&str>, Option<&str>) {
    for unit in units {
        if col >= unit.start_col && col < unit.end_col {
            return (
                Some(unit.name.as_str()),
                unit.color.as_deref(),
            );
        }
    }
    (None, None)
}

/// Extract CSS background colors for each cell position in the grid.
///
/// Since `TableCell` doesn't store the CSS class name, we re-parse the HTML
/// to find `<td class="sN">` elements and map their grid positions to colors.
fn extract_css_bg_colors_for_cells(
    html: &str,
    css_colors: &HashMap<String, String>,
) -> HashMap<(usize, usize), String> {
    let mut result = HashMap::new();
    if css_colors.is_empty() {
        return result;
    }

    let document = Html::parse_document(html);
    let table_sel = Selector::parse("table").expect("valid selector");
    let tr_sel = Selector::parse("tr").expect("valid selector");
    let cell_sel = Selector::parse("td, th").expect("valid selector");

    // Find the main table (largest by row count).
    let table_el = document
        .select(&table_sel)
        .max_by_key(|t| t.select(&tr_sel).count());

    let table_el = match table_el {
        Some(t) => t,
        None => return result,
    };

    let table_node_id = table_el.id();

    // Walk the table rows and cells, maintaining a grid cursor that accounts
    // for colspan and rowspan — mirroring the resolve_grid algorithm.
    let rows: Vec<_> = table_el
        .select(&tr_sel)
        .filter(|tr| {
            tr.ancestors()
                .filter_map(|a| {
                    a.value().as_element().and_then(|e| {
                        if e.name() == "table" {
                            Some(a.id())
                        } else {
                            None
                        }
                    })
                })
                .next()
                == Some(table_node_id)
        })
        .collect();

    let height = rows.len();
    // Estimate width from the first row.
    let width = rows
        .iter()
        .map(|tr| {
            tr.select(&cell_sel)
                .map(|c| {
                    c.value()
                        .attr("colspan")
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(1)
                })
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0);

    if width == 0 || height == 0 {
        return result;
    }

    let mut occupied = vec![vec![false; width + 10]; height];

    for (row_idx, tr) in rows.iter().enumerate() {
        let mut col_cursor = 0;

        let cells: Vec<_> = tr
            .select(&cell_sel)
            .filter(|cell| {
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
                    .next()
                    == Some(tr.id())
            })
            .collect();

        for cell_el in &cells {
            while col_cursor < occupied[row_idx].len() && occupied[row_idx][col_cursor] {
                col_cursor += 1;
            }

            let colspan = cell_el
                .value()
                .attr("colspan")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            let rowspan = cell_el
                .value()
                .attr("rowspan")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);

            // Check if this cell has a CSS class with a background color.
            let class_attr = cell_el.value().attr("class").unwrap_or("");
            let color = class_attr
                .split_whitespace()
                .find_map(|cls| css_colors.get(cls));

            // Mark occupied and store color.
            for dr in 0..rowspan {
                for dc in 0..colspan {
                    let r = row_idx + dr;
                    let c = col_cursor + dc;
                    if r < height && c < occupied[0].len() {
                        occupied[r][c] = true;
                        if let Some(color) = color {
                            result.insert((r, c), color.clone());
                        }
                    }
                }
            }

            col_cursor += colspan;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_css_colors() {
        let html = r#"<html><head><style>
            .ritz .waffle .s0{background-color:#674ea7;color:#000}
            .ritz .waffle .s3{background-color:#ff9900;font-size:12pt}
            .ritz .waffle .s10{background-color:#ffffff;color:#000}
        </style></head><body></body></html>"#;

        let colors = extract_css_colors(html);
        assert_eq!(colors.get("s0"), Some(&"#674ea7".to_string()));
        assert_eq!(colors.get("s3"), Some(&"#ff9900".to_string()));
        // White should be excluded.
        assert!(!colors.contains_key("s10"));
    }

    #[test]
    fn test_extract_css_colors_empty() {
        let colors = extract_css_colors("<html><body></body></html>");
        assert!(colors.is_empty());
    }

    #[test]
    fn test_extract_bg_color_from_css_rule() {
        assert_eq!(
            extract_bg_color_from_css_rule("background-color:#ff9900;font-size:12pt"),
            Some("#ff9900".to_string())
        );
        assert_eq!(
            extract_bg_color_from_css_rule("color:#000;background-color:#00ffff;"),
            Some("#00ffff".to_string())
        );
        assert_eq!(
            extract_bg_color_from_css_rule("background-color:transparent;"),
            None
        );
        assert_eq!(
            extract_bg_color_from_css_rule("no-bg-here"),
            None
        );
    }

#[test]
    fn test_count_month_names() {
        assert_eq!(count_month_names(&["August", "September", "October"]), 3);
        assert_eq!(count_month_names(&["", "august", "SEPTEMBER", "foo"]), 2);
        assert_eq!(count_month_names(&["Monday", "Tuesday"]), 0);
    }

    #[test]
    fn test_parse_ltp_html_simple() {
        // A minimal LTP-like table.
        let html = r#"<html><head><style>
            .ritz .waffle .s3{background-color:#ff9900}
            .ritz .waffle .s4{background-color:#00ffff}
        </style></head><body>
        <table class="waffle">
            <tr><th>1</th><td></td><td>August</td><td>September</td><td>October</td></tr>
            <tr><th>2</th><td>Instructional Days:</td><td>13</td><td>19</td><td>23</td></tr>
            <tr><th>3</th><td>Units</td><td class="s3">BOY</td><td class="s4" colspan="2">Unit 1</td></tr>
            <tr><th>4</th><td>Reading</td><td>Books about School</td><td>Self and Families</td><td>Community</td></tr>
            <tr><th>5</th><td>Math</td><td>Counting</td><td>Shapes</td><td>Addition</td></tr>
        </table>
        </body></html>"#;

        let result = parse_ltp_html(html);
        assert!(!result.cells.is_empty(), "Should parse cells");
        assert!(result.month_headers.contains(&"August".to_string()));
        assert!(result.month_headers.contains(&"September".to_string()));
        assert!(result.month_headers.contains(&"October".to_string()));

        // Check that subject labels were found.
        assert!(result.subject_labels.contains(&"Reading".to_string()));
        assert!(result.subject_labels.contains(&"Math".to_string()));

        // Check that cells have month assignments.
        let reading_aug = result
            .cells
            .iter()
            .find(|c| {
                c.subject.as_deref() == Some("Reading")
                    && c.month.as_deref() == Some("August")
            });
        assert!(reading_aug.is_some(), "Should find Reading/August cell");
        assert_eq!(
            reading_aug.unwrap().content_text.as_deref(),
            Some("Books about School")
        );

        // Check unit assignment.
        let reading_sep = result
            .cells
            .iter()
            .find(|c| {
                c.subject.as_deref() == Some("Reading")
                    && c.month.as_deref() == Some("September")
            });
        assert!(reading_sep.is_some(), "Should find Reading/September cell");
        assert_eq!(
            reading_sep.unwrap().unit_name.as_deref(),
            Some("Unit 1")
        );
    }

    #[test]
    fn test_parse_ltp_html_empty() {
        let result = parse_ltp_html("");
        assert!(result.cells.is_empty());
        assert!(result.month_headers.is_empty());
    }

    #[test]
    fn test_parse_ltp_html_no_months() {
        let html = r#"<html><body>
        <table>
            <tr><td>Title</td><td>Content</td></tr>
            <tr><td>Lesson 1</td><td>Details</td></tr>
        </table>
        </body></html>"#;

        let result = parse_ltp_html(html);
        assert!(result.cells.is_empty(), "No month headers means not an LTP");
    }

    #[test]
    fn test_parse_ltp_html_with_colspan() {
        // October spans 2 columns.
        let html = r#"<html><body>
        <table>
            <tr><td></td><td>August</td><td colspan="2">October</td><td>November</td></tr>
            <tr><td>Instructional Days:</td><td>13</td><td>10</td><td>13</td><td>15</td></tr>
            <tr><td>Units</td><td>BOY</td><td colspan="2">Unit 1</td><td>Unit 2</td></tr>
            <tr><td>Reading</td><td>Intro</td><td>Ch 1</td><td>Ch 2</td><td>Review</td></tr>
        </table>
        </body></html>"#;

        let result = parse_ltp_html(html);

        // October should map to two columns — both Ch 1 and Ch 2 should be "October".
        let oct_reading_cells: Vec<_> = result
            .cells
            .iter()
            .filter(|c| {
                c.month.as_deref() == Some("October")
                    && c.subject.as_deref() == Some("Reading")
            })
            .collect();
        assert_eq!(oct_reading_cells.len(), 2, "October spans 2 columns, Reading should have 2 cells");
    }

    #[test]
    fn test_parse_ltp_html_with_freezebar() {
        // Simulates Google Sheets export with row-number and freezebar columns.
        let html = r#"<html><body>
        <table>
            <tr><th class="row-headers-background">1</th><td></td><td class="freezebar-cell"></td><td>August</td><td>September</td><td>October</td></tr>
            <tr><th class="row-headers-background">2</th><td>Days:</td><td class="freezebar-cell"></td><td>13</td><td>19</td><td>23</td></tr>
            <tr><th class="row-headers-background">3</th><td>Units</td><td class="freezebar-cell"></td><td>BOY</td><td>Unit 1</td><td>Unit 2</td></tr>
            <tr><th class="row-headers-background">4</th><td>Reading</td><td class="freezebar-cell"></td><td>Intro</td><td>Books</td><td>Review</td></tr>
            <tr><th class="row-headers-background">5</th><td>Math</td><td class="freezebar-cell"></td><td>Counting</td><td>Shapes</td><td>Addition</td></tr>
        </table>
        </body></html>"#;

        let result = parse_ltp_html(html);
        assert!(!result.cells.is_empty(), "Should parse cells with freezebar");
        assert!(result.subject_labels.contains(&"Reading".to_string()));
        assert!(result.subject_labels.contains(&"Math".to_string()));
    }

    #[test]
    fn test_find_unit_for_column() {
        let units = vec![
            UnitSpan {
                name: "BOY".to_string(),
                color: Some("#ff9900".to_string()),
                start_col: 3,
                end_col: 4,
            },
            UnitSpan {
                name: "Unit 1".to_string(),
                color: Some("#00ffff".to_string()),
                start_col: 4,
                end_col: 7,
            },
        ];

        let (name, color) = find_unit_for_column(&units, 3);
        assert_eq!(name, Some("BOY"));
        assert_eq!(color, Some("#ff9900"));

        let (name, color) = find_unit_for_column(&units, 5);
        assert_eq!(name, Some("Unit 1"));
        assert_eq!(color, Some("#00ffff"));

        let (name, _) = find_unit_for_column(&units, 10);
        assert_eq!(name, None);
    }

    #[test]
    #[ignore] // Requires sample file: ~/Downloads/_TK 23-24_24-25 LTP/24-25 TK Long Term Plan.html
    fn test_parse_real_ltp_file() {
        let path = dirs::home_dir()
            .unwrap()
            .join("Downloads/_TK 23-24_24-25 LTP/24-25 TK Long Term Plan.html");
        if !path.exists() {
            eprintln!("Skipping: sample LTP file not found at {:?}", path);
            return;
        }

        let html = std::fs::read_to_string(&path).unwrap();
        let result = parse_ltp_html(&html);

        // Should find all 11 months (Aug–June).
        assert!(
            result.month_headers.len() >= 10,
            "Expected at least 10 months, got {}: {:?}",
            result.month_headers.len(),
            result.month_headers
        );

        // Should find subject labels.
        assert!(
            result.subject_labels.len() >= 5,
            "Expected at least 5 subjects, got {}: {:?}",
            result.subject_labels.len(),
            result.subject_labels
        );

        // Should have a reasonable number of cells (the real LTP has ~200+ content cells).
        assert!(
            result.cells.len() >= 50,
            "Expected at least 50 cells, got {}",
            result.cells.len()
        );

        // Check that known subjects are present.
        let subjects: Vec<&str> = result.subject_labels.iter().map(|s| s.as_str()).collect();
        // At least some of these should be present (exact names may vary).
        let known_subjects = ["Reading", "Math", "Science", "Social Studies"];
        let found = known_subjects
            .iter()
            .filter(|ks| {
                subjects
                    .iter()
                    .any(|s| s.to_lowercase().contains(&ks.to_lowercase()))
            })
            .count();
        assert!(
            found >= 2,
            "Expected at least 2 known subjects, found {}: {:?}",
            found,
            subjects
        );

        // Check that unit assignments are present.
        let cells_with_units: Vec<_> = result
            .cells
            .iter()
            .filter(|c| c.unit_name.is_some())
            .collect();
        assert!(
            !cells_with_units.is_empty(),
            "Expected some cells with unit assignments"
        );

        // Print summary for debugging.
        eprintln!(
            "Real LTP parse: {} cells, {} months, {} subjects",
            result.cells.len(),
            result.month_headers.len(),
            result.subject_labels.len()
        );
        eprintln!("Months: {:?}", result.month_headers);
        eprintln!("Subjects: {:?}", result.subject_labels);

        let unit_names: std::collections::HashSet<_> = cells_with_units
            .iter()
            .filter_map(|c| c.unit_name.as_deref())
            .collect();
        eprintln!("Units found: {:?}", unit_names);
    }

    #[test]
    fn test_extract_css_bg_colors_for_cells() {
        let html = r#"<html><head><style>
            .ritz .waffle .s3{background-color:#ff9900}
        </style></head><body>
        <table>
            <tr><td>A</td><td class="s3">B</td></tr>
            <tr><td>C</td><td>D</td></tr>
        </table>
        </body></html>"#;

        let css_colors = extract_css_colors(html);
        let cell_colors = extract_css_bg_colors_for_cells(html, &css_colors);

        // Cell at (0,1) should have color #ff9900.
        assert_eq!(cell_colors.get(&(0, 1)), Some(&"#ff9900".to_string()));
        // Cell at (0,0) should have no color.
        assert!(!cell_colors.contains_key(&(0, 0)));
    }
}
