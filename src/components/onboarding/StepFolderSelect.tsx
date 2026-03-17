import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";

interface DriveFolder {
  id: string;
  name: string;
  mime_type: string;
}

interface DriveItem {
  id: string;
  name: string;
  mime_type: string;
  is_folder: boolean;
}

interface BreadcrumbEntry {
  id: string;
  name: string;
}

interface Props {
  onNext: () => void;
  onBack: () => void;
  setError: (err: string | null) => void;
}

export function StepFolderSelect({ onNext, onBack, setError }: Props) {
  const [items, setItems] = useState<DriveItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [breadcrumb, setBreadcrumb] = useState<BreadcrumbEntry[]>([
    { id: "root", name: "My Drive" },
  ]);

  const currentParentId = breadcrumb[breadcrumb.length - 1].id;

  const selectedItem = items.find((i) => i.id === selectedId);

  const loadItems = useCallback(
    async (parentId: string) => {
      setLoading(true);
      setError(null);
      setSelectedId(null);
      try {
        const result = await invoke<DriveItem[]>("list_drive_items", {
          parentId,
        });
        setItems(result);
      } catch (e) {
        // Fallback to folder-only listing
        try {
          let folders: DriveFolder[];
          if (parentId === "root") {
            folders = await invoke<DriveFolder[]>("list_drive_folders");
          } else {
            folders = await invoke<DriveFolder[]>("list_drive_subfolders", {
              parentId,
            });
          }
          setItems(
            folders.map((f) => ({ ...f, is_folder: true }))
          );
        } catch (e2) {
          setError(`Failed to load items: ${e2}`);
        }
      } finally {
        setLoading(false);
      }
    },
    [setError]
  );

  useEffect(() => {
    loadItems(currentParentId);
  }, [currentParentId, loadItems]);

  const handleDrillIn = (item: DriveItem) => {
    if (item.is_folder) {
      setBreadcrumb((prev) => [...prev, { id: item.id, name: item.name }]);
    }
  };

  const handleBreadcrumbNav = (index: number) => {
    setBreadcrumb((prev) => prev.slice(0, index + 1));
  };

  const handleSelect = async () => {
    if (!selectedId || !selectedItem) {
      setError("Please select a folder or document.");
      return;
    }

    setTesting(true);
    setError(null);
    try {
      if (selectedItem.is_folder) {
        // Folder selection (existing flow)
        const accessible = await invoke<boolean>(
          "test_folder_permissions_command",
          { folderId: selectedId, folderName: selectedItem.name }
        );
        if (accessible) {
          onNext();
        } else {
          setError(
            "Cannot access this folder. Check that Chalk has permission to read it."
          );
        }
      } else {
        // Single document selection (LPA-9)
        const accessible = await invoke<boolean>("select_single_document", {
          docId: selectedId,
          docName: selectedItem.name,
        });
        if (accessible) {
          onNext();
        } else {
          setError(
            "Cannot access this document. Check that Chalk has permission to read it."
          );
        }
      }
    } catch (e) {
      setError(`Permission test failed: ${e}`);
    } finally {
      setTesting(false);
    }
  };

  const folders = items.filter((i) => i.is_folder);
  const docs = items.filter((i) => !i.is_folder);

  return (
    <div>
      <h2 className="text-2xl font-bold text-bat-cyan mb-2">
        Select Your Lesson Plans
      </h2>
      <p className="text-gray-400 text-sm mb-4">
        Choose a folder containing your lesson plans, or select a single Google
        Doc. Chalk will read and index your selection.
      </p>

      {/* Breadcrumb navigation */}
      <nav className="flex items-center gap-1 text-sm mb-4 text-gray-400 overflow-x-auto">
        {breadcrumb.map((entry, i) => (
          <span key={entry.id} className="flex items-center gap-1 shrink-0">
            {i > 0 && <span className="text-gray-600">/</span>}
            {i < breadcrumb.length - 1 ? (
              <button
                onClick={() => handleBreadcrumbNav(i)}
                className="hover:text-bat-cyan transition-colors"
              >
                {entry.name}
              </button>
            ) : (
              <span className="text-white font-medium">{entry.name}</span>
            )}
          </span>
        ))}
      </nav>

      {loading ? (
        <div className="flex items-center justify-center py-12">
          <motion.div
            animate={{ rotate: 360 }}
            transition={{ duration: 1.5, repeat: Infinity, ease: "linear" }}
            className="w-8 h-8 border-2 border-bat-cyan border-t-transparent rounded-full"
          />
          <span className="ml-3 text-gray-400">Loading...</span>
        </div>
      ) : items.length === 0 ? (
        <div className="text-center py-8">
          <p className="text-gray-500 mb-4">
            {breadcrumb.length > 1
              ? "No items in this folder."
              : "No folders or documents found in your Drive."}
          </p>
          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={() => loadItems(currentParentId)}
            className="px-4 py-2 border border-bat-cyan rounded-lg text-bat-cyan hover:bg-bat-cyan/10 transition-colors"
          >
            Refresh
          </motion.button>
        </div>
      ) : (
        <div className="max-h-64 overflow-y-auto overflow-x-hidden mb-6 space-y-1 pr-1 scrollbar-thin scrollbar-thumb-bat-purple/40">
          {/* Folders first */}
          {folders.map((item) => (
            <motion.button
              key={item.id}
              whileHover={{ x: 4 }}
              onClick={() => setSelectedId(item.id)}
              onDoubleClick={() => handleDrillIn(item)}
              className={`w-full flex items-center gap-3 px-4 py-3 rounded-lg text-left transition-colors ${
                selectedId === item.id
                  ? "bg-bat-purple/30 border border-bat-cyan/50"
                  : "bg-bat-charcoal/50 border border-transparent hover:bg-bat-charcoal"
              }`}
            >
              <svg
                className={`w-5 h-5 flex-shrink-0 ${
                  selectedId === item.id ? "text-bat-cyan" : "text-gray-500"
                }`}
                fill="currentColor"
                viewBox="0 0 20 20"
              >
                <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z" />
              </svg>
              <span className="truncate">{item.name}</span>
              {selectedId === item.id && (
                <motion.span
                  initial={{ scale: 0 }}
                  animate={{ scale: 1 }}
                  className="ml-auto text-bat-green text-sm"
                >
                  &#x2713;
                </motion.span>
              )}
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  handleDrillIn(item);
                }}
                className="ml-auto p-1 text-gray-500 hover:text-bat-cyan transition-colors"
                title="Browse subfolders"
              >
                <svg
                  className="w-4 h-4"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M9 5l7 7-7 7"
                  />
                </svg>
              </button>
            </motion.button>
          ))}

          {/* Separator if both folders and docs exist */}
          {folders.length > 0 && docs.length > 0 && (
            <div className="flex items-center gap-2 py-2 px-4">
              <div className="flex-1 h-px bg-bat-charcoal" />
              <span className="text-xs text-gray-600">Documents</span>
              <div className="flex-1 h-px bg-bat-charcoal" />
            </div>
          )}

          {/* Documents */}
          {docs.map((item) => (
            <motion.button
              key={item.id}
              whileHover={{ x: 4 }}
              onClick={() => setSelectedId(item.id)}
              className={`w-full flex items-center gap-3 px-4 py-3 rounded-lg text-left transition-colors ${
                selectedId === item.id
                  ? "bg-bat-purple/30 border border-bat-cyan/50"
                  : "bg-bat-charcoal/50 border border-transparent hover:bg-bat-charcoal"
              }`}
            >
              <svg
                className={`w-5 h-5 flex-shrink-0 ${
                  selectedId === item.id ? "text-bat-cyan" : "text-blue-400/60"
                }`}
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.5}
                  d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                />
              </svg>
              <span className="truncate">{item.name}</span>
              {selectedId === item.id && (
                <motion.span
                  initial={{ scale: 0 }}
                  animate={{ scale: 1 }}
                  className="ml-auto text-bat-green text-sm"
                >
                  &#x2713;
                </motion.span>
              )}
            </motion.button>
          ))}
        </div>
      )}

      {/* Selection hint */}
      {selectedItem && !selectedItem.is_folder && (
        <p className="text-xs text-bat-cyan/70 mb-4">
          Selecting a single document — Chalk will use this as your lesson plan
          source.
        </p>
      )}

      <div className="flex justify-between">
        <motion.button
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
          onClick={onBack}
          className="px-6 py-2.5 border border-gray-600 rounded-lg text-gray-400 hover:text-white hover:border-gray-400 transition-colors"
        >
          Back
        </motion.button>
        <motion.button
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
          onClick={handleSelect}
          disabled={!selectedId || testing}
          className="px-6 py-2.5 bg-gradient-to-r from-bat-cyan to-bat-purple rounded-lg font-semibold text-white disabled:opacity-50 shadow-lg shadow-bat-cyan/20"
        >
          {testing
            ? "Testing access..."
            : selectedItem && !selectedItem.is_folder
              ? "Select Document"
              : "Select & Continue"}
        </motion.button>
      </div>
    </div>
  );
}
