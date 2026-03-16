import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";

interface DriveFolder {
  id: string;
  name: string;
  mime_type: string;
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

  useEffect(() => {
    loadFolders();
  }, []);

  const loadFolders = async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<DriveFolder[]>("list_drive_folders");
      setFolders(result);
    } catch (e) {
      setError(`Failed to load folders: ${e}`);
    } finally {
      setLoading(false);
    }
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
      <p className="text-gray-400 text-sm mb-6">
        Choose the Google Drive folder containing your lesson plans.
        Chalk will read and index documents from this folder.
      </p>

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
            No folders found in your Drive. Create a folder with your
            lesson plans and try again.
          </p>
          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={loadFolders}
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
