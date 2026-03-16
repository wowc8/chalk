import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";

interface DriveFolder {
  id: string;
  name: string;
  mime_type: string;
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
  const [folders, setFolders] = useState<DriveFolder[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [breadcrumb, setBreadcrumb] = useState<BreadcrumbEntry[]>([
    { id: "root", name: "My Drive" },
  ]);

  const currentParentId = breadcrumb[breadcrumb.length - 1].id;

  const loadFolders = useCallback(async (parentId: string) => {
    setLoading(true);
    setError(null);
    setSelectedId(null);
    try {
      let result: DriveFolder[];
      if (parentId === "root") {
        result = await invoke<DriveFolder[]>("list_drive_folders");
      } else {
        result = await invoke<DriveFolder[]>("list_drive_subfolders", {
          parentId,
        });
      }
      setFolders(result);
    } catch (e) {
      setError(`Failed to load folders: ${e}`);
    } finally {
      setLoading(false);
    }
  }, [setError]);

  useEffect(() => {
    loadFolders(currentParentId);
  }, [currentParentId, loadFolders]);

  const handleDrillIn = (folder: DriveFolder) => {
    setBreadcrumb((prev) => [...prev, { id: folder.id, name: folder.name }]);
  };

  const handleBreadcrumbNav = (index: number) => {
    setBreadcrumb((prev) => prev.slice(0, index + 1));
  };

  const handleSelect = async () => {
    if (!selectedId) {
      setError("Please select a folder.");
      return;
    }

    const folder = folders.find((f) => f.id === selectedId);
    if (!folder) return;

    setTesting(true);
    setError(null);
    try {
      const accessible = await invoke<boolean>(
        "test_folder_permissions_command",
        { folderId: selectedId, folderName: folder.name }
      );
      if (accessible) {
        onNext();
      } else {
        setError(
          "Cannot access this folder. Check that Chalk has permission to read it."
        );
      }
    } catch (e) {
      setError(`Permission test failed: ${e}`);
    } finally {
      setTesting(false);
    }
  };

  return (
    <div>
      <h2 className="text-2xl font-bold text-bat-cyan mb-2">
        Select Lesson Plan Folder
      </h2>
      <p className="text-gray-400 text-sm mb-4">
        Choose the Google Drive folder containing your lesson plans.
        Chalk will read and index documents from this folder.
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
          <span className="ml-3 text-gray-400">Loading folders...</span>
        </div>
      ) : folders.length === 0 ? (
        <div className="text-center py-8">
          <p className="text-gray-500 mb-4">
            {breadcrumb.length > 1
              ? "No subfolders in this folder."
              : "No folders found in your Drive. Create a folder with your lesson plans and try again."}
          </p>
          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={() => loadFolders(currentParentId)}
            className="px-4 py-2 border border-bat-cyan rounded-lg text-bat-cyan hover:bg-bat-cyan/10 transition-colors"
          >
            Refresh
          </motion.button>
        </div>
      ) : (
        <div className="max-h-64 overflow-y-auto mb-6 space-y-1 pr-1 scrollbar-thin scrollbar-thumb-bat-purple/40">
          {folders.map((folder) => (
            <motion.button
              key={folder.id}
              whileHover={{ x: 4 }}
              onClick={() => setSelectedId(folder.id)}
              onDoubleClick={() => handleDrillIn(folder)}
              className={`w-full flex items-center gap-3 px-4 py-3 rounded-lg text-left transition-colors ${
                selectedId === folder.id
                  ? "bg-bat-purple/30 border border-bat-cyan/50"
                  : "bg-bat-charcoal/50 border border-transparent hover:bg-bat-charcoal"
              }`}
            >
              <svg
                className={`w-5 h-5 flex-shrink-0 ${
                  selectedId === folder.id ? "text-bat-cyan" : "text-gray-500"
                }`}
                fill="currentColor"
                viewBox="0 0 20 20"
              >
                <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z" />
              </svg>
              <span className="truncate">{folder.name}</span>
              {selectedId === folder.id && (
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
                  handleDrillIn(folder);
                }}
                className="ml-auto p-1 text-gray-500 hover:text-bat-cyan transition-colors"
                title="Browse subfolders"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                </svg>
              </button>
            </motion.button>
          ))}
        </div>
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
          {testing ? "Testing access..." : "Select & Continue"}
        </motion.button>
      </div>
    </div>
  );
}
