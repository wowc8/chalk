import { useState, useCallback } from "react";

interface ImportDropZoneProps {
  onImport: (filePath?: string) => void;
  onImportUrl: (url: string) => void;
  importing: boolean;
}

export function ImportDropZone({ onImport, onImportUrl, importing }: ImportDropZoneProps) {
  const [dragOver, setDragOver] = useState(false);
  const [showUrlInput, setShowUrlInput] = useState(false);
  const [url, setUrl] = useState("");

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

  function handleUrlSubmit() {
    const trimmed = url.trim();
    if (!trimmed) return;
    onImportUrl(trimmed);
    setUrl("");
    setShowUrlInput(false);
  }

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

      <div className="flex flex-col gap-2.5">
        <button
          onClick={() => onImport()}
          disabled={importing}
          className="btn btn-primary w-full"
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

        {!showUrlInput ? (
          <button
            onClick={() => setShowUrlInput(true)}
            disabled={importing}
            className="btn btn-secondary w-full text-xs"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1" />
            </svg>
            Paste URL
          </button>
        ) : (
          <div className="flex flex-col gap-2">
            <input
              type="url"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleUrlSubmit();
                if (e.key === "Escape") {
                  setShowUrlInput(false);
                  setUrl("");
                }
              }}
              placeholder="https://docs.google.com/spreadsheets/d/..."
              autoFocus
              className="w-full px-3 py-2 rounded-lg text-xs bg-chalk-board-dark border border-chalk-white/15 text-chalk-white placeholder:text-chalk-muted/40 focus:outline-none focus:border-chalk-blue/40 focus:ring-1 focus:ring-chalk-blue/20"
            />
            <div className="flex gap-2">
              <button
                onClick={handleUrlSubmit}
                disabled={importing || !url.trim()}
                className="btn btn-primary flex-1 text-xs"
              >
                {importing ? (
                  <>
                    <span className="spinner spinner-sm" style={{ width: 12, height: 12, borderWidth: 1.5 }} />
                    Importing...
                  </>
                ) : (
                  "Import from URL"
                )}
              </button>
              <button
                onClick={() => {
                  setShowUrlInput(false);
                  setUrl("");
                }}
                className="btn btn-secondary text-xs"
              >
                Cancel
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
