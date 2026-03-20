//! Template extractor — analyzes table formatting patterns during digest.
//!
//! Extracts a [`TeachingTemplateSchema`] from parsed HTML tables that captures
//! HOW a teacher formats their plans: color scheme, table structure, time slot
//! patterns, content organization, and recurring elements. This schema is stored
//! alongside reference documents and used to format AI-generated plans to match
//! the teacher's style.
//!
//! When an AI provider is available, uses an LLM call to identify the correct
//! planning template table (instead of relying on heuristic scoring). The AI
//! call happens once per digest, not per chat message, so cost is minimal.
//! Falls back to heuristic scoring when AI is not configured.

use std::collections::HashMap;

use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::chat::provider::{AiProvider, CompletionMessage};
use crate::database::{
    ColorMapping, ColorScheme, ContentPatterns, DailyRoutineEvent, RecurringElements,
    RoutineEventType, TableStructure, TeachingTemplateSchema,
};
use crate::errors::ChalkError;

use super::parser::{self, ParsedTable, TableCell};
use super::{capitalize_header, detect_schedule_columns, is_merged_row, is_time_like, DAY_NAMES};

/// A resolved 2D grid built from a parsed table using the standard HTML table
/// grid-building algorithm. Each position in the grid maps to the cell that
/// occupies it (accounting for colspan and rowspan).
///
/// This is the same algorithm browsers use to lay out tables:
/// 1. For each row, walk cells left-to-right
/// 2. Place each cell in the next unoccupied grid position
/// 3. Mark all spanned positions (colspan × rowspan rectangle) as occupied
///
/// The result is a complete, unambiguous 2D grid where every position is either
/// occupied by a cell reference or empty (for short rows with no colspan).
pub struct ResolvedGrid<'a> {
    /// 2D grid of cell references. `grid[row][col]` is `Some(&cell)` if that
    /// position is occupied, `None` if it's an empty trailing position.
    pub grid: Vec<Vec<Option<&'a TableCell>>>,
    /// The true number of columns in the table (grid width).
    pub width: usize,
    /// The number of rows in the grid.
    pub height: usize,
}

impl<'a> ResolvedGrid<'a> {
    /// Check if a grid row is a merged/section-divider row.
    ///
    /// A row where a single source cell spans the entire grid width is a
    /// section divider (e.g., "Week 5", "Spring Break", "NO SCHOOL").
    pub fn is_full_width_merge(&self, row_idx: usize) -> bool {
        if self.width <= 2 {
            return false;
        }
        if let Some(grid_row) = self.grid.get(row_idx) {
            // Check if all cells in this row point to the same source cell.
            let first = grid_row.first().and_then(|c| c.map(|cell| cell as *const _));
            if let Some(first_ptr) = first {
                return grid_row.iter().all(|c| {
                    c.map(|cell| cell as *const _ == first_ptr).unwrap_or(false)
                });
            }
        }
        false
    }

    /// Get the text of a cell at a grid position.
    pub fn cell_text(&self, row: usize, col: usize) -> &str {
        self.grid
            .get(row)
            .and_then(|r| r.get(col))
            .and_then(|c| c.as_ref())
            .map(|c| c.text.trim())
            .unwrap_or("")
    }

    /// Get the cell reference at a grid position.
    pub fn cell_at(&self, row: usize, col: usize) -> Option<&'a TableCell> {
        self.grid.get(row).and_then(|r| r.get(col)).and_then(|c| *c)
    }
}

/// Build a resolved 2D grid from a parsed table using the standard HTML table
/// grid-building algorithm (the same algorithm browsers use).
///
/// Handles colspan, rowspan, and short rows correctly:
/// - A cell with `colspan="3"` occupies 3 grid columns
/// - A cell with `rowspan="2"` occupies 2 grid rows
/// - A short row (fewer cells than grid width, no colspan) has empty trailing positions
pub fn resolve_grid(table: &ParsedTable) -> ResolvedGrid<'_> {
    if table.rows.is_empty() {
        return ResolvedGrid {
            grid: Vec::new(),
            width: 0,
            height: 0,
        };
    }

    let height = table.rows.len();

    // First pass: determine grid width by finding the max effective row width.
    // Also account for rowspans that may push cells into later rows.
    let mut width = table.grid_width();

    // Build the grid. We use a "occupied" tracker to handle rowspans.
    // occupied[row][col] = true means that position is already taken by a
    // rowspan from a previous row.
    let mut grid: Vec<Vec<Option<&TableCell>>> = vec![vec![None; width]; height];
    let mut occupied: Vec<Vec<bool>> = vec![vec![false; width]; height];

    for (row_idx, row) in table.rows.iter().enumerate() {
        let mut col_cursor = 0;

        for cell in &row.cells {
            // Skip past occupied positions (from previous rowspans).
            while col_cursor < width && occupied[row_idx][col_cursor] {
                col_cursor += 1;
            }

            if col_cursor >= width {
                // Row has more cells than the grid width — extend the grid.
                width = col_cursor + cell.colspan;
                for grid_row in grid.iter_mut() {
                    grid_row.resize(width, None);
                }
                for occ_row in occupied.iter_mut() {
                    occ_row.resize(width, false);
                }
            }

            // Place this cell in the colspan × rowspan rectangle.
            let cs = cell.colspan;
            let rs = cell.rowspan;
            for dr in 0..rs {
                for dc in 0..cs {
                    let r = row_idx + dr;
                    let c = col_cursor + dc;
                    if r < height && c < width {
                        grid[r][c] = Some(cell);
                        occupied[r][c] = true;
                    }
                }
            }

            col_cursor += cs;
        }
    }

    ResolvedGrid {
        grid,
        width,
        height,
    }
}

/// Normalize an activity name for frequency-based matching.
///
/// In real teacher schedules, the same activity often has different detail text
/// across days. For example:
///   - "Soft Start Breakfast 8:15-9:00"
///   - "Soft Start Breakfast 8:15-9:00 Good Morning Preschool Friends..."
///
/// This function extracts the core activity name by:
/// 1. Taking only the first line of text
/// 2. Truncating before embedded time patterns (e.g., "8:15-9:00")
/// 3. Limiting to a reasonable length
fn normalize_activity_name(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return String::new();
    }

    // Find the position of the first embedded time-like pattern (digit:digit).
    // Many cells have "Activity Name 8:15-9:00 Extra Details..." — we want
    // just "Activity Name".
    let mut truncate_at = first_line.len();
    let bytes = first_line.as_bytes();
    for i in 0..bytes.len().saturating_sub(3) {
        if bytes[i].is_ascii_digit()
            && bytes.get(i + 1) == Some(&b':')
            && bytes.get(i + 2).map_or(false, |b| b.is_ascii_digit())
        {
            // Found a time pattern — truncate just before it (trim trailing space).
            truncate_at = i;
            break;
        }
    }

    let result = first_line[..truncate_at].trim();

    // If the result is empty (cell starts with a time), fall back to the full text.
    let result = if result.is_empty() { first_line } else { result };

    // If still too long, take just the first 4 words as the activity name.
    // Teacher schedule cells often have "Morning Circle Schedule Greeting Song/..."
    // where the first 2-3 words identify the activity.
    if result.len() > 60 {
        let words: Vec<&str> = result.split_whitespace().take(4).collect();
        let short = words.join(" ");
        if short.is_empty() {
            return String::new();
        }
        return short;
    }

    result.to_string()
}

/// Determine the effective header row index for a table.
///
/// Google Docs exports often have merged title rows (e.g., "Mrs. Cole's
/// TK Schedule 2025-2026") spanning all columns as row 0, with the actual
/// column headers (Day/Time, Monday, Tuesday, ...) in row 1 or later. Some
/// teachers have multiple merged title/banner rows before the real headers.
///
/// Uses the max-width heuristic: calculates the maximum cell count across all
/// rows (the expected grid width), then finds the first row with at least half
/// that many cells. Rows with far fewer cells than the max are merged title rows.
///
/// Returns the index of the first non-merged row, or 0 if all rows are similar.
pub(crate) fn effective_header_row(table: &ParsedTable) -> usize {
    if table.rows.len() < 2 {
        return 0;
    }

    // Build the resolved grid for accurate merged-row detection.
    let grid = resolve_grid(table);

    if grid.width <= 2 {
        return 0;
    }

    // Find the first row that is NOT a full-width merged banner.
    // A full-width merge means a single cell spans all columns — that's
    // a title row like "Mrs. Cole's TK Schedule 2025-2026".
    for i in 0..grid.height {
        if !grid.is_full_width_merge(i) {
            // Also verify the row has enough distinct cells to be a header.
            let cell_count = table.rows.get(i).map_or(0, |r| r.cells.len());
            if cell_count > 1 {
                return i;
            }
        }
    }

    0
}

// ── AI Table Identification ──────────────────────────────────

/// Structured response from the AI identifying the planning template table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTableIdentification {
    /// Zero-based index of the table the AI identified as the planning template.
    pub table_index: usize,
    /// What the columns represent (e.g., "days_of_week", "lesson_attributes").
    pub column_semantic: String,
    /// What the rows represent (e.g., "time_slots", "lessons", "categories").
    pub row_semantic: String,
    /// The type of table layout (e.g., "schedule_grid", "standard_table").
    pub layout_type: String,
}

/// System prompt for the AI table identification call.
const TABLE_ID_SYSTEM_PROMPT: &str = r#"You are a document analysis assistant. Your task is to identify which table in a teacher's Google Doc is the weekly lesson planning template.

Teachers often have multiple tables in their documents:
- Reference/archive tables (e.g., curriculum mapping with columns like "LP 2022-2023", "Eureka Math")
- Weekly planning grids (days of the week as columns, time slots as rows)
- Standard lesson plan tables (columns for title, subject, objectives, etc.)
- Miscellaneous tables (notes, contacts, supply lists)

You must identify THE planning template — the table the teacher actually uses to plan their weekly lessons.

Respond with ONLY a JSON object (no markdown, no explanation) in this exact format:
{"table_index": 0, "column_semantic": "days_of_week", "row_semantic": "time_slots", "layout_type": "schedule_grid"}

Fields:
- table_index: zero-based index of the planning template table
- column_semantic: what the columns represent. Common values: "days_of_week", "lesson_attributes" (title/subject/duration columns), "time_periods" (semesters, quarters)
- row_semantic: what the rows represent. Common values: "time_slots", "lessons", "categories", "subjects"
- layout_type: either "schedule_grid" (days × time slots) or "standard_table" (any other format)"#;

/// Format a summary of all tables for the AI prompt.
///
/// Sends headers + first few rows of each table so the AI can identify
/// which one is the planning template without seeing the full document.
fn format_tables_for_ai(tables: &[ParsedTable]) -> String {
    let mut summary = String::new();

    for (i, table) in tables.iter().enumerate() {
        summary.push_str(&format!("=== Table {} ({} rows, {} columns) ===\n",
            i,
            table.rows.len(),
            table.rows.first().map_or(0, |r| r.cells.len()),
        ));

        // Show headers + first 3 data rows.
        let rows_to_show = table.rows.len().min(4);
        for (row_idx, row) in table.rows.iter().take(rows_to_show).enumerate() {
            let label = if row_idx == 0 { "Header" } else { "Row" };
            let cells: Vec<&str> = row.cells.iter()
                .map(|c| {
                    let text = c.text.trim();
                    if text.len() > 60 { &text[..60] } else { text }
                })
                .collect();
            summary.push_str(&format!("  {}: [{}]\n", label, cells.join(" | ")));
        }

        if table.rows.len() > 4 {
            summary.push_str(&format!("  ... ({} more rows)\n", table.rows.len() - 4));
        }
        summary.push('\n');
    }

    summary
}

/// Use AI to identify which table is the planning template.
///
/// Sends a summary of all tables to the LLM and parses the structured response.
/// Returns `None` if the AI response cannot be parsed or the table index is out of range.
pub async fn identify_planning_table_with_ai(
    provider: &dyn AiProvider,
    tables: &[ParsedTable],
) -> Result<AiTableIdentification, ChalkError> {
    let table_summary = format_tables_for_ai(tables);

    let messages = vec![
        CompletionMessage {
            role: "system".to_string(),
            content: TABLE_ID_SYSTEM_PROMPT.to_string(),
        },
        CompletionMessage {
            role: "user".to_string(),
            content: format!(
                "Here are {} tables from a teacher's Google Doc. Which one is the weekly lesson planning template?\n\n{}",
                tables.len(),
                table_summary
            ),
        },
    ];

    let response = provider.complete(&messages, 256, 0.0).await?;

    // Parse the JSON response, stripping any markdown code fences.
    let json_str = response.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let identification: AiTableIdentification = serde_json::from_str(json_str)
        .map_err(|e| ChalkError::new(
            crate::errors::ErrorDomain::Digest,
            crate::errors::ErrorCode::DigestParseFailed,
            format!("Failed to parse AI table identification response: {}. Raw: {}", e, response),
        ))?;

    // Validate the table index.
    if identification.table_index >= tables.len() {
        return Err(ChalkError::new(
            crate::errors::ErrorDomain::Digest,
            crate::errors::ErrorCode::DigestParseFailed,
            format!(
                "AI returned table_index {} but only {} tables exist",
                identification.table_index,
                tables.len()
            ),
        ));
    }

    info!(
        table_index = identification.table_index,
        column_semantic = identification.column_semantic.as_str(),
        row_semantic = identification.row_semantic.as_str(),
        layout_type = identification.layout_type.as_str(),
        "AI identified planning template table"
    );

    Ok(identification)
}

/// Detect a transposed schedule grid where days are in the first column (rows)
/// and time slots are in the header row (columns).
///
/// Returns `Some((day_rows, time_columns))` where:
/// - `day_rows` = `(row_index, day_label)` for each row containing a day name
/// - `time_columns` = column indices whose header cell is time-like
///
/// Returns `None` if the table does not match a transposed schedule pattern.
fn detect_transposed_schedule(table: &ParsedTable) -> Option<(Vec<(usize, String)>, Vec<usize>)> {
    if table.rows.len() < 3 {
        return None;
    }

    let h_idx = effective_header_row(table);

    // Check if first column of data rows contains day names.
    let mut day_rows: Vec<(usize, String)> = Vec::new();
    for (row_idx, row) in table.rows.iter().enumerate().skip(h_idx + 1) {
        if let Some(first_cell) = row.cells.first() {
            let text = first_cell.text.trim().to_lowercase();
            if DAY_NAMES.iter().any(|d| text.contains(d)) {
                day_rows.push((row_idx, capitalize_header(first_cell.text.trim())));
            }
        }
    }

    if day_rows.len() < 2 {
        return None;
    }

    // Check if header row (columns 1+) contains time-like values.
    let headers = &table.rows[h_idx].cells;
    let time_cols: Vec<usize> = headers
        .iter()
        .enumerate()
        .skip(1) // skip first column (likely "Day" label)
        .filter(|(_, c)| is_time_like(c.text.trim()))
        .map(|(i, _)| i)
        .collect();

    if time_cols.len() >= 2 {
        Some((day_rows, time_cols))
    } else {
        None
    }
}

/// Select the index of the best planning table by heuristic scoring.
///
/// Logs all table scores for diagnostic visibility, then returns the index
/// of the highest-scoring table.
fn select_best_table_index(tables: &[ParsedTable]) -> usize {
    if tables.is_empty() {
        return 0;
    }
    for (i, table) in tables.iter().enumerate() {
        let score = score_planning_table(table);
        let headers: Vec<String> = table
            .rows
            .first()
            .map(|r| r.cells.iter().map(|c| c.text.trim().to_string()).collect())
            .unwrap_or_default();
        info!(
            table_index = i,
            score = score,
            rows = table.rows.len(),
            cols = headers.len(),
            headers = headers.join(" | ").as_str(),
            "Heuristic table score (select_best_table_index)"
        );
    }
    tables
        .iter()
        .enumerate()
        .max_by_key(|(_, t)| score_planning_table(t))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Extract a teaching template schema using AI to identify the correct table.
///
/// This is the preferred entry point during digest when an AI provider is
/// available. Falls back to heuristic scoring if the AI call fails.
/// Returns `(schema, method)` where method is `"ai"` or `"heuristic"`.
pub async fn extract_template_with_ai(
    html: &str,
    provider: &dyn AiProvider,
) -> (TeachingTemplateSchema, &'static str) {
    let tables = parser::extract_tables(html);
    if tables.is_empty() {
        return (TeachingTemplateSchema::default(), "none");
    }

    // Try AI identification first.
    let ai_result = if tables.len() > 1 {
        match identify_planning_table_with_ai(provider, &tables).await {
            Ok(id) => {
                info!(
                    table_index = id.table_index,
                    layout_type = id.layout_type.as_str(),
                    column_semantic = id.column_semantic.as_str(),
                    row_semantic = id.row_semantic.as_str(),
                    "AI successfully identified planning table"
                );
                Some(id)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    table_count = tables.len(),
                    "AI table identification FAILED — falling back to heuristic scoring. \
                     This may select the wrong table if archive tables are present."
                );
                None
            }
        }
    } else {
        info!("Only 1 table in document — skipping AI identification");
        None
    };

    let method = if ai_result.is_some() { "ai" } else { "heuristic" };

    // Determine the selected table index so ALL extraction functions operate
    // on the same table. Previously, extract_time_slots/extract_daily_routine/
    // extract_recurring_elements iterated ALL tables independently, which could
    // extract data from the wrong table when multiple schedule-like tables exist,
    // or extract nothing when the AI-identified table has a non-standard orientation.
    let selected_idx = match &ai_result {
        Some(ai_id) => ai_id.table_index,
        None => select_best_table_index(&tables),
    };
    let selected = &tables[selected_idx..selected_idx + 1];

    info!(
        selected_table = selected_idx,
        method = method,
        total_tables = tables.len(),
        "Scoping all extraction to selected table"
    );

    let color_scheme = extract_colors(html);
    let table_structure = match &ai_result {
        Some(ai_id) => {
            info!(method = "ai", "Building table structure from AI-identified table");
            extract_table_structure_with_ai(&tables, ai_id)
        }
        None => {
            info!(method = "heuristic", "Building table structure from heuristic scoring");
            extract_table_structure(selected)
        }
    };
    let time_slots = extract_time_slots(selected);
    let content_patterns = extract_content_patterns(html, selected);

    // For recurring elements and daily routine, use ALL schedule-grid tables
    // (not just the selected one). When a document contains multiple weekly
    // plans, each table represents a different week with the same recurring
    // structure. Aggregating across all schedule tables produces more robust
    // detection of truly recurring events.
    let schedule_tables: Vec<&ParsedTable> = tables.iter()
        .filter(|t| {
            if t.rows.len() < 3 { return false; }
            let h = effective_header_row(t);
            let hdrs: Vec<String> = t.rows[h].cells.iter()
                .map(|c| c.text.trim().to_lowercase()).collect();
            detect_schedule_columns(&hdrs).is_some()
        })
        .collect();
    let routine_tables: &[ParsedTable] = if schedule_tables.is_empty() {
        selected
    } else {
        // Safety: we only read from these, and the tables slice outlives this scope.
        // We need a contiguous slice, so we can't directly use Vec<&ParsedTable>.
        // Instead, pass ALL tables and let the extraction functions filter internally.
        &tables
    };
    let recurring_elements = extract_recurring_elements(routine_tables);
    let daily_routine = extract_daily_routine(routine_tables);

    let schema = TeachingTemplateSchema {
        color_scheme,
        table_structure,
        time_slots,
        content_patterns,
        recurring_elements,
        daily_routine,
    };
    (schema, method)
}

/// Extract a teaching template schema from the raw HTML of a Google Doc.
///
/// Uses heuristic scoring to select the planning table. This is the fallback
/// when AI is not configured.
pub fn extract_template(html: &str) -> TeachingTemplateSchema {
    let tables = parser::extract_tables(html);
    if tables.is_empty() {
        return TeachingTemplateSchema::default();
    }

    // Select the best table FIRST so all extraction functions operate on the
    // same table. Previously each function iterated all tables independently.
    let best_idx = select_best_table_index(&tables);
    let selected = &tables[best_idx..best_idx + 1];

    let color_scheme = extract_colors(html);
    let table_structure = extract_table_structure(selected);
    let time_slots = extract_time_slots(selected);
    let content_patterns = extract_content_patterns(html, selected);

    // For recurring elements and daily routine, aggregate across ALL schedule-grid
    // tables for better detection when the document contains multiple weekly plans.
    let has_schedule_tables = tables.iter().any(|t| {
        if t.rows.len() < 3 { return false; }
        let h = effective_header_row(t);
        let hdrs: Vec<String> = t.rows[h].cells.iter()
            .map(|c| c.text.trim().to_lowercase()).collect();
        detect_schedule_columns(&hdrs).is_some()
    });
    let routine_tables: &[ParsedTable] = if has_schedule_tables && tables.len() > 1 {
        &tables
    } else {
        selected
    };
    let recurring_elements = extract_recurring_elements(routine_tables);
    let daily_routine = extract_daily_routine(routine_tables);

    TeachingTemplateSchema {
        color_scheme,
        table_structure,
        time_slots,
        content_patterns,
        recurring_elements,
        daily_routine,
    }
}

/// Extract color-to-category mappings from inline styles in the HTML.
///
/// Google Docs exports use inline styles like `background-color:#9900ff` and
/// `color:#00ffff`. We tally occurrences and infer categories based on where
/// colors appear (headers vs body cells).
fn extract_colors(html: &str) -> ColorScheme {
    let document = Html::parse_document(html);

    let mut header_colors: HashMap<String, usize> = HashMap::new();
    let mut cell_colors: HashMap<String, usize> = HashMap::new();

    // Check header cells (<th>) for background colors.
    if let Ok(th_sel) = Selector::parse("th") {
        for el in document.select(&th_sel) {
            collect_element_colors(&el, &mut header_colors);
        }
    }

    // Check data cells (<td>) for background colors.
    if let Ok(td_sel) = Selector::parse("td") {
        for el in document.select(&td_sel) {
            collect_element_colors(&el, &mut cell_colors);
        }
    }

    let mut mappings = Vec::new();

    for (color, freq) in &header_colors {
        mappings.push(ColorMapping {
            color: color.clone(),
            category: "header".to_string(),
            frequency: *freq,
        });
    }

    for (color, freq) in &cell_colors {
        // Skip colors already categorized as headers.
        if header_colors.contains_key(color) {
            continue;
        }
        let category = if *freq > 10 {
            "highlight"
        } else {
            "activity"
        };
        mappings.push(ColorMapping {
            color: color.clone(),
            category: category.to_string(),
            frequency: *freq,
        });
    }

    // Sort by frequency (most common first).
    mappings.sort_by(|a, b| b.frequency.cmp(&a.frequency));

    ColorScheme { mappings }
}

/// Collect background-color values from an element and its styled children.
fn collect_element_colors(el: &scraper::ElementRef, colors: &mut HashMap<String, usize>) {
    // Check the element itself.
    if let Some(style) = el.value().attr("style") {
        if let Some(color) = parse_background_color(style) {
            *colors.entry(color).or_insert(0) += 1;
        }
    }

    // Check styled spans/divs inside the element.
    if let Ok(span_sel) = Selector::parse("span, div, p") {
        for child in el.select(&span_sel) {
            if let Some(style) = child.value().attr("style") {
                if let Some(color) = parse_background_color(style) {
                    *colors.entry(color).or_insert(0) += 1;
                }
            }
        }
    }
}

/// Parse a `background-color` value from a CSS style string.
/// Returns the color as a lowercase hex string or named color.
fn parse_background_color(style: &str) -> Option<String> {
    let lower = style.to_lowercase();
    // Match "background-color:" or "background:" followed by a color value.
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
            // Skip "transparent", "inherit", "none", "initial".
            if color == "transparent"
                || color == "inherit"
                || color == "none"
                || color == "initial"
            {
                return None;
            }
            return Some(color);
        }
    }
    None
}

/// Check if a string contains a year range like "2022-2023", "2023/2024", or "2022-23".
fn contains_year_range(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 7 {
        return false;
    }
    for i in 0..=bytes.len().saturating_sub(7) {
        // Check for "20" followed by 2 digits (a 4-digit year starting with 20)
        if bytes.get(i..i + 2) == Some(b"20")
            && bytes.get(i + 2).map_or(false, |b| b.is_ascii_digit())
            && bytes.get(i + 3).map_or(false, |b| b.is_ascii_digit())
        {
            // Check for separator after the 4-digit year
            let sep = bytes.get(i + 4).copied();
            if sep == Some(b'-') || sep == Some(b'/') {
                // Check for at least 2 more digits after separator
                if bytes.get(i + 5).map_or(false, |b| b.is_ascii_digit())
                    && bytes.get(i + 6).map_or(false, |b| b.is_ascii_digit())
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Score a table for how likely it is to be the teacher's weekly planning template.
///
/// Higher score = more likely to be the actual planning table.
/// Penalizes tables that look like reference/archive data (year ranges, multi-year
/// columns like "LP 2022-2023"), and rewards tables with day-of-week columns
/// and time-like first-column values.
fn score_planning_table(table: &ParsedTable) -> i32 {
    if table.rows.is_empty() {
        return 0;
    }

    let header_idx = effective_header_row(table);
    let headers: Vec<String> = table.rows[header_idx]
        .cells
        .iter()
        .map(|c| c.text.trim().to_string())
        .collect();
    let header_lower: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();

    let mut score: i32 = 0;

    // Reward: has day-of-week columns (strong signal for a weekly planning grid).
    if detect_schedule_columns(&header_lower).is_some() {
        score += 50;
    }

    // Reward: transposed schedule grid (days in rows, times in columns).
    if detect_transposed_schedule(table).is_some() {
        score += 50;
    }

    let expected_width = headers.len();

    // Reward: has time-like values in the first column of data rows.
    let time_rows = table
        .rows
        .iter()
        .skip(header_idx + 1)
        .filter(|r| !is_merged_row(r, expected_width))
        .filter(|r| r.cells.first().map_or(false, |c| is_time_like(c.text.trim())))
        .count();
    score += (time_rows as i32) * 5;

    // Reward: reasonable number of rows (2-30 rows typical for weekly plans).
    // Exclude merged/section-divider rows from the count.
    let data_rows = table
        .rows
        .iter()
        .skip(header_idx + 1)
        .filter(|r| !is_merged_row(r, expected_width))
        .count();
    if (2..=30).contains(&data_rows) {
        score += 10;
    }

    // Reward: reasonable number of columns (5-7 typical for Mon-Fri + time col).
    if (5..=7).contains(&headers.len()) {
        score += 10;
    }

    // Penalty: headers contain year ranges (e.g., "2022-2023", "2023/2024").
    // These are reference/archive tables, not planning templates.
    // Use aggressive penalties — archive tables must NEVER win over real planning tables.
    let mut archive_penalty_count = 0;
    for header in &header_lower {
        if contains_year_range(header) {
            score -= 100;
            archive_penalty_count += 1;
        }
        // Penalty: headers like "LP", "Lesson Plan" followed by year.
        if header.contains("lp ") && header.chars().any(|c| c.is_ascii_digit()) {
            score -= 80;
            archive_penalty_count += 1;
        }
        // Penalty: curriculum name columns (e.g., "eureka math").
        if header.contains("eureka") || header.contains("curriculum") || header.contains("edition") {
            score -= 50;
            archive_penalty_count += 1;
        }
        // Penalty: headers that are just a bare 4-digit year (e.g., "2024", "2025").
        if header.trim().len() == 4 && header.trim().chars().all(|c| c.is_ascii_digit()) {
            let year: u32 = header.trim().parse().unwrap_or(0);
            if (2000..=2099).contains(&year) {
                score -= 60;
                archive_penalty_count += 1;
            }
        }
    }

    // If the majority of headers triggered archive penalties, this is almost certainly
    // an archive/reference table — apply an overwhelming additional penalty.
    if archive_penalty_count > 0 && archive_penalty_count * 2 >= headers.len() {
        score -= 500;
    }

    // Penalty: very few columns (1-2) — unlikely to be a plan grid.
    if headers.len() <= 2 {
        score -= 20;
    }

    // Small tiebreaker: more rows = slightly higher score.
    score += data_rows.min(15) as i32;

    score
}

/// Determine the table layout structure from the parsed tables.
///
/// Scores all tables to identify the actual planning template (not just the largest
/// table, which may be a reference/archive table). Adds semantic labels describing
/// what columns and rows represent.
fn extract_table_structure(tables: &[ParsedTable]) -> TableStructure {
    if tables.is_empty() {
        return TableStructure::default();
    }

    // Score each table and pick the best candidate for a planning template.
    // Log all scores so we can debug table selection issues.
    for (i, table) in tables.iter().enumerate() {
        let score = score_planning_table(table);
        let h_idx = effective_header_row(table);
        let headers: Vec<String> = table
            .rows
            .get(h_idx)
            .map(|r| r.cells.iter().map(|c| c.text.trim().to_string()).collect())
            .unwrap_or_default();
        info!(
            table_index = i,
            score = score,
            rows = table.rows.len(),
            cols = headers.len(),
            header_row = h_idx,
            headers = headers.join(" | ").as_str(),
            "Heuristic table score"
        );
    }

    let main_table = tables
        .iter()
        .max_by_key(|t| score_planning_table(t))
        .unwrap();

    if main_table.rows.is_empty() {
        return TableStructure::default();
    }

    let h_idx = effective_header_row(main_table);
    let headers: Vec<String> = main_table.rows[h_idx]
        .cells
        .iter()
        .map(|c| c.text.trim().to_string())
        .collect();

    let header_lower: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();

    let is_schedule = detect_schedule_columns(&header_lower).is_some();
    let is_transposed = !is_schedule && detect_transposed_schedule(main_table).is_some();

    let layout_type = if is_schedule || is_transposed {
        "schedule_grid".to_string()
    } else {
        "standard_table".to_string()
    };

    // Assign semantic labels based on detected orientation.
    let (column_semantic, row_semantic) = if is_schedule {
        // Standard: days in columns, time slots in rows.
        let has_time_rows = main_table
            .rows
            .iter()
            .skip(h_idx + 1)
            .any(|r| r.cells.first().map_or(false, |c| is_time_like(c.text.trim())));
        (
            Some("days_of_week".to_string()),
            if has_time_rows {
                Some("time_slots".to_string())
            } else {
                None
            },
        )
    } else if is_transposed {
        // Transposed: time slots in columns, days in rows.
        (
            Some("time_slots".to_string()),
            Some("days_of_week".to_string()),
        )
    } else {
        // Standard table — check if the first column looks like categories.
        let first_col_values: Vec<&str> = main_table
            .rows
            .iter()
            .skip(h_idx + 1)
            .filter_map(|r| r.cells.first().map(|c| c.text.trim()))
            .filter(|t| !t.is_empty())
            .collect();
        let row_sem = if !first_col_values.is_empty()
            && first_col_values.iter().all(|v| !is_time_like(v))
        {
            Some("categories".to_string())
        } else {
            None
        };
        (None, row_sem)
    };

    // Extract row categories from the first column of data rows,
    // skipping merged/section-divider rows.
    let mut row_categories = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let expected_width = headers.len();
    for row in main_table.rows.iter().skip(h_idx + 1) {
        if is_merged_row(row, expected_width) {
            continue;
        }
        if let Some(first_cell) = row.cells.first() {
            let text = first_cell.text.trim().to_string();
            if !text.is_empty() && !is_time_like(&text) && seen.insert(text.clone()) {
                row_categories.push(text);
            }
        }
    }

    let column_count = headers.len();

    // Clean up column names: for schedule grids, use canonical day names
    // instead of raw header text (which may include "Monday 8:15-3:05 3/23 ...").
    let clean_columns = if is_schedule {
        if let Some((_tc, day_cols)) = detect_schedule_columns(&header_lower) {
            let mut cols = Vec::with_capacity(column_count);
            for (i, h) in headers.iter().enumerate() {
                if let Some((_, day_name)) = day_cols.iter().find(|(ci, _)| *ci == i) {
                    cols.push(day_name.clone());
                } else {
                    // First column (time/day) — keep a clean label.
                    let lower = h.to_lowercase();
                    if lower.contains("time") || lower.contains("day") {
                        cols.push("Day/Time".to_string());
                    } else {
                        cols.push(h.clone());
                    }
                }
            }
            cols
        } else {
            headers
        }
    } else {
        headers
    };

    TableStructure {
        layout_type,
        columns: clean_columns,
        row_categories,
        column_count,
        column_semantic,
        row_semantic,
    }
}

/// Determine table layout structure using AI-identified table and semantics.
///
/// Uses the AI's identification to select the correct table and applies the
/// AI's semantic labels instead of inferring them from heuristics.
fn extract_table_structure_with_ai(
    tables: &[ParsedTable],
    ai_id: &AiTableIdentification,
) -> TableStructure {
    let main_table = match tables.get(ai_id.table_index) {
        Some(t) => t,
        None => return extract_table_structure(tables), // fallback
    };

    if main_table.rows.is_empty() {
        return TableStructure::default();
    }

    let h_idx = effective_header_row(main_table);
    let headers: Vec<String> = main_table.rows[h_idx]
        .cells
        .iter()
        .map(|c| c.text.trim().to_string())
        .collect();

    // Extract row categories from the first column of data rows,
    // skipping merged/section-divider rows.
    let mut row_categories = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let expected_width = headers.len();
    for row in main_table.rows.iter().skip(h_idx + 1) {
        if is_merged_row(row, expected_width) {
            continue;
        }
        if let Some(first_cell) = row.cells.first() {
            let text = first_cell.text.trim().to_string();
            if !text.is_empty() && !is_time_like(&text) && seen.insert(text.clone()) {
                row_categories.push(text);
            }
        }
    }

    let column_count = headers.len();

    // Clean up column names: for schedule grids, use canonical day names
    // instead of raw header text (which may include "Monday 8:15-3:05 3/23 ...").
    let header_lower: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();
    let is_schedule = ai_id.layout_type == "schedule_grid"
        || ai_id.column_semantic == "days_of_week"
        || detect_schedule_columns(&header_lower).is_some();

    let clean_columns = if is_schedule {
        if let Some((_tc, day_cols)) = detect_schedule_columns(&header_lower) {
            let mut cols = Vec::with_capacity(column_count);
            for (i, h) in headers.iter().enumerate() {
                if let Some((_, day_name)) = day_cols.iter().find(|(ci, _)| *ci == i) {
                    cols.push(day_name.clone());
                } else {
                    // First column (time/day) — keep a clean label.
                    let lower = h.to_lowercase();
                    if lower.contains("time") || lower.contains("day") {
                        cols.push("Day/Time".to_string());
                    } else {
                        cols.push(h.clone());
                    }
                }
            }
            cols
        } else {
            headers
        }
    } else {
        headers
    };

    TableStructure {
        layout_type: ai_id.layout_type.clone(),
        columns: clean_columns,
        row_categories,
        column_count,
        column_semantic: Some(ai_id.column_semantic.clone()),
        row_semantic: Some(ai_id.row_semantic.clone()),
    }
}

/// Extract time slot patterns from schedule-grid tables.
///
/// Finds time-like values in the first column (or designated time column) and
/// deduplicates them to produce the teacher's standard time block pattern.
fn extract_time_slots(tables: &[ParsedTable]) -> Vec<String> {
    let mut time_slots = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for table in tables {
        if table.rows.len() < 2 {
            continue;
        }

        let h_idx = effective_header_row(table);
        let headers: Vec<String> = table.rows[h_idx]
            .cells
            .iter()
            .map(|c| c.text.trim().to_lowercase())
            .collect();

        let expected_width = headers.len();

        // Standard orientation: time slots in the first column of data rows.
        if let Some((time_col, _)) = detect_schedule_columns(&headers) {
            for row in table.rows.iter().skip(h_idx + 1) {
                // Skip merged/section-divider rows.
                if is_merged_row(row, expected_width) {
                    continue;
                }
                if let Some(cell) = row.cells.get(time_col) {
                    let text = cell.text.trim().to_string();
                    if is_time_like(&text) && seen.insert(text.clone()) {
                        time_slots.push(text);
                    }
                }
            }
            continue;
        }

        // Transposed orientation: time slots in the header row (columns).
        if let Some((_, time_cols)) = detect_transposed_schedule(table) {
            for col_idx in &time_cols {
                if let Some(cell) = table.rows[0].cells.get(*col_idx) {
                    let text = cell.text.trim().to_string();
                    if is_time_like(&text) && seen.insert(text.clone()) {
                        time_slots.push(text);
                    }
                }
            }
        }
    }

    time_slots
}

/// Analyze content patterns in table cells.
///
/// Determines what types of content appear (links, rich formatting, etc.).
fn extract_content_patterns(html: &str, tables: &[ParsedTable]) -> ContentPatterns {
    let mut cell_content_types = Vec::new();
    let mut has_links = false;
    let mut has_rich_formatting = false;

    // Check for links in the HTML.
    if html.contains("<a ") && html.contains("href") {
        has_links = true;
    }

    // Check for rich formatting elements.
    let rich_markers = ["<strong>", "<em>", "<b>", "<i>", "<ul>", "<ol>"];
    for marker in &rich_markers {
        if html.contains(marker) {
            has_rich_formatting = true;
            break;
        }
    }

    // Infer content types from table headers.
    let mut seen_types = std::collections::HashSet::new();
    for table in tables {
        if table.rows.is_empty() {
            continue;
        }
        for cell in &table.rows[0].cells {
            let header = cell.text.trim().to_lowercase();
            let content_type = match header.as_str() {
                h if h.contains("time") || h.contains("hour") => "time",
                h if h.contains("material") || h.contains("resource") => "materials",
                h if h.contains("duration") || h.contains("length") => "duration",
                h if h.contains("link") || h.contains("url") => "links",
                h if h.contains("objective") || h.contains("goal") || h.contains("standard") => {
                    "objectives"
                }
                h if h.contains("note") => "notes",
                h if h.contains("title") || h.contains("topic") || h.contains("lesson") => {
                    "activity_name"
                }
                _ => continue,
            };
            if seen_types.insert(content_type) {
                cell_content_types.push(content_type.to_string());
            }
        }
    }

    ContentPatterns {
        cell_content_types,
        has_links,
        has_rich_formatting,
    }
}

/// Extract recurring elements (subjects, activities) that appear frequently.
///
/// Scans all non-header, non-time cells and finds values that repeat across
/// multiple rows, indicating they are standard weekly activities.
fn extract_recurring_elements(tables: &[ParsedTable]) -> RecurringElements {
    let mut activity_counts: HashMap<String, usize> = HashMap::new();
    let mut first_col_counts: HashMap<String, usize> = HashMap::new();

    for table in tables {
        if table.rows.len() < 2 {
            continue;
        }

        let h_idx = effective_header_row(table);
        let headers: Vec<String> = table.rows[h_idx]
            .cells
            .iter()
            .map(|c| c.text.trim().to_lowercase())
            .collect();

        let expected_width = headers.len();
        let is_schedule = detect_schedule_columns(&headers).is_some();
        let is_transposed = !is_schedule && detect_transposed_schedule(table).is_some();

        if is_transposed {
            // Transposed: days in rows, time columns. Activity cells are at
            // row[day_row][time_col], skipping the first column (day label).
            for row in table.rows.iter().skip(h_idx + 1) {
                if is_merged_row(row, expected_width) {
                    continue;
                }
                for (_i, cell) in row.cells.iter().enumerate().skip(1) {
                    let text = cell.text.trim().to_string();
                    if text.is_empty() || is_time_like(&text) {
                        continue;
                    }
                    let activity = text
                        .lines()
                        .next()
                        .unwrap_or(&text)
                        .trim()
                        .to_string();
                    if !activity.is_empty() && activity.len() < 60 {
                        *activity_counts.entry(activity).or_insert(0) += 1;
                    }
                }
            }
            continue;
        }

        for row in table.rows.iter().skip(h_idx + 1) {
            // Skip merged/section-divider rows.
            if is_merged_row(row, expected_width) {
                continue;
            }
            for (i, cell) in row.cells.iter().enumerate() {
                let text = cell.text.trim().to_string();
                if text.is_empty() || is_time_like(&text) {
                    continue;
                }

                // First column in non-schedule tables often has category names.
                if i == 0 && !is_schedule {
                    *first_col_counts.entry(text.clone()).or_insert(0) += 1;
                }

                // In schedule grids, day column cells are activities/subjects.
                if is_schedule && i > 0 {
                    // Normalize: take the first line or first few words as the activity name.
                    let activity = text
                        .lines()
                        .next()
                        .unwrap_or(&text)
                        .trim()
                        .to_string();
                    if !activity.is_empty() && activity.len() < 60 {
                        *activity_counts.entry(activity).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    // Filter to items appearing 2+ times (recurring).
    let mut subjects: Vec<String> = first_col_counts
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(name, _)| name)
        .collect();
    subjects.sort();

    let mut activities: Vec<String> = activity_counts
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(name, _)| name)
        .collect();
    activities.sort();

    RecurringElements {
        subjects,
        activities,
    }
}

/// Extract daily routine events — recurring activities that appear consistently
/// across most day columns at the same time slot in schedule grids.
///
/// Detection is purely frequency-based: for each time slot row, if the same activity
/// text (case-insensitive, trimmed) appears in ≥40% of day columns, it is captured as
/// a `DailyRoutineEvent` along with which specific days it occurs on.
///
/// No hardcoded keyword list is used — any recurring event is detected dynamically.
/// Classify a routine event as fixed, variable, or day-specific based on its name
/// and how many days it appears on.
fn classify_routine_event(name: &str, days: &[String], total_days: usize) -> RoutineEventType {
    let lower = name.to_lowercase();

    // Fixed: meals, breaks, transitions that are truly identical every day.
    let fixed_keywords = [
        "breakfast", "lunch", "recess", "dismissal", "snack", "pack up",
        "rest time", "nap", "arrival", "car line", "bus",
    ];
    if fixed_keywords.iter().any(|kw| lower.contains(kw)) {
        return RoutineEventType::Fixed;
    }

    // Day-specific: appears on fewer days than the total (e.g., PE on Mon/Wed only).
    if days.len() < total_days {
        // Check if it's a special/elective.
        let special_keywords = [
            "pe", "drama", "music", "art", "library", "mandarin", "spanish",
            "french", "stem", "lab", "chapel", "assembly", "gym",
        ];
        if special_keywords.iter().any(|kw| lower.contains(kw)) {
            return RoutineEventType::DaySpecific;
        }
        // Fewer days but not a recognized special — still day-specific.
        if days.len() <= total_days / 2 {
            return RoutineEventType::DaySpecific;
        }
    }

    // Variable: instructional blocks that should have different content each day.
    let variable_keywords = [
        "center", "small group", "lesson", "morning work", "journal",
        "writing", "reading", "math", "ela", "science", "social studies",
        "circle", "calendar", "phonics", "read aloud",
    ];
    if variable_keywords.iter().any(|kw| lower.contains(kw)) {
        return RoutineEventType::Variable;
    }

    // Default: if it appears every day, treat as fixed; otherwise variable.
    if days.len() >= total_days {
        RoutineEventType::Fixed
    } else {
        RoutineEventType::Variable
    }
}

fn extract_daily_routine(tables: &[ParsedTable]) -> Vec<DailyRoutineEvent> {
    let mut routine_events: Vec<DailyRoutineEvent> = Vec::new();
    let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    for table in tables {
        if table.rows.len() < 2 {
            continue;
        }

        let h_idx = effective_header_row(table);
        let headers: Vec<String> = table.rows[h_idx]
            .cells
            .iter()
            .map(|c| c.text.trim().to_lowercase())
            .collect();

        let expected_width = headers.len();

        // ── Standard orientation: days in columns, time slots in rows ──
        if let Some((time_col, day_col_pairs)) = detect_schedule_columns(&headers) {
            let num_days = day_col_pairs.len();
            if num_days < 2 {
                continue;
            }

            // Use 40% threshold so events appearing 2/5 days are captured (e.g. breakfast
            // that only appears Mon-Wed). Previously 60% missed events like these.
            let threshold = (num_days as f64 * 0.4).ceil() as usize;

            for row in table.rows.iter().skip(h_idx + 1) {
                // Skip merged/section-divider rows.
                if is_merged_row(row, expected_width) {
                    continue;
                }
                let time_slot = row
                    .cells
                    .get(time_col)
                    .map(|c| c.text.trim().to_string())
                    .filter(|t| is_time_like(t));

                // Map: lowercase activity → (display_name, days, bg_color)
                let mut activity_days: HashMap<String, (String, Vec<String>, Option<String>)> = HashMap::new();
                for &(col_idx, ref day_label) in &day_col_pairs {
                    if let Some(cell) = row.cells.get(col_idx) {
                        let activity = normalize_activity_name(&cell.text);
                        if !activity.is_empty() {
                            let key = activity.to_lowercase();
                            let entry = activity_days
                                .entry(key)
                                .or_insert_with(|| (activity.clone(), Vec::new(), cell.bg_color.clone()));
                            entry.1.push(day_label.clone());
                        }
                    }
                }

                for (_key, (display_name, days, bg_color)) in &activity_days {
                    if days.len() >= threshold {
                        // Dedup by name+time_slot so the same activity at different times
                        // is captured separately (e.g. "Recess" at 11:00 AM and 1:45 PM).
                        let dedup_key = format!(
                            "{}@{}",
                            display_name.to_lowercase(),
                            time_slot.as_deref().unwrap_or("")
                        );
                        if seen_keys.insert(dedup_key) {
                            routine_events.push(DailyRoutineEvent {
                                name: display_name.clone(),
                                time_slot: time_slot.clone(),
                                days: days.clone(),
                                bg_color: bg_color.clone(),
                                event_type: classify_routine_event(display_name, days, num_days),
                            });
                        }
                    }
                }
            }
            continue;
        }

        // ── Transposed orientation: days in rows, time slots in columns ──
        if let Some((day_row_pairs, time_cols)) = detect_transposed_schedule(table) {
            let num_days = day_row_pairs.len();
            if num_days < 2 {
                continue;
            }

            let threshold = (num_days as f64 * 0.4).ceil() as usize;

            // For each time-slot column, check if the same activity appears
            // in ≥60% of day rows at that column.
            for &time_col_idx in &time_cols {
                let time_slot = table.rows[h_idx]
                    .cells
                    .get(time_col_idx)
                    .map(|c| c.text.trim().to_string())
                    .filter(|t| is_time_like(t));

                let mut activity_days: HashMap<String, (String, Vec<String>, Option<String>)> = HashMap::new();
                for &(row_idx, ref day_label) in &day_row_pairs {
                    if let Some(cell) =
                        table.rows.get(row_idx).and_then(|r| r.cells.get(time_col_idx))
                    {
                        let activity = normalize_activity_name(&cell.text);
                        if !activity.is_empty() {
                            let key = activity.to_lowercase();
                            let entry = activity_days
                                .entry(key)
                                .or_insert_with(|| (activity.clone(), Vec::new(), cell.bg_color.clone()));
                            entry.1.push(day_label.clone());
                        }
                    }
                }

                for (_key, (display_name, days, bg_color)) in &activity_days {
                    if days.len() >= threshold {
                        let dedup_key = format!(
                            "{}@{}",
                            display_name.to_lowercase(),
                            time_slot.as_deref().unwrap_or("")
                        );
                        if seen_keys.insert(dedup_key) {
                            routine_events.push(DailyRoutineEvent {
                                name: display_name.clone(),
                                time_slot: time_slot.clone(),
                                days: days.clone(),
                                bg_color: bg_color.clone(),
                                event_type: classify_routine_event(display_name, days, num_days),
                            });
                        }
                    }
                }
            }
        }
    }

    // Sort by time slot for natural ordering.
    routine_events.sort_by(|a, b| a.time_slot.cmp(&b.time_slot));
    routine_events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_template_empty_html() {
        let template = extract_template("");
        assert!(template.table_structure.columns.is_empty());
        assert!(template.time_slots.is_empty());
        assert!(template.recurring_elements.activities.is_empty());
    }

    #[test]
    fn test_extract_template_no_tables() {
        let html = "<html><body><p>Just text, no tables</p></body></html>";
        let template = extract_template(html);
        assert!(template.table_structure.columns.is_empty());
    }

    #[test]
    fn test_extract_template_schedule_grid() {
        let html = r#"<html><body>
            <table>
                <tr><th>Day/Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:15-9:00</td><td>Math</td><td>Reading</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td>9:00-9:30</td><td>PE</td><td>Art</td><td>PE</td><td>Music</td><td>PE</td></tr>
                <tr><td>9:30-10:00</td><td>Math</td><td>Science</td><td>Math</td><td>Science</td><td>Math</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        assert_eq!(template.table_structure.layout_type, "schedule_grid");
        assert_eq!(template.table_structure.column_count, 6);
        assert_eq!(template.table_structure.columns.len(), 6);

        // Time slots should be extracted.
        assert!(template.time_slots.contains(&"8:15-9:00".to_string()));
        assert!(template.time_slots.contains(&"9:00-9:30".to_string()));
        assert!(template.time_slots.contains(&"9:30-10:00".to_string()));

        // Math and PE appear 3+ times — should be recurring.
        assert!(template.recurring_elements.activities.contains(&"Math".to_string()));
        assert!(template.recurring_elements.activities.contains(&"PE".to_string()));
    }

    #[test]
    fn test_extract_template_standard_table() {
        let html = r#"<html><body>
            <table>
                <tr><th>Title</th><th>Subject</th><th>Duration</th><th>Objectives</th></tr>
                <tr><td>Photosynthesis</td><td>Biology</td><td>45 min</td><td>Learn light reactions</td></tr>
                <tr><td>Cell Division</td><td>Biology</td><td>60 min</td><td>Learn mitosis</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        assert_eq!(template.table_structure.layout_type, "standard_table");
        assert_eq!(template.table_structure.column_count, 4);

        // Content types should include inferred types from headers.
        assert!(template.content_patterns.cell_content_types.contains(&"activity_name".to_string()));
        assert!(template.content_patterns.cell_content_types.contains(&"duration".to_string()));
        assert!(template.content_patterns.cell_content_types.contains(&"objectives".to_string()));
    }

    #[test]
    fn test_extract_colors_from_styled_html() {
        let html = r#"<html><body>
            <table>
                <tr>
                    <th style="background-color:#9900ff">Time</th>
                    <th style="background-color:#9900ff">Monday</th>
                </tr>
                <tr>
                    <td>8:00</td>
                    <td style="background-color:#00ffff">Math</td>
                </tr>
                <tr>
                    <td>9:00</td>
                    <td style="background-color:#00ffff">Reading</td>
                </tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // Should find purple as header color and cyan as activity color.
        let header_colors: Vec<&ColorMapping> = template
            .color_scheme
            .mappings
            .iter()
            .filter(|m| m.category == "header")
            .collect();
        assert!(!header_colors.is_empty());
        assert!(header_colors.iter().any(|m| m.color == "#9900ff"));

        let activity_colors: Vec<&ColorMapping> = template
            .color_scheme
            .mappings
            .iter()
            .filter(|m| m.category == "activity")
            .collect();
        assert!(!activity_colors.is_empty());
        assert!(activity_colors.iter().any(|m| m.color == "#00ffff"));
    }

    #[test]
    fn test_extract_time_slots_deduplicates() {
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th></tr>
                <tr><td>9:00-9:30</td><td>Math</td><td>Reading</td></tr>
                <tr><td>9:30-10:00</td><td>Science</td><td>Art</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        assert_eq!(template.time_slots.len(), 2);
        assert!(template.time_slots.contains(&"9:00-9:30".to_string()));
        assert!(template.time_slots.contains(&"9:30-10:00".to_string()));
    }

    #[test]
    fn test_extract_content_patterns_detects_links() {
        let html = r#"<html><body>
            <table>
                <tr><th>Title</th><th>Link</th></tr>
                <tr><td>Lesson</td><td><a href="https://example.com">Resource</a></td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        assert!(template.content_patterns.has_links);
    }

    #[test]
    fn test_extract_content_patterns_detects_rich_formatting() {
        let html = r#"<html><body>
            <table>
                <tr><th>Topic</th><th>Notes</th></tr>
                <tr><td>Math</td><td><strong>Important:</strong> <em>review</em></td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        assert!(template.content_patterns.has_rich_formatting);
    }

    #[test]
    fn test_extract_recurring_elements_filters_by_frequency() {
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th></tr>
                <tr><td>9:00</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td>10:00</td><td>PE</td><td>Unique Thing</td><td>PE</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // Math and PE appear 2+ times.
        assert!(template.recurring_elements.activities.contains(&"Math".to_string()));
        assert!(template.recurring_elements.activities.contains(&"PE".to_string()));

        // "Unique Thing" appears only once — should NOT be recurring.
        assert!(!template.recurring_elements.activities.contains(&"Unique Thing".to_string()));
    }

    #[test]
    fn test_extract_row_categories() {
        let html = r#"<html><body>
            <table>
                <tr><th>Category</th><th>Details</th></tr>
                <tr><td>Morning Circle</td><td>Welcome song</td></tr>
                <tr><td>Centers</td><td>Rotation A</td></tr>
                <tr><td>Small Group</td><td>Guided reading</td></tr>
                <tr><td>Recess</td><td>Outdoor play</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        assert!(template.table_structure.row_categories.contains(&"Morning Circle".to_string()));
        assert!(template.table_structure.row_categories.contains(&"Centers".to_string()));
        assert!(template.table_structure.row_categories.contains(&"Small Group".to_string()));
        assert!(template.table_structure.row_categories.contains(&"Recess".to_string()));
    }

    #[test]
    fn test_parse_background_color() {
        assert_eq!(
            parse_background_color("background-color:#ff0000;font-weight:bold"),
            Some("#ff0000".to_string())
        );
        assert_eq!(
            parse_background_color("background: #00ff00"),
            Some("#00ff00".to_string())
        );
        assert_eq!(parse_background_color("color:red"), None);
        assert_eq!(
            parse_background_color("background-color:transparent"),
            None
        );
    }

    #[test]
    fn test_template_schema_serialization() {
        let schema = TeachingTemplateSchema {
            color_scheme: ColorScheme {
                mappings: vec![ColorMapping {
                    color: "#9900ff".to_string(),
                    category: "header".to_string(),
                    frequency: 5,
                }],
            },
            table_structure: TableStructure {
                layout_type: "schedule_grid".to_string(),
                columns: vec!["Time".to_string(), "Monday".to_string()],
                row_categories: vec!["Math".to_string()],
                column_count: 2,
                column_semantic: Some("days_of_week".to_string()),
                row_semantic: Some("time_slots".to_string()),
            },
            time_slots: vec!["9:00-9:30".to_string()],
            content_patterns: ContentPatterns {
                cell_content_types: vec!["activity_name".to_string()],
                has_links: false,
                has_rich_formatting: true,
            },
            recurring_elements: RecurringElements {
                subjects: vec!["Math".to_string()],
                activities: vec!["Reading Group".to_string()],
            },
            daily_routine: vec![DailyRoutineEvent {
                name: "Lunch".to_string(),
                time_slot: Some("12:00-12:30".to_string()),
                days: vec!["Monday".to_string(), "Tuesday".to_string(), "Wednesday".to_string(), "Thursday".to_string(), "Friday".to_string()],
                bg_color: None,
                event_type: RoutineEventType::Fixed,
            }],
        };

        let json = serde_json::to_string(&schema).unwrap();
        let deserialized: TeachingTemplateSchema = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.table_structure.layout_type, "schedule_grid");
        assert_eq!(deserialized.color_scheme.mappings.len(), 1);
        assert_eq!(deserialized.time_slots.len(), 1);
        assert_eq!(deserialized.recurring_elements.subjects.len(), 1);
        assert_eq!(deserialized.daily_routine.len(), 1);
        assert_eq!(deserialized.daily_routine[0].name, "Lunch");
    }

    #[test]
    fn test_template_schema_default_deserialization() {
        // An empty JSON object should deserialize to all defaults.
        let schema: TeachingTemplateSchema = serde_json::from_str("{}").unwrap();
        assert!(schema.color_scheme.mappings.is_empty());
        assert!(schema.table_structure.columns.is_empty());
        assert!(schema.time_slots.is_empty());
        assert!(schema.recurring_elements.activities.is_empty());
        assert!(schema.daily_routine.is_empty());
    }

    // ── Daily Routine Extraction Tests ──────────────────────────

    // Note: is_routine_activity() and ROUTINE_KEYWORDS have been removed.
    // Detection is now purely frequency-based — any activity appearing in ≥60% of
    // day columns at the same time slot is detected as a recurring event.

    #[test]
    fn test_extract_daily_routine_schedule_with_lunch_and_recess() {
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:00-8:45</td><td>Math</td><td>Math</td><td>Math</td><td>Math</td><td>Math</td></tr>
                <tr><td>8:45-9:30</td><td>Reading</td><td>Reading</td><td>Reading</td><td>Reading</td><td>Reading</td></tr>
                <tr><td>9:30-10:00</td><td>Recess</td><td>Recess</td><td>Recess</td><td>Recess</td><td>Recess</td></tr>
                <tr><td>10:00-10:45</td><td>Science</td><td>Writing</td><td>Science</td><td>Writing</td><td>Science</td></tr>
                <tr><td>11:00-11:30</td><td>Lunch</td><td>Lunch</td><td>Lunch</td><td>Lunch</td><td>Lunch</td></tr>
                <tr><td>11:30-12:15</td><td>Social Studies</td><td>Social Studies</td><td>Social Studies</td><td>Social Studies</td><td>Social Studies</td></tr>
                <tr><td>12:15-1:00</td><td>Specials</td><td>Specials</td><td>Specials</td><td>Specials</td><td>Specials</td></tr>
                <tr><td>2:30-2:45</td><td>Dismissal</td><td>Dismissal</td><td>Dismissal</td><td>Dismissal</td><td>Dismissal</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // Frequency-based detection: any activity in ≥40% of day columns is detected.
        let routine_names: Vec<&str> = template.daily_routine.iter().map(|e| e.name.as_str()).collect();
        assert!(routine_names.contains(&"Recess"), "Expected Recess in routine: {:?}", routine_names);
        assert!(routine_names.contains(&"Lunch"), "Expected Lunch in routine: {:?}", routine_names);
        assert!(routine_names.contains(&"Specials"), "Expected Specials in routine: {:?}", routine_names);
        assert!(routine_names.contains(&"Dismissal"), "Expected Dismissal in routine: {:?}", routine_names);
        // Academic subjects appearing every day are also detected as recurring.
        assert!(routine_names.contains(&"Math"), "Expected Math in routine: {:?}", routine_names);
        assert!(routine_names.contains(&"Reading"), "Expected Reading in routine: {:?}", routine_names);
        assert!(routine_names.contains(&"Social Studies"), "Expected Social Studies in routine: {:?}", routine_names);

        // Science appears 3/5 = 60% — meets threshold.
        assert!(routine_names.contains(&"Science"), "Expected Science in routine: {:?}", routine_names);
        // Writing appears 2/5 = 40% — meets 40% threshold.
        assert!(routine_names.contains(&"Writing"), "Expected Writing in routine (2/5 = 40%%): {:?}", routine_names);

        // Verify time slots are captured.
        let recess = template.daily_routine.iter().find(|e| e.name == "Recess").unwrap();
        assert_eq!(recess.time_slot, Some("9:30-10:00".to_string()));

        let lunch = template.daily_routine.iter().find(|e| e.name == "Lunch").unwrap();
        assert_eq!(lunch.time_slot, Some("11:00-11:30".to_string()));

        // Verify days are tracked.
        assert_eq!(recess.days.len(), 5, "Recess should list 5 days");
        assert_eq!(lunch.days.len(), 5, "Lunch should list 5 days");

        let science = template.daily_routine.iter().find(|e| e.name == "Science").unwrap();
        assert_eq!(science.days.len(), 3, "Science should list 3 days");
    }

    #[test]
    fn test_extract_daily_routine_no_recurring_events() {
        // A schedule grid where no activity appears in ≥40% of day columns.
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th></tr>
                <tr><td>9:00</td><td>Math</td><td>Reading</td><td>Science</td></tr>
                <tr><td>10:00</td><td>Art</td><td>Music</td><td>Writing</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        // Each activity appears 1/3 = 33%, below 40% threshold.
        assert!(template.daily_routine.is_empty());
    }

    #[test]
    fn test_extract_daily_routine_partial_coverage() {
        // Recess appears in only 2 out of 5 days — meets 40% threshold.
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>9:30-10:00</td><td>Recess</td><td>Recess</td><td>Math</td><td>Science</td><td>Reading</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        // Recess in 2/5 days = 40%, meets threshold.
        let routine_names: Vec<&str> = template.daily_routine.iter().map(|e| e.name.as_str()).collect();
        assert!(routine_names.contains(&"Recess"), "2/5 = 40%% should meet threshold: {:?}", routine_names);

        let recess = template.daily_routine.iter().find(|e| e.name == "Recess").unwrap();
        assert_eq!(recess.days.len(), 2, "Recess should list 2 days");
    }

    #[test]
    fn test_extract_daily_routine_meets_threshold() {
        // Recess appears in 3 out of 5 days — meets 40% threshold.
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>9:30-10:00</td><td>Recess</td><td>Recess</td><td>Recess</td><td>Science</td><td>Reading</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        let routine_names: Vec<&str> = template.daily_routine.iter().map(|e| e.name.as_str()).collect();
        assert!(routine_names.contains(&"Recess"), "3/5 = 60%% should meet threshold: {:?}", routine_names);

        // Verify days are tracked correctly.
        let recess = template.daily_routine.iter().find(|e| e.name == "Recess").unwrap();
        assert_eq!(recess.days.len(), 3, "Recess should list 3 days");
        assert!(recess.days.contains(&"Monday".to_string()));
        assert!(recess.days.contains(&"Tuesday".to_string()));
        assert!(recess.days.contains(&"Wednesday".to_string()));
    }

    #[test]
    fn test_extract_daily_routine_standard_table_ignored() {
        // Standard tables (non-schedule grids) should not produce daily_routine events.
        let html = r#"<html><body>
            <table>
                <tr><th>Title</th><th>Subject</th><th>Duration</th></tr>
                <tr><td>Lunch Break</td><td>N/A</td><td>30 min</td></tr>
                <tr><td>Recess</td><td>N/A</td><td>15 min</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        assert!(template.daily_routine.is_empty());
    }

    #[test]
    fn test_extract_daily_routine_deduplicates() {
        // Same routine event at same time should only appear once.
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th></tr>
                <tr><td>11:30-12:00</td><td>Lunch</td><td>Lunch</td><td>Lunch</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        let lunch_count = template.daily_routine.iter().filter(|e| e.name == "Lunch").count();
        assert_eq!(lunch_count, 1, "Lunch should appear exactly once");
    }

    #[test]
    fn test_extract_daily_routine_sorted_by_time() {
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th></tr>
                <tr><td>12:00-12:30</td><td>Lunch</td><td>Lunch</td><td>Lunch</td></tr>
                <tr><td>8:00-8:15</td><td>Morning Meeting</td><td>Morning Meeting</td><td>Morning Meeting</td></tr>
                <tr><td>10:00-10:15</td><td>Recess</td><td>Recess</td><td>Recess</td></tr>
                <tr><td>2:45-3:00</td><td>Dismissal</td><td>Dismissal</td><td>Dismissal</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        assert!(template.daily_routine.len() >= 4);

        // Should be sorted by time slot.
        let time_slots: Vec<Option<String>> = template
            .daily_routine
            .iter()
            .map(|e| e.time_slot.clone())
            .collect();
        let mut sorted = time_slots.clone();
        sorted.sort();
        assert_eq!(time_slots, sorted, "Daily routine should be sorted by time slot");
    }

    #[test]
    fn test_extract_daily_routine_empty_html() {
        let template = extract_template("");
        assert!(template.daily_routine.is_empty());
    }

    #[test]
    fn test_extract_daily_routine_realistic_elementary_schedule() {
        let html = r#"<html><body>
            <table>
                <tr><th>Day/Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>7:45-8:00</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td></tr>
                <tr><td>8:00-8:15</td><td>Morning Meeting</td><td>Morning Meeting</td><td>Morning Meeting</td><td>Morning Meeting</td><td>Morning Meeting</td></tr>
                <tr><td>8:15-9:00</td><td>Math Workshop</td><td>Math Workshop</td><td>Math Workshop</td><td>Math Workshop</td><td>Math Assessment</td></tr>
                <tr><td>9:00-9:45</td><td>Reading Block</td><td>Reading Block</td><td>Reading Block</td><td>Reading Block</td><td>Reading Block</td></tr>
                <tr><td>9:45-10:00</td><td>Snack</td><td>Snack</td><td>Snack</td><td>Snack</td><td>Snack</td></tr>
                <tr><td>10:00-10:15</td><td>Recess</td><td>Recess</td><td>Recess</td><td>Recess</td><td>Recess</td></tr>
                <tr><td>10:15-11:00</td><td>Writing</td><td>Writing</td><td>Writing</td><td>Writing</td><td>Writing</td></tr>
                <tr><td>11:00-11:45</td><td>Art</td><td>Music</td><td>PE</td><td>Library</td><td>Art</td></tr>
                <tr><td>11:45-12:15</td><td>Lunch</td><td>Lunch</td><td>Lunch</td><td>Lunch</td><td>Lunch</td></tr>
                <tr><td>12:15-12:30</td><td>Recess</td><td>Recess</td><td>Recess</td><td>Recess</td><td>Recess</td></tr>
                <tr><td>12:30-1:15</td><td>Science</td><td>Social Studies</td><td>Science</td><td>Social Studies</td><td>Science</td></tr>
                <tr><td>1:15-1:30</td><td>Pack Up</td><td>Pack Up</td><td>Pack Up</td><td>Pack Up</td><td>Pack Up</td></tr>
                <tr><td>1:30</td><td>Dismissal</td><td>Dismissal</td><td>Dismissal</td><td>Dismissal</td><td>Dismissal</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        let routine_names: Vec<&str> = template.daily_routine.iter().map(|e| e.name.as_str()).collect();

        // All recurring events should be detected (frequency-based, no keyword filter).
        assert!(routine_names.contains(&"Breakfast"), "Missing Breakfast: {:?}", routine_names);
        assert!(routine_names.contains(&"Morning Meeting"), "Missing Morning Meeting: {:?}", routine_names);
        assert!(routine_names.contains(&"Snack"), "Missing Snack: {:?}", routine_names);
        assert!(routine_names.contains(&"Lunch"), "Missing Lunch: {:?}", routine_names);
        assert!(routine_names.contains(&"Pack Up"), "Missing Pack Up: {:?}", routine_names);
        assert!(routine_names.contains(&"Dismissal"), "Missing Dismissal: {:?}", routine_names);

        // Recess appears at two different times — both should be captured as separate events
        // since they represent distinct time slots in the daily routine.
        let recess_count = template.daily_routine.iter().filter(|e| e.name == "Recess").count();
        assert_eq!(recess_count, 2, "Recess at two different time slots should produce 2 entries");
        let recess_times: Vec<&Option<String>> = template.daily_routine.iter()
            .filter(|e| e.name == "Recess")
            .map(|e| &e.time_slot)
            .collect();
        assert!(recess_times.contains(&&Some("10:00-10:15".to_string())),
            "Missing Recess at 10:00-10:15: {:?}", recess_times);
        assert!(recess_times.contains(&&Some("12:15-12:30".to_string())),
            "Missing Recess at 12:15-12:30: {:?}", recess_times);

        // Academic subjects appearing in ≥40% of days are also detected as recurring.
        // Math Workshop appears 4/5 = 80%, Reading Block 5/5 = 100%, Writing 5/5 = 100%.
        assert!(routine_names.contains(&"Math Workshop"), "Missing Math Workshop: {:?}", routine_names);
        assert!(routine_names.contains(&"Reading Block"), "Missing Reading Block: {:?}", routine_names);
        assert!(routine_names.contains(&"Writing"), "Missing Writing: {:?}", routine_names);

        // Science appears 3/5 = 60%, Social Studies 2/5 = 40% — both meet 40% threshold.
        assert!(routine_names.contains(&"Science"), "Missing Science (3/5 = 60%%): {:?}", routine_names);
        assert!(routine_names.contains(&"Social Studies"), "Missing Social Studies (2/5 = 40%%): {:?}", routine_names);

        // The specials row has different activities each day (Art, Music, PE, Library) —
        // Art appears 2/5 = 40% — meets threshold now.
        assert!(routine_names.contains(&"Art"), "Missing Art (2/5 = 40%%): {:?}", routine_names);

        // Verify days are tracked on Breakfast (all 5 days).
        let breakfast = template.daily_routine.iter().find(|e| e.name == "Breakfast").unwrap();
        assert_eq!(breakfast.days.len(), 5, "Breakfast should list 5 days");
    }

    // ── Year Range Detection Tests ──────────────────────────────

    #[test]
    fn test_contains_year_range() {
        assert!(contains_year_range("lp 2022-2023"));
        assert!(contains_year_range("LP 2023/2024"));
        assert!(contains_year_range("2022-23"));
        assert!(contains_year_range("Eureka Math 2021-2022"));
        assert!(!contains_year_range("Monday"));
        assert!(!contains_year_range("8:15-9:00"));
        assert!(!contains_year_range("Grade 3"));
        assert!(!contains_year_range(""));
    }

    // ── Table Scoring Tests ──────────────────────────────────────

    #[test]
    fn test_score_planning_table_prefers_schedule_grid() {
        let schedule_html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:00-8:45</td><td>Math</td><td>Reading</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td>9:00-9:30</td><td>PE</td><td>Art</td><td>PE</td><td>Music</td><td>PE</td></tr>
            </table>
        </body></html>"#;
        let schedule_tables = parser::extract_tables(schedule_html);

        let reference_html = r#"<html><body>
            <table>
                <tr><th>LP 2022-2023</th><th>LP 2023/2024</th><th>Eureka Math</th></tr>
                <tr><td>Unit 1</td><td>Module 1</td><td>Chapter 1</td></tr>
                <tr><td>Unit 2</td><td>Module 2</td><td>Chapter 2</td></tr>
                <tr><td>Unit 3</td><td>Module 3</td><td>Chapter 3</td></tr>
                <tr><td>Unit 4</td><td>Module 4</td><td>Chapter 4</td></tr>
            </table>
        </body></html>"#;
        let reference_tables = parser::extract_tables(reference_html);

        let schedule_score = score_planning_table(&schedule_tables[0]);
        let reference_score = score_planning_table(&reference_tables[0]);

        assert!(
            schedule_score > reference_score,
            "Schedule grid (score={}) should score higher than reference table (score={})",
            schedule_score,
            reference_score
        );
    }

    #[test]
    fn test_extract_table_structure_picks_schedule_over_reference() {
        // HTML with two tables: a reference table (larger) and a schedule grid (smaller).
        let html = r#"<html><body>
            <table>
                <tr><th>LP 2022-2023</th><th>LP 2023/2024</th><th>Eureka Math</th></tr>
                <tr><td>Unit 1</td><td>Module 1</td><td>Chapter 1</td></tr>
                <tr><td>Unit 2</td><td>Module 2</td><td>Chapter 2</td></tr>
                <tr><td>Unit 3</td><td>Module 3</td><td>Chapter 3</td></tr>
                <tr><td>Unit 4</td><td>Module 4</td><td>Chapter 4</td></tr>
                <tr><td>Unit 5</td><td>Module 5</td><td>Chapter 5</td></tr>
                <tr><td>Unit 6</td><td>Module 6</td><td>Chapter 6</td></tr>
            </table>
            <table>
                <tr><th>Day/Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:15-9:00</td><td>Math</td><td>Reading</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td>9:00-9:30</td><td>Centers</td><td>Writing</td><td>Centers</td><td>Writing</td><td>Centers</td></tr>
            </table>
        </body></html>"#;

        let tables = parser::extract_tables(html);
        assert_eq!(tables.len(), 2, "Should find 2 tables");

        let structure = extract_table_structure(&tables);
        // Should pick the schedule grid, not the larger reference table.
        assert_eq!(structure.layout_type, "schedule_grid");
        assert_eq!(structure.column_count, 6);
        assert!(structure.columns.contains(&"Monday".to_string()));
        assert!(!structure.columns.contains(&"LP 2022-2023".to_string()));
    }

    #[test]
    fn test_archive_table_with_many_rows_never_wins() {
        // Even with 30+ rows, an archive table must never beat a small schedule grid.
        let mut archive_rows = String::new();
        for i in 1..=35 {
            archive_rows.push_str(&format!(
                "<tr><td>Unit {i}</td><td>Module {i}</td><td>Chapter {i}</td></tr>\n"
            ));
        }
        let html = format!(
            r#"<html><body>
            <table>
                <tr><th>LP 2022-2023</th><th>LP 2023/2024</th><th>Eureka Math</th></tr>
                {archive_rows}
            </table>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:15-9:00</td><td>Math</td><td>Reading</td><td>Math</td><td>Reading</td><td>Math</td></tr>
            </table>
        </body></html>"#
        );

        let tables = parser::extract_tables(&html);
        assert_eq!(tables.len(), 2);

        let archive_score = score_planning_table(&tables[0]);
        let schedule_score = score_planning_table(&tables[1]);
        assert!(
            schedule_score > archive_score,
            "Schedule grid (score={}) MUST beat archive table (score={}) even with 35 rows",
            schedule_score,
            archive_score
        );

        let structure = extract_table_structure(&tables);
        assert_eq!(structure.layout_type, "schedule_grid");
        assert!(structure.columns.contains(&"Monday".to_string()));
    }

    #[test]
    fn test_heuristic_selects_correct_table_with_daily_routine() {
        // When both archive and schedule tables exist, daily routine should come
        // from the schedule table (which has day-of-week columns).
        let html = r#"<html><body>
            <table>
                <tr><th>LP 2022-2023</th><th>LP 2023/2024</th><th>Eureka Math</th></tr>
                <tr><td>Unit 1</td><td>Module 1</td><td>Chapter 1</td></tr>
            </table>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:00-8:30</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td></tr>
                <tr><td>8:30-9:15</td><td>Math</td><td>Reading</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td>11:00-11:30</td><td>Lunch</td><td>Lunch</td><td>Lunch</td><td>Lunch</td><td>Lunch</td></tr>
                <tr><td>11:30-12:00</td><td>Recess</td><td>Recess</td><td>Recess</td><td>Recess</td><td>Recess</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // Template should have schedule grid columns, not archive columns.
        assert_eq!(template.table_structure.layout_type, "schedule_grid");
        assert!(template.table_structure.columns.contains(&"Monday".to_string()));
        assert!(!template.table_structure.columns.iter().any(|c| c.contains("2022")));

        // Daily routine should be extracted from the schedule table.
        let routine_names: Vec<&str> = template.daily_routine.iter().map(|e| e.name.as_str()).collect();
        assert!(routine_names.contains(&"Breakfast"), "Missing Breakfast: {:?}", routine_names);
        assert!(routine_names.contains(&"Lunch"), "Missing Lunch: {:?}", routine_names);
        assert!(routine_names.contains(&"Recess"), "Missing Recess: {:?}", routine_names);
    }

    // ── Semantic Labels Tests ────────────────────────────────────

    #[test]
    fn test_semantic_labels_schedule_grid() {
        let html = r#"<html><body>
            <table>
                <tr><th>Day/Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:15-9:00</td><td>Math</td><td>Reading</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td>9:00-9:30</td><td>PE</td><td>Art</td><td>PE</td><td>Music</td><td>PE</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        assert_eq!(template.table_structure.column_semantic, Some("days_of_week".to_string()));
        assert_eq!(template.table_structure.row_semantic, Some("time_slots".to_string()));
    }

    #[test]
    fn test_semantic_labels_standard_table() {
        let html = r#"<html><body>
            <table>
                <tr><th>Title</th><th>Subject</th><th>Duration</th></tr>
                <tr><td>Photosynthesis</td><td>Biology</td><td>45 min</td></tr>
                <tr><td>Cell Division</td><td>Biology</td><td>60 min</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        assert_eq!(template.table_structure.column_semantic, None);
        assert_eq!(template.table_structure.row_semantic, Some("categories".to_string()));
    }

    #[test]
    fn test_score_planning_table_empty() {
        let empty_table = ParsedTable { rows: vec![] };
        assert_eq!(score_planning_table(&empty_table), 0);
    }

    // ── AI Table Identification Tests ────────────────────────────

    use crate::chat::provider::{CompletionMessage, ProviderInfo, ModelInfo, TokenCallback};
    use std::sync::{Arc, Mutex as StdMutex};

    /// Mock AI provider for testing. Returns pre-configured responses.
    struct MockAiProvider {
        response: String,
        calls: Arc<StdMutex<Vec<Vec<CompletionMessage>>>>,
    }

    impl MockAiProvider {
        fn new(response: &str) -> Self {
            Self {
                response: response.to_string(),
                calls: Arc::new(StdMutex::new(Vec::new())),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    #[async_trait::async_trait]
    impl crate::chat::provider::AiProvider for MockAiProvider {
        async fn complete(
            &self,
            messages: &[CompletionMessage],
            _max_tokens: u32,
            _temperature: f32,
        ) -> Result<String, crate::errors::ChalkError> {
            self.calls.lock().unwrap().push(messages.to_vec());
            Ok(self.response.clone())
        }

        async fn complete_stream(
            &self,
            _messages: &[CompletionMessage],
            _max_tokens: u32,
            _temperature: f32,
            _on_token: TokenCallback,
        ) -> Result<String, crate::errors::ChalkError> {
            Ok(self.response.clone())
        }

        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                id: "mock".into(),
                display_name: "Mock".into(),
                models: vec![ModelInfo {
                    id: "mock-model".into(),
                    display_name: "Mock Model".into(),
                    description: "For testing".into(),
                }],
            }
        }

        fn model(&self) -> &str {
            "mock-model"
        }
    }

    /// Mock provider that returns an error.
    struct ErrorAiProvider;

    #[async_trait::async_trait]
    impl crate::chat::provider::AiProvider for ErrorAiProvider {
        async fn complete(
            &self,
            _messages: &[CompletionMessage],
            _max_tokens: u32,
            _temperature: f32,
        ) -> Result<String, crate::errors::ChalkError> {
            Err(crate::errors::ChalkError::new(
                crate::errors::ErrorDomain::Digest,
                crate::errors::ErrorCode::DigestParseFailed,
                "API call failed",
            ))
        }

        async fn complete_stream(
            &self,
            _messages: &[CompletionMessage],
            _max_tokens: u32,
            _temperature: f32,
            _on_token: TokenCallback,
        ) -> Result<String, crate::errors::ChalkError> {
            Err(crate::errors::ChalkError::new(
                crate::errors::ErrorDomain::Digest,
                crate::errors::ErrorCode::DigestParseFailed,
                "API call failed",
            ))
        }

        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                id: "error".into(),
                display_name: "Error".into(),
                models: vec![],
            }
        }

        fn model(&self) -> &str {
            "error-model"
        }
    }

    #[test]
    fn test_format_tables_for_ai() {
        let html = r#"<html><body>
            <table>
                <tr><th>LP 2022-2023</th><th>LP 2023/2024</th><th>Eureka Math</th></tr>
                <tr><td>Unit 1</td><td>Module 1</td><td>Chapter 1</td></tr>
                <tr><td>Unit 2</td><td>Module 2</td><td>Chapter 2</td></tr>
            </table>
            <table>
                <tr><th>Day/Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th></tr>
                <tr><td>9:00-9:30</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td>9:30-10:00</td><td>PE</td><td>Art</td><td>PE</td></tr>
            </table>
        </body></html>"#;

        let tables = parser::extract_tables(html);
        let summary = format_tables_for_ai(&tables);

        // Should reference both tables.
        assert!(summary.contains("Table 0"), "Should list Table 0");
        assert!(summary.contains("Table 1"), "Should list Table 1");

        // Should include headers.
        assert!(summary.contains("LP 2022-2023"), "Table 0 headers");
        assert!(summary.contains("Monday"), "Table 1 headers");

        // Should include data rows.
        assert!(summary.contains("Unit 1"), "Table 0 data");
        assert!(summary.contains("Math"), "Table 1 data");
    }

    #[test]
    fn test_format_tables_for_ai_truncates_long_cells() {
        let long_text = "A".repeat(100);
        let html = format!(
            r#"<html><body><table>
                <tr><th>{}</th></tr>
                <tr><td>Short</td></tr>
            </table></body></html>"#,
            long_text
        );

        let tables = parser::extract_tables(&html);
        let summary = format_tables_for_ai(&tables);

        // Long text should be truncated to 60 chars.
        assert!(!summary.contains(&long_text));
        assert!(summary.contains(&"A".repeat(60)));
    }

    #[test]
    fn test_format_tables_for_ai_shows_row_count() {
        let html = r#"<html><body>
            <table>
                <tr><th>A</th></tr>
                <tr><td>1</td></tr>
                <tr><td>2</td></tr>
                <tr><td>3</td></tr>
                <tr><td>4</td></tr>
                <tr><td>5</td></tr>
            </table>
        </body></html>"#;

        let tables = parser::extract_tables(html);
        let summary = format_tables_for_ai(&tables);

        // Should show "6 rows" and "2 more rows" (only shows 4 of 6).
        assert!(summary.contains("6 rows"));
        assert!(summary.contains("2 more rows"));
    }

    #[tokio::test]
    async fn test_identify_planning_table_with_ai_valid_response() {
        let provider = MockAiProvider::new(
            r#"{"table_index": 1, "column_semantic": "days_of_week", "row_semantic": "time_slots", "layout_type": "schedule_grid"}"#,
        );

        let html = r#"<html><body>
            <table><tr><th>LP 2022</th><th>LP 2023</th></tr><tr><td>A</td><td>B</td></tr></table>
            <table><tr><th>Time</th><th>Monday</th><th>Tuesday</th></tr><tr><td>9:00</td><td>Math</td><td>Reading</td></tr></table>
        </body></html>"#;
        let tables = parser::extract_tables(html);

        let result = identify_planning_table_with_ai(&provider, &tables).await;
        assert!(result.is_ok());

        let id = result.unwrap();
        assert_eq!(id.table_index, 1);
        assert_eq!(id.column_semantic, "days_of_week");
        assert_eq!(id.row_semantic, "time_slots");
        assert_eq!(id.layout_type, "schedule_grid");

        // Should have made exactly one API call.
        assert_eq!(provider.call_count(), 1);
    }

    #[tokio::test]
    async fn test_identify_planning_table_with_ai_json_in_code_fence() {
        let provider = MockAiProvider::new(
            "```json\n{\"table_index\": 0, \"column_semantic\": \"lesson_attributes\", \"row_semantic\": \"lessons\", \"layout_type\": \"standard_table\"}\n```",
        );

        let html = r#"<html><body>
            <table><tr><th>Title</th><th>Subject</th></tr><tr><td>Lesson 1</td><td>Math</td></tr></table>
        </body></html>"#;
        let tables = parser::extract_tables(html);

        let result = identify_planning_table_with_ai(&provider, &tables).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().table_index, 0);
    }

    #[tokio::test]
    async fn test_identify_planning_table_with_ai_invalid_json() {
        let provider = MockAiProvider::new("I think table 1 is the planning template.");

        let html = r#"<html><body>
            <table><tr><th>A</th></tr><tr><td>B</td></tr></table>
        </body></html>"#;
        let tables = parser::extract_tables(html);

        let result = identify_planning_table_with_ai(&provider, &tables).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_identify_planning_table_with_ai_invalid_index() {
        let provider = MockAiProvider::new(
            r#"{"table_index": 99, "column_semantic": "days", "row_semantic": "time", "layout_type": "schedule_grid"}"#,
        );

        let html = r#"<html><body>
            <table><tr><th>A</th></tr><tr><td>B</td></tr></table>
        </body></html>"#;
        let tables = parser::extract_tables(html);

        let result = identify_planning_table_with_ai(&provider, &tables).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("table_index 99"));
    }

    #[tokio::test]
    async fn test_identify_planning_table_with_ai_api_error() {
        let provider = ErrorAiProvider;

        let html = r#"<html><body>
            <table><tr><th>A</th></tr><tr><td>B</td></tr></table>
        </body></html>"#;
        let tables = parser::extract_tables(html);

        let result = identify_planning_table_with_ai(&provider, &tables).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_table_structure_with_ai() {
        let html = r#"<html><body>
            <table><tr><th>LP 2022-2023</th><th>LP 2023/2024</th><th>Eureka Math</th></tr>
                <tr><td>Unit 1</td><td>Module 1</td><td>Chapter 1</td></tr></table>
            <table><tr><th>Day/Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th></tr>
                <tr><td>9:00-9:30</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td>9:30-10:00</td><td>PE</td><td>Art</td><td>PE</td></tr></table>
        </body></html>"#;
        let tables = parser::extract_tables(html);

        let ai_id = AiTableIdentification {
            table_index: 1,
            column_semantic: "days_of_week".to_string(),
            row_semantic: "time_slots".to_string(),
            layout_type: "schedule_grid".to_string(),
        };

        let structure = extract_table_structure_with_ai(&tables, &ai_id);

        assert_eq!(structure.layout_type, "schedule_grid");
        assert_eq!(structure.column_semantic, Some("days_of_week".to_string()));
        assert_eq!(structure.row_semantic, Some("time_slots".to_string()));
        assert!(structure.columns.contains(&"Monday".to_string()));
        // Should NOT contain reference table columns.
        assert!(!structure.columns.iter().any(|c| c.contains("2022")));
    }

    #[test]
    fn test_extract_table_structure_with_ai_cleans_dirty_headers() {
        // Simulate real-world headers: day names with dates, notes, and link text.
        let html = r#"<html><body>
            <table>
                <tr>
                    <th>Day/Time LP 2022-2023 LP 2023/2024 LP 2024/2025 TK Long Term Plan</th>
                    <th>Monday 8:15-3:05 8/11 PD NO SCHOOL</th>
                    <th>Tuesday 8:15-3:05 8/12 First Day of School 8:15-12:30</th>
                    <th>Wednesday 8:15-3:05 8/13</th>
                    <th>Thursday 8:15-3:05 8/14</th>
                    <th>Friday 8:15-3:05 8/15</th>
                </tr>
                <tr><td>9:00-9:30</td><td>Math</td><td>Reading</td><td>Math</td><td>Reading</td><td>Math</td></tr>
            </table>
        </body></html>"#;
        let tables = parser::extract_tables(html);

        let ai_id = AiTableIdentification {
            table_index: 0,
            column_semantic: "days_of_week".to_string(),
            row_semantic: "time_slots".to_string(),
            layout_type: "schedule_grid".to_string(),
        };

        let structure = extract_table_structure_with_ai(&tables, &ai_id);

        assert_eq!(structure.layout_type, "schedule_grid");
        // Headers should be cleaned to canonical day names.
        assert_eq!(structure.columns[0], "Day/Time");
        assert_eq!(structure.columns[1], "Monday");
        assert_eq!(structure.columns[2], "Tuesday");
        assert_eq!(structure.columns[3], "Wednesday");
        assert_eq!(structure.columns[4], "Thursday");
        assert_eq!(structure.columns[5], "Friday");
        // Should NOT contain raw text like dates or notes.
        assert!(!structure.columns.iter().any(|c| c.contains("8/11")));
        assert!(!structure.columns.iter().any(|c| c.contains("PD NO SCHOOL")));
        assert!(!structure.columns.iter().any(|c| c.contains("LP 2022")));
    }

    #[test]
    fn test_extract_table_structure_with_ai_out_of_range_fallback() {
        let html = r#"<html><body>
            <table><tr><th>Time</th><th>Monday</th><th>Tuesday</th></tr>
                <tr><td>9:00</td><td>Math</td><td>Reading</td></tr></table>
        </body></html>"#;
        let tables = parser::extract_tables(html);

        let ai_id = AiTableIdentification {
            table_index: 99, // out of range
            column_semantic: "days_of_week".to_string(),
            row_semantic: "time_slots".to_string(),
            layout_type: "schedule_grid".to_string(),
        };

        // Should fall back to heuristic.
        let structure = extract_table_structure_with_ai(&tables, &ai_id);
        assert!(!structure.columns.is_empty());
    }

    #[tokio::test]
    async fn test_extract_template_with_ai_selects_correct_table() {
        // AI correctly identifies table 1 as the schedule grid.
        let provider = MockAiProvider::new(
            r#"{"table_index": 1, "column_semantic": "days_of_week", "row_semantic": "time_slots", "layout_type": "schedule_grid"}"#,
        );

        let html = r#"<html><body>
            <table>
                <tr><th>LP 2022-2023</th><th>LP 2023/2024</th><th>Eureka Math</th></tr>
                <tr><td>Unit 1</td><td>Module 1</td><td>Chapter 1</td></tr>
                <tr><td>Unit 2</td><td>Module 2</td><td>Chapter 2</td></tr>
                <tr><td>Unit 3</td><td>Module 3</td><td>Chapter 3</td></tr>
                <tr><td>Unit 4</td><td>Module 4</td><td>Chapter 4</td></tr>
                <tr><td>Unit 5</td><td>Module 5</td><td>Chapter 5</td></tr>
                <tr><td>Unit 6</td><td>Module 6</td><td>Chapter 6</td></tr>
            </table>
            <table>
                <tr><th>Day/Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:15-9:00</td><td>Math</td><td>Reading</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td>9:00-9:30</td><td>Centers</td><td>Writing</td><td>Centers</td><td>Writing</td><td>Centers</td></tr>
            </table>
        </body></html>"#;

        let (template, method) = extract_template_with_ai(html, &provider).await;

        assert_eq!(method, "ai");
        // AI-selected table should be the schedule grid.
        assert_eq!(template.table_structure.layout_type, "schedule_grid");
        assert_eq!(template.table_structure.column_semantic, Some("days_of_week".to_string()));
        assert_eq!(template.table_structure.row_semantic, Some("time_slots".to_string()));
        assert!(template.table_structure.columns.contains(&"Monday".to_string()));
        // Should NOT contain reference table columns.
        assert!(!template.table_structure.columns.iter().any(|c| c.contains("2022")));
    }

    #[tokio::test]
    async fn test_extract_template_with_ai_falls_back_on_error() {
        let provider = ErrorAiProvider;

        let html = r#"<html><body>
            <table>
                <tr><th>LP 2022-2023</th><th>LP 2023/2024</th></tr>
                <tr><td>Unit 1</td><td>Module 1</td></tr>
            </table>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th></tr>
                <tr><td>9:00-9:30</td><td>Math</td><td>Reading</td><td>Math</td></tr>
            </table>
        </body></html>"#;

        // Should still produce a valid template using heuristic fallback.
        let (template, method) = extract_template_with_ai(html, &provider).await;
        assert_eq!(method, "heuristic");
        assert!(!template.table_structure.columns.is_empty());
    }

    #[tokio::test]
    async fn test_extract_template_with_ai_single_table_skips_ai() {
        let provider = MockAiProvider::new(
            r#"{"table_index": 0, "column_semantic": "test", "row_semantic": "test", "layout_type": "test"}"#,
        );

        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th></tr>
                <tr><td>9:00</td><td>Math</td><td>Reading</td></tr>
            </table>
        </body></html>"#;

        let (template, method) = extract_template_with_ai(html, &provider).await;

        // With only one table, AI should NOT be called.
        assert_eq!(provider.call_count(), 0);
        assert_eq!(method, "heuristic");
        // Should still extract the table using heuristic.
        assert!(!template.table_structure.columns.is_empty());
    }

    #[tokio::test]
    async fn test_extract_template_with_ai_empty_html() {
        let provider = MockAiProvider::new("{}");
        let (template, method) = extract_template_with_ai("", &provider).await;
        assert!(template.table_structure.columns.is_empty());
        assert_eq!(method, "none");
        assert_eq!(provider.call_count(), 0);
    }

    #[test]
    fn test_ai_table_identification_serialization() {
        let id = AiTableIdentification {
            table_index: 1,
            column_semantic: "days_of_week".to_string(),
            row_semantic: "time_slots".to_string(),
            layout_type: "schedule_grid".to_string(),
        };

        let json = serde_json::to_string(&id).unwrap();
        let deserialized: AiTableIdentification = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.table_index, 1);
        assert_eq!(deserialized.column_semantic, "days_of_week");
        assert_eq!(deserialized.row_semantic, "time_slots");
        assert_eq!(deserialized.layout_type, "schedule_grid");
    }

    #[test]
    fn test_ai_table_identification_deserialization_from_ai_response() {
        // Test that we can deserialize the exact format the AI is expected to return.
        let json_str = r#"{"table_index": 0, "column_semantic": "lesson_attributes", "row_semantic": "lessons", "layout_type": "standard_table"}"#;
        let id: AiTableIdentification = serde_json::from_str(json_str).unwrap();
        assert_eq!(id.table_index, 0);
        assert_eq!(id.layout_type, "standard_table");
    }

    #[tokio::test]
    async fn test_extract_template_with_ai_preserves_other_fields() {
        // Verify that AI table selection still extracts colors, time slots, etc.
        let provider = MockAiProvider::new(
            r#"{"table_index": 0, "column_semantic": "days_of_week", "row_semantic": "time_slots", "layout_type": "schedule_grid"}"#,
        );

        let html = r#"<html><body>
            <table>
                <tr><th style="background-color:#9900ff">Time</th><th style="background-color:#9900ff">Monday</th><th style="background-color:#9900ff">Tuesday</th><th style="background-color:#9900ff">Wednesday</th></tr>
                <tr><td>9:00-9:30</td><td>Math</td><td>Math</td><td>Math</td></tr>
                <tr><td>9:30-10:00</td><td>Recess</td><td>Recess</td><td>Recess</td></tr>
                <tr><td>10:00-10:30</td><td>Reading</td><td>Reading</td><td>Reading</td></tr>
            </table>
            <table>
                <tr><th>Archive</th><th>Notes</th></tr>
                <tr><td>Old</td><td>Data</td></tr>
            </table>
        </body></html>"#;

        let (template, method) = extract_template_with_ai(html, &provider).await;
        assert_eq!(method, "ai");

        // Time slots should still be extracted.
        assert!(template.time_slots.contains(&"9:00-9:30".to_string()));
        assert!(template.time_slots.contains(&"9:30-10:00".to_string()));

        // Colors should still be extracted.
        assert!(!template.color_scheme.mappings.is_empty());

        // Recurring elements should still be extracted.
        assert!(template.recurring_elements.activities.contains(&"Math".to_string()));

        // Daily routine should still be extracted.
        let routine_names: Vec<&str> = template.daily_routine.iter().map(|e| e.name.as_str()).collect();
        assert!(routine_names.contains(&"Recess"));
    }

    // ── Transposed Schedule Tests ────────────────────────────────

    #[test]
    fn test_detect_transposed_schedule_basic() {
        let html = r#"<html><body>
            <table>
                <tr><th>Day</th><th>8:00-8:30</th><th>8:30-9:00</th><th>9:00-9:30</th></tr>
                <tr><td>Monday</td><td>Math</td><td>Reading</td><td>Science</td></tr>
                <tr><td>Tuesday</td><td>Math</td><td>Writing</td><td>Art</td></tr>
                <tr><td>Wednesday</td><td>Math</td><td>Reading</td><td>PE</td></tr>
            </table>
        </body></html>"#;

        let tables = parser::extract_tables(html);
        assert_eq!(tables.len(), 1);

        let result = detect_transposed_schedule(&tables[0]);
        assert!(result.is_some(), "Should detect transposed schedule");

        let (day_rows, time_cols) = result.unwrap();
        assert_eq!(day_rows.len(), 3, "Should find 3 day rows");
        assert_eq!(time_cols.len(), 3, "Should find 3 time columns");
    }

    #[test]
    fn test_detect_transposed_schedule_not_transposed() {
        // Standard orientation should NOT be detected as transposed.
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th></tr>
                <tr><td>8:00-8:30</td><td>Math</td><td>Reading</td><td>Science</td></tr>
            </table>
        </body></html>"#;

        let tables = parser::extract_tables(html);
        assert!(detect_transposed_schedule(&tables[0]).is_none());
    }

    #[test]
    fn test_transposed_schedule_time_slots_extracted() {
        let html = r#"<html><body>
            <table>
                <tr><th>Day</th><th>8:00-8:30</th><th>8:30-9:00</th><th>9:00-9:30</th></tr>
                <tr><td>Monday</td><td>Math</td><td>Reading</td><td>Recess</td></tr>
                <tr><td>Tuesday</td><td>Math</td><td>Writing</td><td>Recess</td></tr>
                <tr><td>Wednesday</td><td>Math</td><td>Reading</td><td>Recess</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // Time slots should come from header row columns.
        assert!(template.time_slots.contains(&"8:00-8:30".to_string()), "Missing 8:00-8:30");
        assert!(template.time_slots.contains(&"8:30-9:00".to_string()), "Missing 8:30-9:00");
        assert!(template.time_slots.contains(&"9:00-9:30".to_string()), "Missing 9:00-9:30");
    }

    #[test]
    fn test_transposed_schedule_layout_type_and_semantics() {
        let html = r#"<html><body>
            <table>
                <tr><th>Day</th><th>8:00-8:30</th><th>8:30-9:00</th></tr>
                <tr><td>Monday</td><td>Math</td><td>Reading</td></tr>
                <tr><td>Tuesday</td><td>Math</td><td>Writing</td></tr>
                <tr><td>Wednesday</td><td>Math</td><td>Reading</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        assert_eq!(template.table_structure.layout_type, "schedule_grid");
        assert_eq!(
            template.table_structure.column_semantic,
            Some("time_slots".to_string()),
            "Columns should be time_slots in transposed grid"
        );
        assert_eq!(
            template.table_structure.row_semantic,
            Some("days_of_week".to_string()),
            "Rows should be days_of_week in transposed grid"
        );
    }

    #[test]
    fn test_transposed_schedule_daily_routine() {
        let html = r#"<html><body>
            <table>
                <tr><th>Day</th><th>7:45-8:00</th><th>8:00-8:45</th><th>9:00-9:15</th><th>11:00-11:30</th></tr>
                <tr><td>Monday</td><td>Breakfast</td><td>Math</td><td>Recess</td><td>Lunch</td></tr>
                <tr><td>Tuesday</td><td>Breakfast</td><td>Reading</td><td>Recess</td><td>Lunch</td></tr>
                <tr><td>Wednesday</td><td>Breakfast</td><td>Math</td><td>Recess</td><td>Lunch</td></tr>
                <tr><td>Thursday</td><td>Breakfast</td><td>Science</td><td>Recess</td><td>Lunch</td></tr>
                <tr><td>Friday</td><td>Breakfast</td><td>Math</td><td>Recess</td><td>Lunch</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        let routine_names: Vec<&str> = template.daily_routine.iter().map(|e| e.name.as_str()).collect();
        assert!(routine_names.contains(&"Breakfast"), "Missing Breakfast: {:?}", routine_names);
        assert!(routine_names.contains(&"Recess"), "Missing Recess: {:?}", routine_names);
        assert!(routine_names.contains(&"Lunch"), "Missing Lunch: {:?}", routine_names);

        // Verify time slots.
        let breakfast = template.daily_routine.iter().find(|e| e.name == "Breakfast").unwrap();
        assert_eq!(breakfast.time_slot, Some("7:45-8:00".to_string()));
        assert_eq!(breakfast.days.len(), 5, "Breakfast should occur on all 5 days");

        // Math appears 3/5 = 60% — meets threshold.
        assert!(routine_names.contains(&"Math"), "Math at 60%% should meet threshold: {:?}", routine_names);
    }

    #[test]
    fn test_transposed_schedule_recurring_elements() {
        let html = r#"<html><body>
            <table>
                <tr><th>Day</th><th>8:00-8:30</th><th>8:30-9:00</th></tr>
                <tr><td>Monday</td><td>Math</td><td>Reading</td></tr>
                <tr><td>Tuesday</td><td>Math</td><td>Writing</td></tr>
                <tr><td>Wednesday</td><td>Math</td><td>Reading</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // Math appears 3 times, Reading 2 times — both should be recurring.
        assert!(template.recurring_elements.activities.contains(&"Math".to_string()));
        assert!(template.recurring_elements.activities.contains(&"Reading".to_string()));
        // Writing appears once — should NOT be recurring.
        assert!(!template.recurring_elements.activities.contains(&"Writing".to_string()));
    }

    #[test]
    fn test_transposed_schedule_beats_archive_in_scoring() {
        let html = r#"<html><body>
            <table>
                <tr><th>LP 2022-2023</th><th>LP 2023/2024</th><th>LP 2024/2025</th></tr>
                <tr><td>Unit 1</td><td>Module 1</td><td>Chapter 1</td></tr>
                <tr><td>Unit 2</td><td>Module 2</td><td>Chapter 2</td></tr>
            </table>
            <table>
                <tr><th>Day</th><th>8:00-8:30</th><th>8:30-9:00</th><th>9:00-9:30</th></tr>
                <tr><td>Monday</td><td>Math</td><td>Reading</td><td>Recess</td></tr>
                <tr><td>Tuesday</td><td>Math</td><td>Writing</td><td>Recess</td></tr>
                <tr><td>Wednesday</td><td>Math</td><td>Reading</td><td>Recess</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // Should pick the transposed schedule, NOT the archive table.
        assert_eq!(template.table_structure.layout_type, "schedule_grid");
        assert!(
            !template.table_structure.columns.iter().any(|c| c.contains("2022")),
            "Should not contain archive year headers"
        );
        assert_eq!(
            template.table_structure.column_semantic,
            Some("time_slots".to_string())
        );

        // Time slots and daily routine should be extracted from transposed table.
        assert!(!template.time_slots.is_empty(), "Should extract time slots from transposed table");
        assert!(template.time_slots.contains(&"8:00-8:30".to_string()));
    }

    #[test]
    fn test_scoped_extraction_ignores_wrong_table() {
        // Regression: when two schedule-like tables exist, extraction should only
        // use the best-scoring one. Previously, extract_time_slots/daily_routine/
        // recurring_elements iterated ALL tables, mixing data from wrong tables.
        let html = r#"<html><body>
            <table>
                <tr><th>LP 2022-2023</th><th>LP 2023/2024</th><th>LP 2024/2025</th></tr>
                <tr><td>Unit 1</td><td>Module 1</td><td>Chapter 1</td></tr>
            </table>
            <table>
                <tr><th>Day/Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:00-8:30</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td></tr>
                <tr><td>8:30-9:00</td><td>Math</td><td>Reading</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td>11:00-11:30</td><td>Lunch</td><td>Lunch</td><td>Lunch</td><td>Lunch</td><td>Lunch</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // Should select the schedule grid.
        assert_eq!(template.table_structure.layout_type, "schedule_grid");
        assert!(template.table_structure.columns.contains(&"Monday".to_string()));

        // Time slots should be present.
        assert!(template.time_slots.contains(&"8:00-8:30".to_string()));
        assert!(template.time_slots.contains(&"8:30-9:00".to_string()));
        assert!(template.time_slots.contains(&"11:00-11:30".to_string()));

        // Daily routine should be detected.
        let routine_names: Vec<&str> = template.daily_routine.iter().map(|e| e.name.as_str()).collect();
        assert!(routine_names.contains(&"Breakfast"), "Missing Breakfast: {:?}", routine_names);
        assert!(routine_names.contains(&"Lunch"), "Missing Lunch: {:?}", routine_names);
    }

    /// Integration test: a realistic TK teacher schedule with AM/PM formatted
    /// times and 15+ time slots (matching the structure from the bug report).
    /// Verifies that ALL time slots, recurring events, and colors are extracted.
    #[test]
    fn test_extract_template_tk_full_day_schedule_am_pm() {
        let html = r#"<html><body>
            <table>
                <tr>
                    <th style="background-color:#9900ff">Day/Time</th>
                    <th style="background-color:#9900ff">Monday</th>
                    <th style="background-color:#9900ff">Tuesday</th>
                    <th style="background-color:#9900ff">Wednesday</th>
                    <th style="background-color:#9900ff">Thursday</th>
                    <th style="background-color:#9900ff">Friday</th>
                </tr>
                <tr><td>8:15 AM-8:30 AM</td><td style="background-color:#ffff00">Soft Start Breakfast</td><td style="background-color:#ffff00">Soft Start Breakfast</td><td style="background-color:#ffff00">Soft Start Breakfast</td><td style="background-color:#ffff00">Soft Start Breakfast</td><td style="background-color:#ffff00">Soft Start Breakfast</td></tr>
                <tr><td>8:30 AM-9:00 AM</td><td>Morning Circle</td><td>Morning Circle</td><td>Morning Circle</td><td>Morning Circle</td><td>Morning Circle</td></tr>
                <tr><td>9:00 AM-9:10 AM</td><td style="background-color:#d9ead3">Snack/Recess</td><td style="background-color:#d9ead3">Snack/Recess</td><td style="background-color:#d9ead3">Snack/Recess</td><td style="background-color:#d9ead3">Snack/Recess</td><td style="background-color:#d9ead3">Snack/Recess</td></tr>
                <tr><td>9:10 AM-9:30 AM</td><td>Calendar Math</td><td>Calendar Math</td><td>Calendar Math</td><td>Calendar Math</td><td>Calendar Math</td></tr>
                <tr><td>9:30 AM-10:00 AM</td><td>ELA Mini Lesson</td><td>ELA Mini Lesson</td><td>ELA Mini Lesson</td><td>ELA Mini Lesson</td><td>ELA Mini Lesson</td></tr>
                <tr><td>10:00 AM-10:30 AM</td><td>Centers/Small Group</td><td>Centers/Small Group</td><td>Centers/Small Group</td><td>Centers/Small Group</td><td>Centers/Small Group</td></tr>
                <tr><td>10:30 AM-11:00 AM</td><td>Math Lesson</td><td>Math Lesson</td><td>Math Lesson</td><td>Math Lesson</td><td>Math Lesson</td></tr>
                <tr><td>11:00 AM-11:15 AM</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td></tr>
                <tr><td>11:15 AM-11:30 AM</td><td>Lunch Prep</td><td>Lunch Prep</td><td>Lunch Prep</td><td>Lunch Prep</td><td>Lunch Prep</td></tr>
                <tr><td>11:30 AM-12:00 PM</td><td style="background-color:#fce5cd">TK Lunch</td><td style="background-color:#fce5cd">TK Lunch</td><td style="background-color:#fce5cd">TK Lunch</td><td style="background-color:#fce5cd">TK Lunch</td><td style="background-color:#fce5cd">TK Lunch</td></tr>
                <tr><td>12:00 PM-12:45 PM</td><td>Rest Time</td><td>Rest Time</td><td>Rest Time</td><td>Rest Time</td><td>Rest Time</td></tr>
                <tr><td>12:45 PM-1:15 PM</td><td>Science/Social Studies</td><td>Science/Social Studies</td><td>Science/Social Studies</td><td>Science/Social Studies</td><td>Science/Social Studies</td></tr>
                <tr><td>1:15 PM-1:45 PM</td><td>Mandarin</td><td>PE</td><td>Mandarin</td><td>Art</td><td>Music</td></tr>
                <tr><td>1:45 PM-2:00 PM</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td><td style="background-color:#d9ead3">Recess</td></tr>
                <tr><td>2:00 PM-2:30 PM</td><td>Read Aloud/Art</td><td>Read Aloud/Art</td><td>Read Aloud/Art</td><td>Read Aloud/Art</td><td>Read Aloud/Art</td></tr>
                <tr><td>2:30 PM-2:40 PM</td><td>Pack Up</td><td>Pack Up</td><td>Pack Up</td><td>Pack Up</td><td>Pack Up</td></tr>
                <tr><td>2:40 PM-3:00 PM</td><td style="background-color:#c9daf8">Dismissal</td><td style="background-color:#c9daf8">Dismissal</td><td style="background-color:#c9daf8">Dismissal</td><td style="background-color:#c9daf8">Dismissal</td><td style="background-color:#c9daf8">Dismissal</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // Should detect as a schedule grid.
        assert_eq!(template.table_structure.layout_type, "schedule_grid");
        assert_eq!(template.table_structure.column_count, 6);

        // ALL 17 time slots must be extracted — not 4, not 10, all 17.
        assert_eq!(
            template.time_slots.len(), 17,
            "Expected 17 time slots, got {}: {:?}",
            template.time_slots.len(), template.time_slots
        );

        // Spot-check specific AM/PM formatted times are preserved exactly.
        assert!(template.time_slots.contains(&"8:15 AM-8:30 AM".to_string()),
            "Missing 8:15 AM-8:30 AM: {:?}", template.time_slots);
        assert!(template.time_slots.contains(&"11:30 AM-12:00 PM".to_string()),
            "Missing 11:30 AM-12:00 PM: {:?}", template.time_slots);
        assert!(template.time_slots.contains(&"2:40 PM-3:00 PM".to_string()),
            "Missing 2:40 PM-3:00 PM: {:?}", template.time_slots);

        // Daily routine events: all events appearing in ≥40% of days should be detected.
        let routine_names: Vec<&str> = template.daily_routine.iter()
            .map(|e| e.name.as_str()).collect();
        assert!(routine_names.contains(&"Soft Start Breakfast"),
            "Missing Soft Start Breakfast: {:?}", routine_names);
        assert!(routine_names.contains(&"Snack/Recess"),
            "Missing Snack/Recess: {:?}", routine_names);
        assert!(routine_names.contains(&"TK Lunch"),
            "Missing TK Lunch: {:?}", routine_names);
        assert!(routine_names.contains(&"Rest Time"),
            "Missing Rest Time: {:?}", routine_names);
        assert!(routine_names.contains(&"Dismissal"),
            "Missing Dismissal: {:?}", routine_names);

        // Color scheme should detect header color and activity colors.
        assert!(!template.color_scheme.mappings.is_empty(),
            "Color scheme should not be empty");
        let all_colors: Vec<&str> = template.color_scheme.mappings.iter()
            .map(|m| m.color.as_str()).collect();
        assert!(all_colors.contains(&"#9900ff"),
            "Missing purple header color: {:?}", all_colors);
    }

    #[test]
    fn test_merged_title_row_schedule_detection() {
        // Simulates Google Docs export where row 0 is a single merged title cell
        // and row 1 contains the actual day column headers.
        let html = r#"<html><body>
            <table>
                <tr><td colspan="6">Mrs. Cole's TK Schedule 2025-2026</td></tr>
                <tr>
                    <td>Day/Time LP 2022-2023</td>
                    <td>Monday 8:15-3:05</td>
                    <td>Tuesday 8:15-3:05</td>
                    <td>Wednesday 8:15-3:05</td>
                    <td>Thursday 8:15-3:05</td>
                    <td>Friday 8:15-3:05</td>
                </tr>
                <tr><td>8:15-9:00</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td></tr>
                <tr><td>9:00-9:10</td><td>Morning Circle</td><td>Morning Circle</td><td>Morning Circle</td><td>Morning Circle</td><td>Morning Circle</td></tr>
                <tr><td>9:10-9:30</td><td>Eureka Math</td><td>Mandarin</td><td>Music</td><td>Eureka Math</td><td>Centers</td></tr>
                <tr><td>9:30-10:00</td><td>SEL</td><td>SEL</td><td>SEL</td><td>SEL</td><td>SEL</td></tr>
                <tr><td>10:00-10:40</td><td>Snack/Recess</td><td>Snack/Recess</td><td>Snack/Recess</td><td>Snack/Recess</td><td>Snack/Recess</td></tr>
                <tr><td>10:40-11:30</td><td>Centers</td><td>Centers</td><td>Centers</td><td>Centers</td><td>Centers</td></tr>
                <tr><td>11:30-12:10</td><td>Lunch Prep</td><td>Lunch Prep</td><td>Lunch Prep</td><td>Lunch Prep</td><td>Lunch Prep</td></tr>
                <tr><td>12:10-12:20</td><td>Breath</td><td>Breath</td><td>Breath</td><td>Breath</td><td>Breath</td></tr>
                <tr><td>12:20-1:00</td><td>Rest Time</td><td>Rest Time</td><td>Rest Time</td><td>Rest Time</td><td>Rest Time</td></tr>
                <tr><td>1:00-1:10</td><td>Handwriting</td><td>Handwriting</td><td>Handwriting</td><td>Handwriting</td><td>Handwriting</td></tr>
                <tr><td>1:10-1:30</td><td>Read Aloud</td><td>Science</td><td>Read Aloud</td><td>Science</td><td>Read Aloud</td></tr>
                <tr><td>1:30-1:50</td><td>Recess</td><td>Recess</td><td>Recess</td><td>Recess</td><td>Recess</td></tr>
                <tr><td>1:50-2:20</td><td>Art</td><td>PE</td><td>Art</td><td>PE</td><td>Art</td></tr>
                <tr><td>2:20-2:45</td><td>Pack Up</td><td>Pack Up</td><td>Pack Up</td><td>Pack Up</td><td>Pack Up</td></tr>
                <tr><td>2:45-3:05</td><td>Dismissal</td><td>Dismissal</td><td>Dismissal</td><td>Dismissal</td><td>Dismissal</td></tr>
                <tr><td>3:40-4:30</td><td>TK Mtg</td><td></td><td></td><td></td><td></td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // Should detect as schedule_grid despite merged title row.
        assert_eq!(template.table_structure.layout_type, "schedule_grid",
            "Should detect schedule grid. Got: {:?}", template.table_structure);

        // Should use row 1 headers (6 columns, not 1).
        assert_eq!(template.table_structure.column_count, 6,
            "Should have 6 columns from row 1");

        // Time slots should be extracted from rows 2+.
        assert!(template.time_slots.len() >= 15,
            "Expected 15+ time slots, got {}: {:?}",
            template.time_slots.len(), template.time_slots);
        assert!(template.time_slots.contains(&"8:15-9:00".to_string()));
        assert!(template.time_slots.contains(&"9:00-9:10".to_string()));
        assert!(template.time_slots.contains(&"2:45-3:05".to_string()));
        assert!(template.time_slots.contains(&"3:40-4:30".to_string()));

        // Daily routine events should be detected.
        let routine_names: Vec<&str> = template.daily_routine.iter()
            .map(|e| e.name.as_str()).collect();
        assert!(routine_names.contains(&"Breakfast"),
            "Missing Breakfast: {:?}", routine_names);
        assert!(routine_names.contains(&"Morning Circle"),
            "Missing Morning Circle: {:?}", routine_names);
        assert!(routine_names.contains(&"Snack/Recess"),
            "Missing Snack/Recess: {:?}", routine_names);
        assert!(routine_names.contains(&"Rest Time"),
            "Missing Rest Time: {:?}", routine_names);
        assert!(routine_names.contains(&"Dismissal"),
            "Missing Dismissal: {:?}", routine_names);
        assert!(routine_names.contains(&"Recess"),
            "Missing Recess: {:?}", routine_names);
    }

    #[test]
    fn test_real_google_doc_schedule_extraction() {
        // Integration test using the actual Google Doc HTML export.
        let fixture_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/cole_schedule.html"
        );
        let html = match std::fs::read_to_string(fixture_path) {
            Ok(h) => h,
            Err(_) => {
                eprintln!("Skipping test_real_google_doc_schedule_extraction: fixture not found at {}", fixture_path);
                return;
            }
        };

        let template = extract_template(&html);

        // Must detect as schedule_grid.
        assert_eq!(template.table_structure.layout_type, "schedule_grid",
            "Should detect schedule grid from real HTML. Structure: {:?}", template.table_structure);

        // Must have 6 columns (Day/Time + 5 days).
        assert_eq!(template.table_structure.column_count, 6,
            "Should have 6 columns. Got: {:?}", template.table_structure.columns);

        // Must extract 15+ time slots (8:15-9:00 through 3:40-4:30).
        assert!(template.time_slots.len() >= 15,
            "Expected 15+ time slots, got {}: {:?}",
            template.time_slots.len(), template.time_slots);

        // Spot-check specific time slots.
        assert!(template.time_slots.contains(&"8:15-9:00".to_string()),
            "Missing 8:15-9:00: {:?}", template.time_slots);
        assert!(template.time_slots.contains(&"9:00-9:10".to_string()),
            "Missing 9:00-9:10: {:?}", template.time_slots);

        // Must extract daily routine events.
        assert!(!template.daily_routine.is_empty(),
            "Daily routine should not be empty");
        let routine_names: Vec<&str> = template.daily_routine.iter()
            .map(|e| e.name.as_str()).collect();
        eprintln!("Extracted {} daily routine events: {:?}", routine_names.len(), routine_names);
        eprintln!("Extracted {} time slots: {:?}", template.time_slots.len(), template.time_slots);

        // Color scheme is extracted from inline styles — Google Docs may use different
        // styling patterns so we just log rather than require non-empty.
        eprintln!("Color scheme mappings: {}", template.color_scheme.mappings.len());
    }

    // ── Resolve Grid Tests ─────────────────────────────────────────

    #[test]
    fn test_resolve_grid_simple_table() {
        let html = r#"<html><body>
            <table>
                <tr><th>A</th><th>B</th><th>C</th></tr>
                <tr><td>1</td><td>2</td><td>3</td></tr>
            </table>
        </body></html>"#;
        let tables = parser::extract_tables(html);
        let grid = resolve_grid(&tables[0]);

        assert_eq!(grid.width, 3);
        assert_eq!(grid.height, 2);
        assert_eq!(grid.cell_text(0, 0), "A");
        assert_eq!(grid.cell_text(0, 1), "B");
        assert_eq!(grid.cell_text(0, 2), "C");
        assert_eq!(grid.cell_text(1, 0), "1");
        assert_eq!(grid.cell_text(1, 1), "2");
        assert_eq!(grid.cell_text(1, 2), "3");
    }

    #[test]
    fn test_resolve_grid_colspan() {
        let html = r#"<html><body>
            <table>
                <tr><td colspan="3">Title Row</td></tr>
                <tr><th>A</th><th>B</th><th>C</th></tr>
                <tr><td>1</td><td>2</td><td>3</td></tr>
            </table>
        </body></html>"#;
        let tables = parser::extract_tables(html);
        let grid = resolve_grid(&tables[0]);

        assert_eq!(grid.width, 3);
        assert_eq!(grid.height, 3);

        // Row 0: single cell spanning all 3 columns.
        assert_eq!(grid.cell_text(0, 0), "Title Row");
        assert_eq!(grid.cell_text(0, 1), "Title Row");
        assert_eq!(grid.cell_text(0, 2), "Title Row");

        // The merged row should be detected as full-width merge.
        assert!(grid.is_full_width_merge(0));
        assert!(!grid.is_full_width_merge(1));
        assert!(!grid.is_full_width_merge(2));
    }

    #[test]
    fn test_resolve_grid_rowspan() {
        let html = r#"<html><body>
            <table>
                <tr><td rowspan="2">Time</td><td>Mon</td><td>Tue</td></tr>
                <tr><td>Math</td><td>Reading</td></tr>
            </table>
        </body></html>"#;
        let tables = parser::extract_tables(html);
        let grid = resolve_grid(&tables[0]);

        assert_eq!(grid.width, 3);
        assert_eq!(grid.height, 2);

        // Row 0: Time | Mon | Tue
        assert_eq!(grid.cell_text(0, 0), "Time");
        assert_eq!(grid.cell_text(0, 1), "Mon");
        assert_eq!(grid.cell_text(0, 2), "Tue");

        // Row 1: Time (rowspan) | Math | Reading
        assert_eq!(grid.cell_text(1, 0), "Time");
        assert_eq!(grid.cell_text(1, 1), "Math");
        assert_eq!(grid.cell_text(1, 2), "Reading");
    }

    #[test]
    fn test_resolve_grid_mid_table_merged_row() {
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:00-8:30</td><td>Math</td><td>Math</td><td>Math</td><td>Math</td><td>Math</td></tr>
                <tr><td colspan="6">SPRING BREAK - NO SCHOOL</td></tr>
                <tr><td>9:00-9:30</td><td>Reading</td><td>Reading</td><td>Reading</td><td>Reading</td><td>Reading</td></tr>
            </table>
        </body></html>"#;
        let tables = parser::extract_tables(html);
        let grid = resolve_grid(&tables[0]);

        assert_eq!(grid.width, 6);
        assert_eq!(grid.height, 4);

        // Row 2 is a full-width merge (section divider).
        assert!(!grid.is_full_width_merge(0));
        assert!(!grid.is_full_width_merge(1));
        assert!(grid.is_full_width_merge(2));
        assert!(!grid.is_full_width_merge(3));
    }

    #[test]
    fn test_resolve_grid_empty_table() {
        let table = ParsedTable { rows: vec![] };
        let grid = resolve_grid(&table);
        assert_eq!(grid.width, 0);
        assert_eq!(grid.height, 0);
    }

    // ── Merged Row/Column Robustness Tests ─────────────────────────

    #[test]
    fn test_mid_table_merged_row_skipped_in_extraction() {
        // A schedule table with a "SPRING BREAK" banner row in the middle.
        // The extraction should skip the merged row and extract activities
        // from the data rows before and after.
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:00-8:30</td><td>Math</td><td>Reading</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td colspan="6">SPRING BREAK - NO SCHOOL</td></tr>
                <tr><td>9:00-9:30</td><td>Science</td><td>Art</td><td>Science</td><td>Art</td><td>PE</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // The merged banner row should NOT produce garbage in any extraction output.
        assert_eq!(template.table_structure.layout_type, "schedule_grid");
        assert!(template.time_slots.contains(&"8:00-8:30".to_string()));
        assert!(template.time_slots.contains(&"9:00-9:30".to_string()));

        // "SPRING BREAK - NO SCHOOL" should NOT appear as an activity or time slot.
        assert!(!template.recurring_elements.activities.iter().any(|a| a.contains("SPRING BREAK")),
            "Merged banner row should not produce activities");
        assert!(!template.time_slots.iter().any(|t| t.contains("SPRING")),
            "Merged banner row should not produce time slots");

        // The row categories should not include the banner text.
        assert!(!template.table_structure.row_categories.iter().any(|c| c.contains("SPRING")),
            "Merged banner row should not produce row categories");
    }

    #[test]
    fn test_mid_table_merged_row_skipped_in_lesson_extraction() {
        // Lesson extraction from mod.rs should also skip merged rows.
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th></tr>
                <tr><td>8:00-8:30</td><td>Math</td><td>Reading</td><td>Science</td></tr>
                <tr><td colspan="4">Week 5 - Assessment Week</td></tr>
                <tr><td>9:00-9:30</td><td>PE</td><td>Art</td><td>Music</td></tr>
            </table>
        </body></html>"#;

        let lessons = super::super::extract_lessons_from_doc(html);

        // Should extract activities from rows 1 and 3, skip the merged row 2.
        let titles: Vec<&str> = lessons.iter().map(|l| l.title.as_str()).collect();
        assert!(titles.contains(&"Math"), "Should extract Math");
        assert!(titles.contains(&"PE"), "Should extract PE");
        assert!(!titles.iter().any(|t| t.contains("Week 5")),
            "Should NOT extract the merged banner: {:?}", titles);
    }

    #[test]
    fn test_multiple_header_rows_handled() {
        // Table with a merged title row, then the actual header row.
        let html = r#"<html><body>
            <table>
                <tr><td colspan="6">Mrs. Cole's TK Schedule 2025-2026</td></tr>
                <tr><th>Day/Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:15-9:00</td><td>Math</td><td>Reading</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td>9:00-9:30</td><td>PE</td><td>Art</td><td>PE</td><td>Music</td><td>PE</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // Should detect as schedule grid despite the merged title row.
        assert_eq!(template.table_structure.layout_type, "schedule_grid");
        assert!(template.table_structure.columns.contains(&"Monday".to_string()),
            "Should find day columns: {:?}", template.table_structure.columns);
        assert!(template.time_slots.contains(&"8:15-9:00".to_string()),
            "Should extract time slots");
    }

    #[test]
    fn test_two_merged_title_rows() {
        // Two merged title rows before the actual schedule headers.
        let html = r#"<html><body>
            <table>
                <tr><td colspan="6">Springfield Elementary</td></tr>
                <tr><td colspan="6">Week of March 23, 2026</td></tr>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>9:00-9:30</td><td>Math</td><td>Math</td><td>Math</td><td>Math</td><td>Math</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        assert_eq!(template.table_structure.layout_type, "schedule_grid");
        assert_eq!(template.table_structure.column_count, 6);
        assert!(template.time_slots.contains(&"9:00-9:30".to_string()));
    }

    #[test]
    fn test_merged_column_in_data_row() {
        // A schedule where one cell spans multiple day columns (e.g., "Field Trip"
        // spanning Monday-Wednesday). This row has fewer cells than the header.
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:00-8:30</td><td>Math</td><td>Math</td><td>Math</td><td>Math</td><td>Math</td></tr>
                <tr><td>9:00-9:30</td><td colspan="3">Field Trip</td><td>Reading</td><td>Science</td></tr>
                <tr><td>10:00-10:30</td><td>PE</td><td>Art</td><td>PE</td><td>Music</td><td>PE</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // Should not crash and should extract the schedule structure.
        assert_eq!(template.table_structure.layout_type, "schedule_grid");
        assert!(template.time_slots.len() >= 2, "Should extract time slots");

        // The resolve_grid should place "Field Trip" across 3 columns.
        let tables = parser::extract_tables(html);
        let grid = resolve_grid(&tables[0]);
        assert_eq!(grid.cell_text(2, 1), "Field Trip");
        assert_eq!(grid.cell_text(2, 2), "Field Trip");
        assert_eq!(grid.cell_text(2, 3), "Field Trip");
        assert_eq!(grid.cell_text(2, 4), "Reading");
        assert_eq!(grid.cell_text(2, 5), "Science");
    }

    #[test]
    fn test_multiple_mid_table_section_dividers() {
        // Multiple section dividers throughout the table.
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th></tr>
                <tr><td colspan="4">Morning Block</td></tr>
                <tr><td>8:00-8:30</td><td>Math</td><td>Math</td><td>Math</td></tr>
                <tr><td colspan="4">Afternoon Block</td></tr>
                <tr><td>1:00-1:30</td><td>Science</td><td>Art</td><td>Music</td></tr>
                <tr><td colspan="4">After School</td></tr>
                <tr><td>3:00-3:30</td><td>Tutoring</td><td>Clubs</td><td>Tutoring</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        // All three time slots should be extracted despite the dividers.
        assert!(template.time_slots.contains(&"8:00-8:30".to_string()),
            "Should extract 8:00 slot");
        assert!(template.time_slots.contains(&"1:00-1:30".to_string()),
            "Should extract 1:00 slot");
        assert!(template.time_slots.contains(&"3:00-3:30".to_string()),
            "Should extract 3:00 slot");

        // Section divider text should NOT be in row categories.
        assert!(!template.table_structure.row_categories.iter()
            .any(|c| c.contains("Block") || c.contains("After School")),
            "Dividers should not appear in row categories: {:?}",
            template.table_structure.row_categories);
    }

    #[test]
    fn test_is_merged_row_basic() {
        use super::super::is_merged_row;
        use super::super::parser::{TableRow, TableCell};

        // A row with 1 cell when expecting 6 columns is merged.
        let merged_row = TableRow {
            cells: vec![TableCell {
                text: "Banner".into(),
                colspan: 6,
                ..Default::default()
            }],
        };
        assert!(is_merged_row(&merged_row, 6));

        // A row with 6 cells when expecting 6 columns is NOT merged.
        let normal_row = TableRow {
            cells: vec![
                TableCell { text: "A".into(), ..Default::default() },
                TableCell { text: "B".into(), ..Default::default() },
                TableCell { text: "C".into(), ..Default::default() },
                TableCell { text: "D".into(), ..Default::default() },
                TableCell { text: "E".into(), ..Default::default() },
                TableCell { text: "F".into(), ..Default::default() },
            ],
        };
        assert!(!is_merged_row(&normal_row, 6));

        // A row with 2 cells when expecting 6 columns is merged (2*2 < 6).
        let partial_row = TableRow {
            cells: vec![
                TableCell { text: "A".into(), ..Default::default() },
                TableCell { text: "B".into(), ..Default::default() },
            ],
        };
        assert!(is_merged_row(&partial_row, 6));
    }

    #[test]
    fn test_effective_header_row_with_colspan_title() {
        let html = r#"<html><body>
            <table>
                <tr><td colspan="5">My Schedule</td></tr>
                <tr><th>Time</th><th>Mon</th><th>Tue</th><th>Wed</th><th>Thu</th></tr>
                <tr><td>9:00</td><td>Math</td><td>Reading</td><td>Math</td><td>Reading</td></tr>
            </table>
        </body></html>"#;
        let tables = parser::extract_tables(html);

        let h_idx = effective_header_row(&tables[0]);
        assert_eq!(h_idx, 1, "Should skip the colspan title row");
    }

    #[test]
    fn test_effective_header_row_multiple_merged_titles() {
        let html = r#"<html><body>
            <table>
                <tr><td colspan="4">School Name</td></tr>
                <tr><td colspan="4">Week of March 23</td></tr>
                <tr><th>Time</th><th>Mon</th><th>Tue</th><th>Wed</th></tr>
                <tr><td>9:00</td><td>Math</td><td>Reading</td><td>Science</td></tr>
            </table>
        </body></html>"#;
        let tables = parser::extract_tables(html);

        let h_idx = effective_header_row(&tables[0]);
        assert_eq!(h_idx, 2, "Should skip both colspan title rows");
    }

    #[test]
    fn test_graceful_degradation_unknown_format() {
        // A table with an unusual format — should not crash.
        let html = r#"<html><body>
            <table>
                <tr><td>Only one column here</td></tr>
                <tr><td>Another single cell</td></tr>
                <tr><td>Third row</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        // Should produce a valid (possibly empty) template without crashing.
        assert!(template.table_structure.layout_type == "standard_table"
            || template.table_structure.columns.is_empty());
    }

    #[test]
    fn test_holiday_banner_mid_table_with_daily_routine() {
        // Schedule with a holiday banner in the middle — daily routine extraction
        // should work correctly on the non-banner rows.
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>8:00-8:30</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td><td>Breakfast</td></tr>
                <tr><td>8:30-9:00</td><td>Math</td><td>Math</td><td>Math</td><td>Math</td><td>Math</td></tr>
                <tr><td colspan="6">TEACHER IN-SERVICE DAY - NO STUDENTS</td></tr>
                <tr><td>11:00-11:30</td><td>Lunch</td><td>Lunch</td><td>Lunch</td><td>Lunch</td><td>Lunch</td></tr>
                <tr><td>2:30-2:45</td><td>Dismissal</td><td>Dismissal</td><td>Dismissal</td><td>Dismissal</td><td>Dismissal</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);

        let routine_names: Vec<&str> = template.daily_routine.iter()
            .map(|e| e.name.as_str()).collect();

        assert!(routine_names.contains(&"Breakfast"), "Missing Breakfast: {:?}", routine_names);
        assert!(routine_names.contains(&"Math"), "Missing Math: {:?}", routine_names);
        assert!(routine_names.contains(&"Lunch"), "Missing Lunch: {:?}", routine_names);
        assert!(routine_names.contains(&"Dismissal"), "Missing Dismissal: {:?}", routine_names);

        // Banner text should NOT appear in routine.
        assert!(!routine_names.iter().any(|n| n.contains("TEACHER") || n.contains("IN-SERVICE")),
            "Banner text should not be in routine: {:?}", routine_names);
    }

    #[test]
    fn test_colspan_in_parser_preserved() {
        let html = r#"<html><body>
            <table>
                <tr><td colspan="3">Wide cell</td></tr>
                <tr><td>A</td><td>B</td><td>C</td></tr>
            </table>
        </body></html>"#;

        let tables = parser::extract_tables(html);
        assert_eq!(tables[0].rows[0].cells[0].colspan, 3);
        assert_eq!(tables[0].rows[0].cells[0].rowspan, 1);
        assert_eq!(tables[0].rows[1].cells[0].colspan, 1);
        assert_eq!(tables[0].rows[1].cells[0].rowspan, 1);
    }

    #[test]
    fn test_rowspan_in_parser_preserved() {
        let html = r#"<html><body>
            <table>
                <tr><td rowspan="2">Tall cell</td><td>Right 1</td></tr>
                <tr><td>Right 2</td></tr>
            </table>
        </body></html>"#;

        let tables = parser::extract_tables(html);
        assert_eq!(tables[0].rows[0].cells[0].rowspan, 2);
        assert_eq!(tables[0].rows[0].cells[0].colspan, 1);
    }

    #[test]
    fn test_grid_width_uses_colspan() {
        let html = r#"<html><body>
            <table>
                <tr><td colspan="5">Title</td></tr>
                <tr><td>A</td><td>B</td><td>C</td><td>D</td><td>E</td></tr>
            </table>
        </body></html>"#;

        let tables = parser::extract_tables(html);
        assert_eq!(tables[0].grid_width(), 5);
        // Row 0 has 1 cell but effective width is 5.
        assert_eq!(tables[0].rows[0].effective_width(), 5);
        // Row 1 has 5 cells with no colspan.
        assert_eq!(tables[0].rows[1].effective_width(), 5);
    }

    #[test]
    fn test_resolve_grid_colspan_and_rowspan_combined() {
        let html = r#"<html><body>
            <table>
                <tr><td colspan="4" rowspan="1">Title</td></tr>
                <tr><td rowspan="2">Time</td><td>Mon</td><td>Tue</td><td>Wed</td></tr>
                <tr><td>Math</td><td>Reading</td><td>Science</td></tr>
            </table>
        </body></html>"#;

        let tables = parser::extract_tables(html);
        let grid = resolve_grid(&tables[0]);

        assert_eq!(grid.width, 4);
        assert_eq!(grid.height, 3);

        // Row 0: Title spans all 4 columns.
        assert!(grid.is_full_width_merge(0));

        // Row 1: Time | Mon | Tue | Wed
        assert_eq!(grid.cell_text(1, 0), "Time");
        assert_eq!(grid.cell_text(1, 1), "Mon");

        // Row 2: Time (rowspan from row 1) | Math | Reading | Science
        assert_eq!(grid.cell_text(2, 0), "Time");
        assert_eq!(grid.cell_text(2, 1), "Math");
        assert_eq!(grid.cell_text(2, 2), "Reading");
        assert_eq!(grid.cell_text(2, 3), "Science");
    }
}
