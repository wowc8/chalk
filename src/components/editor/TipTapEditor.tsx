import { useEditor, EditorContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Placeholder from "@tiptap/extension-placeholder";
import Underline from "@tiptap/extension-underline";
import TextAlign from "@tiptap/extension-text-align";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Table from "@tiptap/extension-table";
import TableRow from "@tiptap/extension-table-row";
import { CustomTableHeader } from "./CustomTableHeader";
import { CustomTableCell } from "./CustomTableCell";
import Color from "@tiptap/extension-color";
import { TextStyle } from "@tiptap/extension-text-style";
import Highlight from "@tiptap/extension-highlight";
import Image from "@tiptap/extension-image";
import { useCallback, useEffect, useRef, useState } from "react";

interface TipTapEditorProps {
  content: string;
  onUpdate: (content: string) => void;
  editable?: boolean;
}

const TEXT_COLORS = [
  { label: "Default", value: "" },
  { label: "White", value: "#e8e4df" },
  { label: "Red", value: "#ff6b6b" },
  { label: "Orange", value: "#fdcb6e" },
  { label: "Yellow", value: "#ffeaa7" },
  { label: "Green", value: "#55efc4" },
  { label: "Blue", value: "#74b9ff" },
  { label: "Purple", value: "#a29bfe" },
  { label: "Pink", value: "#fd79a8" },
];

const HIGHLIGHT_COLORS = [
  { label: "None", value: "" },
  { label: "Yellow", value: "#ffeaa7" },
  { label: "Green", value: "#55efc4" },
  { label: "Blue", value: "#74b9ff" },
  { label: "Purple", value: "#a29bfe" },
  { label: "Pink", value: "#fd79a8" },
  { label: "Orange", value: "#fdcb6e" },
  { label: "Red", value: "#ff6b6b" },
];

function ColorPicker({
  colors,
  activeColor,
  onSelect,
  onClose,
}: {
  colors: { label: string; value: string }[];
  activeColor: string;
  onSelect: (color: string) => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [onClose]);

  return (
    <div
      ref={ref}
      className="absolute top-full left-0 mt-1 p-1.5 bg-chalk-board-dark border border-chalk-white/12 rounded-lg shadow-xl z-50 grid grid-cols-3 gap-1"
    >
      {colors.map((c) => (
        <button
          key={c.label}
          onClick={() => {
            onSelect(c.value);
            onClose();
          }}
          className={`w-7 h-7 rounded border transition-all ${
            activeColor === c.value
              ? "border-chalk-white ring-1 ring-chalk-white/30 scale-110"
              : "border-chalk-white/10 hover:border-chalk-white/30 hover:scale-105"
          }`}
          style={{
            background: c.value || "transparent",
            ...(c.value === "" && {
              backgroundImage:
                "linear-gradient(135deg, transparent 45%, rgba(255,100,100,0.6) 45%, rgba(255,100,100,0.6) 55%, transparent 55%)",
            }),
          }}
          title={c.label}
        />
      ))}
    </div>
  );
}

function EditorToolbar({ editor }: { editor: ReturnType<typeof useEditor> }) {
  const [showTextColor, setShowTextColor] = useState(false);
  const [showHighlight, setShowHighlight] = useState(false);
  const [showTableMenu, setShowTableMenu] = useState(false);

  if (!editor) return null;

  const btnClass = (active: boolean) =>
    `p-1.5 rounded transition-colors ${
      active
        ? "bg-chalk-white/15 text-chalk-white"
        : "text-chalk-muted hover:text-chalk-white hover:bg-chalk-white/8"
    }`;

  const currentTextColor =
    (editor.getAttributes("textStyle").color as string) || "";
  const currentHighlight =
    (editor.getAttributes("highlight").color as string) || "";

  return (
    <div className="flex items-center gap-0.5 px-3 py-2 border-b border-chalk-white/8 flex-wrap">
      {/* Bold / Italic / Underline */}
      <button
        onClick={() => editor.chain().focus().toggleBold().run()}
        className={btnClass(editor.isActive("bold"))}
        title="Bold"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M6 4h8a4 4 0 014 4 4 4 0 01-4 4H6z" />
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M6 12h9a4 4 0 014 4 4 4 0 01-4 4H6z" />
        </svg>
      </button>
      <button
        onClick={() => editor.chain().focus().toggleItalic().run()}
        className={btnClass(editor.isActive("italic"))}
        title="Italic"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 4h4m-2 0l-4 16m0 0h4" />
        </svg>
      </button>
      <button
        onClick={() => editor.chain().focus().toggleUnderline().run()}
        className={btnClass(editor.isActive("underline"))}
        title="Underline"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 4v7a5 5 0 0010 0V4M5 20h14" />
        </svg>
      </button>

      <div className="w-px h-5 bg-chalk-white/10 mx-1" />

      {/* Text Color */}
      <div className="relative">
        <button
          onClick={() => {
            setShowTextColor(!showTextColor);
            setShowHighlight(false);
            setShowTableMenu(false);
          }}
          className={btnClass(!!currentTextColor)}
          title="Text Color"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5l-4 14M17 5l-4 14M4 12h16" />
          </svg>
          <div
            className="absolute bottom-0.5 left-1/2 -translate-x-1/2 w-3 h-0.5 rounded-full"
            style={{ background: currentTextColor || "#8a9ba8" }}
          />
        </button>
        {showTextColor && (
          <ColorPicker
            colors={TEXT_COLORS}
            activeColor={currentTextColor}
            onSelect={(color) => {
              if (color) {
                editor.chain().focus().setColor(color).run();
              } else {
                editor.chain().focus().unsetColor().run();
              }
            }}
            onClose={() => setShowTextColor(false)}
          />
        )}
      </div>

      {/* Highlight Color */}
      <div className="relative">
        <button
          onClick={() => {
            setShowHighlight(!showHighlight);
            setShowTextColor(false);
            setShowTableMenu(false);
          }}
          className={btnClass(editor.isActive("highlight"))}
          title="Highlight Color"
        >
          <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
          </svg>
          <div
            className="absolute bottom-0.5 left-1/2 -translate-x-1/2 w-3 h-0.5 rounded-full"
            style={{ background: currentHighlight || "#8a9ba8" }}
          />
        </button>
        {showHighlight && (
          <ColorPicker
            colors={HIGHLIGHT_COLORS}
            activeColor={currentHighlight}
            onSelect={(color) => {
              if (color) {
                editor.chain().focus().toggleHighlight({ color }).run();
              } else {
                editor.chain().focus().unsetHighlight().run();
              }
            }}
            onClose={() => setShowHighlight(false)}
          />
        )}
      </div>

      <div className="w-px h-5 bg-chalk-white/10 mx-1" />

      {/* Headings */}
      <button
        onClick={() => editor.chain().focus().toggleHeading({ level: 1 }).run()}
        className={btnClass(editor.isActive("heading", { level: 1 }))}
        title="Heading 1"
      >
        <span className="text-xs font-bold">H1</span>
      </button>
      <button
        onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
        className={btnClass(editor.isActive("heading", { level: 2 }))}
        title="Heading 2"
      >
        <span className="text-xs font-bold">H2</span>
      </button>
      <button
        onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}
        className={btnClass(editor.isActive("heading", { level: 3 }))}
        title="Heading 3"
      >
        <span className="text-xs font-bold">H3</span>
      </button>

      <div className="w-px h-5 bg-chalk-white/10 mx-1" />

      {/* Text Alignment */}
      <button
        onClick={() => editor.chain().focus().setTextAlign("left").run()}
        className={btnClass(editor.isActive({ textAlign: "left" }))}
        title="Align Left"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 6h18M3 12h12M3 18h18" />
        </svg>
      </button>
      <button
        onClick={() => editor.chain().focus().setTextAlign("center").run()}
        className={btnClass(editor.isActive({ textAlign: "center" }))}
        title="Align Center"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 6h18M6 12h12M3 18h18" />
        </svg>
      </button>
      <button
        onClick={() => editor.chain().focus().setTextAlign("right").run()}
        className={btnClass(editor.isActive({ textAlign: "right" }))}
        title="Align Right"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 6h18M9 12h12M3 18h18" />
        </svg>
      </button>
      <button
        onClick={() => editor.chain().focus().setTextAlign("justify").run()}
        className={btnClass(editor.isActive({ textAlign: "justify" }))}
        title="Justify"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 6h18M3 12h18M3 18h18" />
        </svg>
      </button>

      <div className="w-px h-5 bg-chalk-white/10 mx-1" />

      {/* Lists */}
      <button
        onClick={() => editor.chain().focus().toggleBulletList().run()}
        className={btnClass(editor.isActive("bulletList"))}
        title="Bullet List"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" />
        </svg>
      </button>
      <button
        onClick={() => editor.chain().focus().toggleOrderedList().run()}
        className={btnClass(editor.isActive("orderedList"))}
        title="Ordered List"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 6h13M7 12h13M7 18h13" />
          <text x="2" y="8" fontSize="6" fill="currentColor" fontWeight="bold">1</text>
          <text x="2" y="14" fontSize="6" fill="currentColor" fontWeight="bold">2</text>
          <text x="2" y="20" fontSize="6" fill="currentColor" fontWeight="bold">3</text>
        </svg>
      </button>
      <button
        onClick={() => editor.chain().focus().toggleTaskList().run()}
        className={btnClass(editor.isActive("taskList"))}
        title="Checklist"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4" />
        </svg>
      </button>

      <div className="w-px h-5 bg-chalk-white/10 mx-1" />

      {/* Blockquote & Divider */}
      <button
        onClick={() => editor.chain().focus().toggleBlockquote().run()}
        className={btnClass(editor.isActive("blockquote"))}
        title="Quote"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z" />
        </svg>
      </button>
      <button
        onClick={() => editor.chain().focus().setHorizontalRule().run()}
        className={btnClass(false)}
        title="Divider"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 12h16" />
        </svg>
      </button>

      <div className="w-px h-5 bg-chalk-white/10 mx-1" />

      {/* Table controls */}
      <div className="relative">
        <button
          onClick={() => {
            setShowTableMenu(!showTableMenu);
            setShowTextColor(false);
            setShowHighlight(false);
          }}
          className={btnClass(false)}
          title="Table"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 10h18M3 14h18M10 3v18M14 3v18M3 6a3 3 0 013-3h12a3 3 0 013 3v12a3 3 0 01-3 3H6a3 3 0 01-3-3V6z" />
          </svg>
        </button>
        {showTableMenu && (
          <TableMenu editor={editor} onClose={() => setShowTableMenu(false)} />
        )}
      </div>

      {/* Image */}
      <button
        onClick={() => {
          const url = window.prompt("Image URL:");
          if (url) {
            editor.chain().focus().setImage({ src: url }).run();
          }
        }}
        className={btnClass(false)}
        title="Insert Image"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
      </button>

      {/* Undo / Redo */}
      <button
        onClick={() => editor.chain().focus().undo().run()}
        disabled={!editor.can().undo()}
        className={`p-1.5 rounded transition-colors ml-auto ${
          editor.can().undo()
            ? "text-chalk-muted hover:text-chalk-white hover:bg-chalk-white/8"
            : "text-chalk-muted/30 cursor-not-allowed"
        }`}
        title="Undo"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 10h10a5 5 0 015 5v2M3 10l4-4M3 10l4 4" />
        </svg>
      </button>
      <button
        onClick={() => editor.chain().focus().redo().run()}
        disabled={!editor.can().redo()}
        className={`p-1.5 rounded transition-colors ${
          editor.can().redo()
            ? "text-chalk-muted hover:text-chalk-white hover:bg-chalk-white/8"
            : "text-chalk-muted/30 cursor-not-allowed"
        }`}
        title="Redo"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 10H11a5 5 0 00-5 5v2M21 10l-4-4M21 10l-4 4" />
        </svg>
      </button>
    </div>
  );
}

function TableMenu({
  editor,
  onClose,
}: {
  editor: NonNullable<ReturnType<typeof useEditor>>;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [onClose]);

  const isInTable = editor.isActive("table");

  const menuBtnClass =
    "w-full text-left px-3 py-1.5 text-xs rounded hover:bg-chalk-white/8 text-chalk-dust hover:text-chalk-white transition-colors disabled:opacity-30 disabled:cursor-not-allowed";

  return (
    <div
      ref={ref}
      className="absolute top-full left-0 mt-1 py-1 bg-chalk-board-dark border border-chalk-white/12 rounded-lg shadow-xl z-50 min-w-[160px]"
    >
      <button
        className={menuBtnClass}
        onClick={() => {
          editor.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run();
          onClose();
        }}
      >
        Insert Table
      </button>
      <div className="h-px bg-chalk-white/8 my-1" />
      <button
        className={menuBtnClass}
        disabled={!isInTable}
        onClick={() => {
          editor.chain().focus().addColumnBefore().run();
          onClose();
        }}
      >
        Add Column Before
      </button>
      <button
        className={menuBtnClass}
        disabled={!isInTable}
        onClick={() => {
          editor.chain().focus().addColumnAfter().run();
          onClose();
        }}
      >
        Add Column After
      </button>
      <button
        className={menuBtnClass}
        disabled={!isInTable}
        onClick={() => {
          editor.chain().focus().deleteColumn().run();
          onClose();
        }}
      >
        Delete Column
      </button>
      <div className="h-px bg-chalk-white/8 my-1" />
      <button
        className={menuBtnClass}
        disabled={!isInTable}
        onClick={() => {
          editor.chain().focus().addRowBefore().run();
          onClose();
        }}
      >
        Add Row Before
      </button>
      <button
        className={menuBtnClass}
        disabled={!isInTable}
        onClick={() => {
          editor.chain().focus().addRowAfter().run();
          onClose();
        }}
      >
        Add Row After
      </button>
      <button
        className={menuBtnClass}
        disabled={!isInTable}
        onClick={() => {
          editor.chain().focus().deleteRow().run();
          onClose();
        }}
      >
        Delete Row
      </button>
      <div className="h-px bg-chalk-white/8 my-1" />
      <button
        className={menuBtnClass}
        disabled={!isInTable}
        onClick={() => {
          editor.chain().focus().mergeCells().run();
          onClose();
        }}
      >
        Merge Cells
      </button>
      <button
        className={menuBtnClass}
        disabled={!isInTable}
        onClick={() => {
          editor.chain().focus().splitCell().run();
          onClose();
        }}
      >
        Split Cell
      </button>
      <button
        className={menuBtnClass}
        disabled={!isInTable}
        onClick={() => {
          editor.chain().focus().toggleHeaderRow().run();
          onClose();
        }}
      >
        Toggle Header Row
      </button>
      <div className="h-px bg-chalk-white/8 my-1" />
      <button
        className={`${menuBtnClass} !text-red-400 hover:!text-red-300`}
        disabled={!isInTable}
        onClick={() => {
          editor.chain().focus().deleteTable().run();
          onClose();
        }}
      >
        Delete Table
      </button>
    </div>
  );
}

export function TipTapEditor({ content, onUpdate, editable = true }: TipTapEditorProps) {
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleUpdate = useCallback(
    (html: string) => {
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current);
      }
      saveTimerRef.current = setTimeout(() => {
        onUpdate(html);
      }, 500);
    },
    [onUpdate]
  );

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: {
          levels: [1, 2, 3],
        },
      }),
      Placeholder.configure({
        placeholder: "Start writing your lesson plan...",
      }),
      Underline,
      TextStyle,
      Color,
      Highlight.configure({
        multicolor: true,
      }),
      TextAlign.configure({
        types: ["heading", "paragraph"],
      }),
      TaskList,
      TaskItem.configure({
        nested: true,
      }),
      Table.configure({
        resizable: true,
      }),
      TableRow,
      CustomTableHeader,
      CustomTableCell,
      Image.configure({
        inline: false,
        allowBase64: true,
      }),
    ],
    content,
    editable,
    onUpdate: ({ editor }) => {
      handleUpdate(editor.getHTML());
    },
    editorProps: {
      attributes: {
        class: "chalk-editor-content",
      },
    },
  });

  useEffect(() => {
    if (editor && content !== editor.getHTML()) {
      editor.commands.setContent(content, false);
    }
  }, [content]);

  useEffect(() => {
    return () => {
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current);
      }
    };
  }, []);

  return (
    <div className="flex flex-col h-full">
      <EditorToolbar editor={editor} />
      <div className="flex-1 overflow-y-auto px-6 py-4">
        <EditorContent editor={editor} />
      </div>
    </div>
  );
}
