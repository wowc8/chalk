import { useState, useCallback } from "react";

interface ImportDropZoneProps {
  onImport: (filePath?: string) => void;
  importing: boolean;
}

export function ImportDropZone({ onImport, importing }: ImportDropZoneProps) {
  const [dragOver, setDragOver] = useState(false);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOver(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOver(false);
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setDragOver(false);

      const files = e.dataTransfer.files;
      if (files.length > 0) {
        const file = files[0];
        if (file.name.match(/\.html?$/i)) {
          // For Tauri, we need the file path. dataTransfer gives us the path on desktop.
          const path = (file as any).path || file.name;
          onImport(path);
        }
      }
    },
    [onImport],
  );

  return (
    <div
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      className={`w-[420px] p-10 rounded-xl border-2 border-dashed transition-all text-center ${
        dragOver
          ? "border-chalk-blue/50 bg-chalk-blue/5"
          : "border-chalk-white/10 bg-chalk-board-dark/30 hover:border-chalk-white/20"
      }`}
    >
      <div className="w-14 h-14 mx-auto mb-4 rounded-2xl bg-chalk-board-dark border border-chalk-white/8 flex items-center justify-center">
        <svg
          className={`w-7 h-7 transition-colors ${dragOver ? "text-chalk-blue" : "text-chalk-muted"}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.5}
            d="M9 13h6m-3-3v6m5 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
          />
        </svg>
      </div>

      <h3 className="text-base font-medium text-chalk-white mb-1">
        Import Long-Term Plan
      </h3>
      <p className="text-chalk-muted text-sm mb-5 leading-relaxed">
        Drop an HTML file here, or click to browse.
        <br />
        <span className="text-chalk-muted/60 text-xs">
          Exported Google Sheets (File &rarr; Download &rarr; Web Page)
        </span>
      </p>

      <button
        onClick={() => onImport()}
        disabled={importing}
        className="btn btn-primary"
      >
        {importing ? (
          <>
            <span className="spinner spinner-sm" style={{ width: 14, height: 14, borderWidth: 1.5 }} />
            Importing...
          </>
        ) : (
          <>
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
            </svg>
            Choose File
          </>
        )}
      </button>
    </div>
  );
}
