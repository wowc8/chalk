//! Template extractor — analyzes table formatting patterns during digest.
//!
//! Extracts a [`TeachingTemplateSchema`] from parsed HTML tables that captures
//! HOW a teacher formats their plans: color scheme, table structure, time slot
//! patterns, content organization, and recurring elements. This schema is stored
//! alongside reference documents and used to format AI-generated plans to match
//! the teacher's style.

use std::collections::HashMap;

use scraper::{Html, Selector};

use crate::database::{
    ColorMapping, ColorScheme, ContentPatterns, DailyRoutineEvent, RecurringElements,
    TableStructure, TeachingTemplateSchema,
};

use super::parser::{self, ParsedTable};
use super::{detect_schedule_columns, is_time_like};

/// Extract a teaching template schema from the raw HTML of a Google Doc.
///
/// Analyzes all tables in the document to determine the teacher's formatting
/// patterns, color usage, table layout, time slots, and recurring content.
pub fn extract_template(html: &str) -> TeachingTemplateSchema {
    let tables = parser::extract_tables(html);
    if tables.is_empty() {
        return TeachingTemplateSchema::default();
    }

    let color_scheme = extract_colors(html);
    let table_structure = extract_table_structure(&tables);
    let time_slots = extract_time_slots(&tables);
    let content_patterns = extract_content_patterns(html, &tables);
    let recurring_elements = extract_recurring_elements(&tables);
    let daily_routine = extract_daily_routine(&tables);

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

    let headers: Vec<String> = table.rows[0]
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

    // Reward: has time-like values in the first column of data rows.
    let time_rows = table
        .rows
        .iter()
        .skip(1)
        .filter(|r| r.cells.first().map_or(false, |c| is_time_like(c.text.trim())))
        .count();
    score += (time_rows as i32) * 5;

    // Reward: reasonable number of rows (2-30 rows typical for weekly plans).
    let data_rows = table.rows.len().saturating_sub(1);
    if (2..=30).contains(&data_rows) {
        score += 10;
    }

    // Reward: reasonable number of columns (5-7 typical for Mon-Fri + time col).
    if (5..=7).contains(&headers.len()) {
        score += 10;
    }

    // Penalty: headers contain year ranges (e.g., "2022-2023", "2023/2024").
    // These are reference/archive tables, not planning templates.
    for header in &header_lower {
        if contains_year_range(header) {
            score -= 40;
        }
        // Penalty: headers like "LP", "Lesson Plan" followed by year.
        if header.contains("lp ") && header.chars().any(|c| c.is_ascii_digit()) {
            score -= 30;
        }
        // Penalty: curriculum name columns (e.g., "eureka math").
        if header.contains("eureka") || header.contains("curriculum") || header.contains("edition") {
            score -= 20;
        }
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
    let main_table = tables
        .iter()
        .max_by_key(|t| score_planning_table(t))
        .unwrap();

    if main_table.rows.is_empty() {
        return TableStructure::default();
    }

    let headers: Vec<String> = main_table.rows[0]
        .cells
        .iter()
        .map(|c| c.text.trim().to_string())
        .collect();

    let header_lower: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();

    let is_schedule = detect_schedule_columns(&header_lower).is_some();
    let layout_type = if is_schedule {
        "schedule_grid".to_string()
    } else {
        "standard_table".to_string()
    };

    // Assign semantic labels.
    let column_semantic = if is_schedule {
        Some("days_of_week".to_string())
    } else {
        None
    };

    // Check if the first column contains time-like values.
    let has_time_rows = main_table
        .rows
        .iter()
        .skip(1)
        .any(|r| r.cells.first().map_or(false, |c| is_time_like(c.text.trim())));

    let row_semantic = if is_schedule && has_time_rows {
        Some("time_slots".to_string())
    } else if !is_schedule {
        // For standard tables, check if the first column looks like categories.
        let first_col_values: Vec<&str> = main_table
            .rows
            .iter()
            .skip(1)
            .filter_map(|r| r.cells.first().map(|c| c.text.trim()))
            .filter(|t| !t.is_empty())
            .collect();
        if !first_col_values.is_empty() && first_col_values.iter().all(|v| !is_time_like(v)) {
            Some("categories".to_string())
        } else {
            None
        }
    } else {
        None
    };

    // Extract row categories from the first column of data rows.
    let mut row_categories = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in main_table.rows.iter().skip(1) {
        if let Some(first_cell) = row.cells.first() {
            let text = first_cell.text.trim().to_string();
            if !text.is_empty() && !is_time_like(&text) && seen.insert(text.clone()) {
                row_categories.push(text);
            }
        }
    }

    let column_count = headers.len();

    TableStructure {
        layout_type,
        columns: headers,
        row_categories,
        column_count,
        column_semantic,
        row_semantic,
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

        let headers: Vec<String> = table.rows[0]
            .cells
            .iter()
            .map(|c| c.text.trim().to_lowercase())
            .collect();

        // Only extract time slots from schedule grids.
        let time_col = if let Some((tc, _)) = detect_schedule_columns(&headers) {
            tc
        } else {
            continue;
        };

        for row in table.rows.iter().skip(1) {
            if let Some(cell) = row.cells.get(time_col) {
                let text = cell.text.trim().to_string();
                if is_time_like(&text) && seen.insert(text.clone()) {
                    time_slots.push(text);
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

        let headers: Vec<String> = table.rows[0]
            .cells
            .iter()
            .map(|c| c.text.trim().to_lowercase())
            .collect();

        let is_schedule = detect_schedule_columns(&headers).is_some();

        for row in table.rows.iter().skip(1) {
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

/// Known routine / non-academic event keywords.
/// An activity whose lowercase form contains any of these is considered a routine event.
const ROUTINE_KEYWORDS: &[&str] = &[
    "breakfast",
    "lunch",
    "recess",
    "dismissal",
    "arrival",
    "morning meeting",
    "morning circle",
    "pack up",
    "pack-up",
    "snack",
    "restroom",
    "bathroom",
    "transition",
    "bus",
    "carpool",
    "aftercare",
    "assembly",
    "homeroom",
    "advisory",
    "gym",
    "pe ",
    "p.e.",
    "physical education",
    "specials",
    "encore",
    "related arts",
    "music",
    "art",
    "library",
    "computer lab",
    "technology",
    "chapel",
    "devotion",
    "pledge",
    "announcements",
    "cleanup",
    "clean up",
    "clean-up",
    "closing circle",
    "quiet time",
    "rest time",
    "nap",
];

/// Returns true if the activity name matches a known routine/non-academic event.
fn is_routine_activity(name: &str) -> bool {
    let lower = name.to_lowercase();
    ROUTINE_KEYWORDS
        .iter()
        .any(|kw| lower.contains(kw) || lower == kw.trim())
}

/// Extract daily routine events — non-academic activities that appear consistently
/// across most day columns at the same time slot in schedule grids.
///
/// For each time slot row in a schedule grid, if the same activity appears in the
/// majority of day columns (≥ 3 out of 5, or ≥ 60% of day columns) AND the activity
/// matches a known routine keyword, it is captured as a `DailyRoutineEvent`.
///
/// Activities that appear at every time slot but are academic (e.g., "Math") are
/// intentionally excluded — we only want the structural day events.
fn extract_daily_routine(tables: &[ParsedTable]) -> Vec<DailyRoutineEvent> {
    let mut routine_events: Vec<DailyRoutineEvent> = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for table in tables {
        if table.rows.len() < 2 {
            continue;
        }

        let headers: Vec<String> = table.rows[0]
            .cells
            .iter()
            .map(|c| c.text.trim().to_lowercase())
            .collect();

        let (time_col, day_col_pairs) = match detect_schedule_columns(&headers) {
            Some(cols) => cols,
            None => continue,
        };

        let day_indices: Vec<usize> = day_col_pairs.iter().map(|(idx, _)| *idx).collect();
        let num_days = day_indices.len();
        // Need at least 2 day columns to detect patterns.
        if num_days < 2 {
            continue;
        }

        // Threshold: activity must appear in ≥60% of day columns for that row.
        let threshold = (num_days as f64 * 0.6).ceil() as usize;

        for row in table.rows.iter().skip(1) {
            // Get the time slot for this row.
            let time_slot = row
                .cells
                .get(time_col)
                .map(|c| c.text.trim().to_string())
                .filter(|t| is_time_like(t));

            // Count activity occurrences across day columns.
            let mut activity_counts: HashMap<String, usize> = HashMap::new();
            for &col_idx in &day_indices {
                if let Some(cell) = row.cells.get(col_idx) {
                    let activity = cell
                        .text
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if !activity.is_empty() && activity.len() < 60 {
                        *activity_counts.entry(activity).or_insert(0) += 1;
                    }
                }
            }

            // Find activities that meet the threshold and are routine.
            for (activity, count) in &activity_counts {
                if *count >= threshold && is_routine_activity(activity) {
                    let name_lower = activity.to_lowercase();
                    if seen_names.insert(name_lower) {
                        routine_events.push(DailyRoutineEvent {
                            name: activity.clone(),
                            time_slot: time_slot.clone(),
                        });
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

    #[test]
    fn test_is_routine_activity() {
        // Known routine keywords.
        assert!(is_routine_activity("Lunch"));
        assert!(is_routine_activity("lunch"));
        assert!(is_routine_activity("Recess"));
        assert!(is_routine_activity("Morning Meeting"));
        assert!(is_routine_activity("Dismissal"));
        assert!(is_routine_activity("Breakfast"));
        assert!(is_routine_activity("Snack Time"));
        assert!(is_routine_activity("PE "));
        assert!(is_routine_activity("Gym"));
        assert!(is_routine_activity("Art"));
        assert!(is_routine_activity("Music"));
        assert!(is_routine_activity("Library"));
        assert!(is_routine_activity("Assembly"));
        assert!(is_routine_activity("Pack Up"));
        assert!(is_routine_activity("Pack-Up"));
        assert!(is_routine_activity("Announcements"));
        assert!(is_routine_activity("Chapel"));
        assert!(is_routine_activity("Closing Circle"));
        assert!(is_routine_activity("Clean Up"));

        // Academic subjects should NOT match.
        assert!(!is_routine_activity("Math"));
        assert!(!is_routine_activity("Reading"));
        assert!(!is_routine_activity("Science"));
        assert!(!is_routine_activity("Social Studies"));
        assert!(!is_routine_activity("Writing Workshop"));
    }

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

        // Should detect Recess, Lunch, Specials, Dismissal as routine events.
        let routine_names: Vec<&str> = template.daily_routine.iter().map(|e| e.name.as_str()).collect();
        assert!(routine_names.contains(&"Recess"), "Expected Recess in routine: {:?}", routine_names);
        assert!(routine_names.contains(&"Lunch"), "Expected Lunch in routine: {:?}", routine_names);
        assert!(routine_names.contains(&"Specials"), "Expected Specials in routine: {:?}", routine_names);
        assert!(routine_names.contains(&"Dismissal"), "Expected Dismissal in routine: {:?}", routine_names);

        // Academic subjects should NOT be in daily_routine.
        assert!(!routine_names.contains(&"Math"));
        assert!(!routine_names.contains(&"Reading"));
        assert!(!routine_names.contains(&"Science"));

        // Verify time slots are captured.
        let recess = template.daily_routine.iter().find(|e| e.name == "Recess").unwrap();
        assert_eq!(recess.time_slot, Some("9:30-10:00".to_string()));

        let lunch = template.daily_routine.iter().find(|e| e.name == "Lunch").unwrap();
        assert_eq!(lunch.time_slot, Some("11:00-11:30".to_string()));
    }

    #[test]
    fn test_extract_daily_routine_no_routine_events() {
        // A schedule grid with only academic subjects — no routine events.
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th></tr>
                <tr><td>9:00</td><td>Math</td><td>Reading</td><td>Math</td></tr>
                <tr><td>10:00</td><td>Science</td><td>Writing</td><td>Science</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        assert!(template.daily_routine.is_empty());
    }

    #[test]
    fn test_extract_daily_routine_partial_coverage() {
        // Recess appears in only 2 out of 5 days — below 60% threshold.
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>9:30-10:00</td><td>Recess</td><td>Recess</td><td>Math</td><td>Science</td><td>Reading</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        // Recess only in 2/5 days = 40%, below 60% threshold.
        let routine_names: Vec<&str> = template.daily_routine.iter().map(|e| e.name.as_str()).collect();
        assert!(!routine_names.contains(&"Recess"));
    }

    #[test]
    fn test_extract_daily_routine_meets_threshold() {
        // Recess appears in 3 out of 5 days — meets 60% threshold.
        let html = r#"<html><body>
            <table>
                <tr><th>Time</th><th>Monday</th><th>Tuesday</th><th>Wednesday</th><th>Thursday</th><th>Friday</th></tr>
                <tr><td>9:30-10:00</td><td>Recess</td><td>Recess</td><td>Recess</td><td>Science</td><td>Reading</td></tr>
            </table>
        </body></html>"#;

        let template = extract_template(html);
        let routine_names: Vec<&str> = template.daily_routine.iter().map(|e| e.name.as_str()).collect();
        assert!(routine_names.contains(&"Recess"), "3/5 = 60% should meet threshold: {:?}", routine_names);
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

        // All routine events should be detected.
        assert!(routine_names.contains(&"Breakfast"), "Missing Breakfast: {:?}", routine_names);
        assert!(routine_names.contains(&"Morning Meeting"), "Missing Morning Meeting: {:?}", routine_names);
        assert!(routine_names.contains(&"Snack"), "Missing Snack: {:?}", routine_names);
        assert!(routine_names.contains(&"Lunch"), "Missing Lunch: {:?}", routine_names);
        assert!(routine_names.contains(&"Pack Up"), "Missing Pack Up: {:?}", routine_names);
        assert!(routine_names.contains(&"Dismissal"), "Missing Dismissal: {:?}", routine_names);

        // Recess appears at two different times — should appear once (deduplicated by name).
        let recess_count = template.daily_routine.iter().filter(|e| e.name == "Recess").count();
        assert_eq!(recess_count, 1, "Recess should be deduplicated to 1 entry");

        // Academic subjects should NOT be routine events.
        assert!(!routine_names.contains(&"Math Workshop"));
        assert!(!routine_names.contains(&"Reading Block"));
        assert!(!routine_names.contains(&"Writing"));
        assert!(!routine_names.contains(&"Science"));
        assert!(!routine_names.contains(&"Social Studies"));

        // The specials row has different activities each day (Art, Music, PE, Library) —
        // each individual one appears in <60% of days, so none should be in daily_routine
        // as individual entries. But Art appears 2/5 = 40% — below threshold.
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
}
