import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useToast } from "../Toast";

interface LtpDocument {
  id: string;
  filename: string;
  doc_type: string;
}

interface GridCell {
  id: string;
  document_id: string;
  row_index: number;
  col_index: number;
  subject: string | null;
  month: string | null;
  content_html: string | null;
  content_text: string | null;
  background_color: string | null;
  unit_name: string | null;
  unit_color: string | null;
}

interface LtpGridProps {
  document: LtpDocument;
}

/** Lighten a hex color for readable dark text on colored backgrounds. */
function cellBgStyle(color: string | null): React.CSSProperties {
  if (!color) return {};
  return {
    backgroundColor: `${color}30`,
    borderColor: `${color}50`,
  };
}

/** Build the 2D grid from flat cell array. */
function buildGrid(cells: GridCell[]): {
  rows: Map<number, Map<number, GridCell>>;
  maxRow: number;
  maxCol: number;
  months: string[];
  subjects: string[];
} {
  const rows = new Map<number, Map<number, GridCell>>();
  let maxRow = 0;
  let maxCol = 0;
  const monthSet = new Map<number, string>();
  const subjectSet = new Map<number, string>();

  for (const cell of cells) {
    if (!rows.has(cell.row_index)) {
      rows.set(cell.row_index, new Map());
    }
    rows.get(cell.row_index)!.set(cell.col_index, cell);
    maxRow = Math.max(maxRow, cell.row_index);
    maxCol = Math.max(maxCol, cell.col_index);

    if (cell.month && !monthSet.has(cell.col_index)) {
      monthSet.set(cell.col_index, cell.month);
    }
    if (cell.subject && !subjectSet.has(cell.row_index)) {
      subjectSet.set(cell.row_index, cell.subject);
    }
  }

  // Deduplicate months by column position
  const months = Array.from(monthSet.entries())
    .sort(([a], [b]) => a - b)
    .map(([, m]) => m);

  const subjects = Array.from(subjectSet.entries())
    .sort(([a], [b]) => a - b)
    .map(([, s]) => s);

  return { rows, maxRow, maxCol, months, subjects };
}

export function LtpGrid({ document }: LtpGridProps) {
  const { addToast } = useToast();
  const [cells, setCells] = useState<GridCell[]>([]);
  const [loading, setLoading] = useState(true);
  const [editingCell, setEditingCell] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");
  const editInputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);

    async function load() {
      try {
        const data = await invoke<GridCell[]>("get_ltp_grid_cells", {
          documentId: document.id,
        });
        if (!cancelled) {
          setCells(data);
        }
      } catch (e) {
        if (!cancelled) {
          addToast(`Failed to load grid: ${e}`, "error");
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    load();
    return () => { cancelled = true; };
  }, [document.id]);

  useEffect(() => {
    if (editingCell && editInputRef.current) {
      editInputRef.current.focus();
      editInputRef.current.select();
    }
  }, [editingCell]);

  const handleCellClick = useCallback((cell: GridCell) => {
    setEditingCell(cell.id);
    setEditValue(cell.content_text || "");
  }, []);

  const handleSaveCell = useCallback(async () => {
    if (!editingCell) return;

    const originalCell = cells.find((c) => c.id === editingCell);
    if (!originalCell || editValue === (originalCell.content_text || "")) {
      setEditingCell(null);
      return;
    }

    try {
      await invoke("update_ltp_grid_cell", {
        cellId: editingCell,
        contentText: editValue,
      });
      setCells((prev) =>
        prev.map((c) =>
          c.id === editingCell ? { ...c, content_text: editValue } : c,
        ),
      );
    } catch (e) {
      addToast(`Failed to save: ${e}`, "error");
    }
    setEditingCell(null);
  }, [editingCell, editValue, cells]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Escape") {
        setEditingCell(null);
      } else if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSaveCell();
      }
    },
    [handleSaveCell],
  );

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="spinner" />
        <span className="ml-3 text-chalk-muted text-sm">Loading grid...</span>
      </div>
    );
  }

  if (cells.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <p className="text-chalk-muted text-sm">
          No grid data found for this document.
        </p>
      </div>
    );
  }

  const { rows, maxRow, maxCol } = buildGrid(cells);

  return (
    <div className="h-full overflow-auto ltp-grid-container">
      <table className="ltp-grid-table">
        <tbody>
          {Array.from({ length: maxRow + 1 }, (_, rowIdx) => {
            const row = rows.get(rowIdx);
            if (!row) return null;

            return (
              <tr key={rowIdx}>
                {Array.from({ length: maxCol + 1 }, (_, colIdx) => {
                  const cell = row?.get(colIdx);
                  if (!cell) {
                    return <td key={colIdx} className="ltp-cell ltp-cell-empty" />;
                  }

                  const isEditing = editingCell === cell.id;
                  const isFirstCol = colIdx === 0;
                  const isFirstRow = rowIdx === 0;
                  const bgStyle = cellBgStyle(cell.background_color || cell.unit_color);
                  const text = cell.content_text || "";

                  // First column is subject labels
                  if (isFirstCol && cell.subject) {
                    return (
                      <th
                        key={colIdx}
                        className="ltp-cell ltp-cell-header ltp-cell-subject"
                        title={cell.subject}
                      >
                        <span className="ltp-cell-subject-text">{cell.subject}</span>
                      </th>
                    );
                  }

                  // First row is month headers
                  if (isFirstRow && cell.month) {
                    return (
                      <th key={colIdx} className="ltp-cell ltp-cell-header ltp-cell-month">
                        {cell.month}
                      </th>
                    );
                  }

                  return (
                    <td
                      key={colIdx}
                      className={`ltp-cell ${isEditing ? "ltp-cell-editing" : "ltp-cell-interactive"}`}
                      style={bgStyle}
                      onClick={() => !isEditing && handleCellClick(cell)}
                      title={cell.unit_name ? `Unit: ${cell.unit_name}` : undefined}
                    >
                      {isEditing ? (
                        <textarea
                          ref={editInputRef}
                          value={editValue}
                          onChange={(e) => setEditValue(e.target.value)}
                          onBlur={handleSaveCell}
                          onKeyDown={handleKeyDown}
                          className="ltp-cell-input"
                          rows={3}
                        />
                      ) : (
                        <span className="ltp-cell-text">{text}</span>
                      )}
                    </td>
                  );
                })}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
