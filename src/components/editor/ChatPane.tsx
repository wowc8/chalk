import { useState, useRef, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useChat, type ChatMessage as BackendMessage } from "../../hooks/useChat";
import { useTeacherName } from "../../hooks/useTeacherName";

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  timestamp: Date;
}

interface ChatPaneProps {
  /** Optional lesson plan ID to provide context for the chat. */
  planId?: string;
  /** External messages (for backwards compatibility). If provided, uses these instead of the hook. */
  messages?: ChatMessage[];
  /** External send handler (for backwards compatibility). */
  onSendMessage?: (message: string) => void;
  /** External loading state (for backwards compatibility). */
  isLoading?: boolean;
}

/** Convert backend message to display format. */
function toDisplayMessage(msg: BackendMessage): ChatMessage {
  return {
    id: msg.id,
    role: msg.role as "user" | "assistant",
    content: msg.content,
    timestamp: new Date(msg.created_at + "Z"),
  };
}

function MessageBubble({ message }: { message: ChatMessage }) {
  const isUser = message.role === "user";

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      className={`flex ${isUser ? "justify-end" : "justify-start"} mb-3`}
    >
      <div
        className={`max-w-[85%] px-3.5 py-2.5 rounded-xl text-sm leading-relaxed ${
          isUser
            ? "bg-chalk-blue/15 border border-chalk-blue/20 text-chalk-white"
            : "bg-chalk-board-dark/80 border border-chalk-white/8 text-chalk-dust"
        }`}
      >
        <div className="whitespace-pre-wrap">{message.content}</div>
        <div
          className={`text-[10px] mt-1.5 ${
            isUser ? "text-chalk-blue/40" : "text-chalk-muted/40"
          }`}
        >
          {message.timestamp.toLocaleTimeString(undefined, {
            hour: "numeric",
            minute: "2-digit",
          })}
        </div>
      </div>
    </motion.div>
  );
}

function TypingIndicator() {
  return (
    <div className="flex justify-start mb-3">
      <div className="bg-chalk-board-dark/80 border border-chalk-white/8 rounded-xl px-4 py-3">
        <div className="flex gap-1.5">
          {[0, 1, 2].map((i) => (
            <motion.div
              key={i}
              className="w-1.5 h-1.5 rounded-full bg-chalk-muted"
              animate={{ opacity: [0.3, 1, 0.3] }}
              transition={{
                duration: 1.2,
                repeat: Infinity,
                delay: i * 0.2,
              }}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function ContextBadge({ count }: { count: number }) {
  if (count === 0) return null;

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0, scale: 0.9 }}
        animate={{ opacity: 1, scale: 1 }}
        exit={{ opacity: 0 }}
        className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-chalk-green/10 border border-chalk-green/20"
      >
        <svg className="w-3 h-3 text-chalk-green" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        <span className="text-[10px] text-chalk-green/80">
          {count} plan{count !== 1 ? "s" : ""} referenced
        </span>
      </motion.div>
    </AnimatePresence>
  );
}

function ErrorBanner({ error, onDismiss }: { error: string; onDismiss: () => void }) {
  return (
    <motion.div
      initial={{ opacity: 0, height: 0 }}
      animate={{ opacity: 1, height: "auto" }}
      exit={{ opacity: 0, height: 0 }}
      className="mx-3 mt-2 px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20 flex items-center justify-between"
    >
      <span className="text-xs text-red-400">{error}</span>
      <button onClick={onDismiss} className="text-red-400/60 hover:text-red-400 ml-2">
        <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </motion.div>
  );
}

export function ChatPane({
  planId,
  messages: externalMessages,
  onSendMessage: externalSendMessage,
  isLoading: externalLoading,
}: ChatPaneProps) {
  // Use the integrated chat hook when no external messages are provided.
  const chat = useChat(planId);
  const isIntegrated = !externalMessages && !externalSendMessage;
  const { name: teacherName } = useTeacherName();

  const displayMessages: ChatMessage[] = isIntegrated
    ? chat.messages
        .filter((m) => m.role === "user" || m.role === "assistant")
        .map(toDisplayMessage)
    : (externalMessages ?? []);

  const loading = isIntegrated ? chat.isLoading : (externalLoading ?? false);
  const error = isIntegrated ? chat.error : null;
  const contextCount = isIntegrated ? chat.lastContextPlans.length : 0;

  const handleSend = isIntegrated
    ? (msg: string) => chat.sendMessage(msg)
    : externalSendMessage!;

  const [input, setInput] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [displayMessages, loading]);

  const handleSendClick = () => {
    const trimmed = input.trim();
    if (!trimmed || loading) return;
    handleSend(trimmed);
    setInput("");
    if (inputRef.current) {
      inputRef.current.style.height = "auto";
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSendClick();
    }
  };

  const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
    const el = e.target;
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, 120) + "px";
  };

  return (
    <div className="flex flex-col h-full">
      {/* Chat header */}
      <div className="flex items-center justify-between px-4 py-2.5 border-b border-chalk-white/8">
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 rounded-full bg-chalk-green animate-pulse" />
          <span className="text-xs font-medium text-chalk-muted">Chalk AI</span>
        </div>
        <div className="flex items-center gap-2">
          <ContextBadge count={contextCount} />
          {isIntegrated && chat.conversationId && (
            <button
              onClick={chat.startNewConversation}
              className="text-[10px] text-chalk-muted/50 hover:text-chalk-muted transition-colors"
              title="New conversation"
            >
              + New
            </button>
          )}
        </div>
      </div>

      {/* Error banner */}
      <AnimatePresence>
        {error && (
          <ErrorBanner
            error={error}
            onDismiss={() => {
              /* Error will clear on next send */
            }}
          />
        )}
      </AnimatePresence>

      {/* Empty state */}
      {displayMessages.length === 0 && !loading && (
        <div className="flex-1 flex items-center justify-center px-6">
          <div className="text-center">
            <p className="text-sm text-chalk-muted/60 mb-1">
              {teacherName
                ? `Hey ${teacherName}, what are we working on?`
                : "Ask Chalk about your lesson plans"}
            </p>
            <p className="text-[11px] text-chalk-muted/30">
              Chalk searches your teaching history to give context-aware suggestions
            </p>
          </div>
        </div>
      )}

      {/* Messages */}
      {displayMessages.length > 0 && (
        <div ref={scrollRef} className="flex-1 overflow-y-auto px-4 py-3">
          {displayMessages.map((msg) => (
            <MessageBubble key={msg.id} message={msg} />
          ))}
          {loading && <TypingIndicator />}
        </div>
      )}

      {/* Spacer when empty + loading */}
      {displayMessages.length === 0 && loading && (
        <div ref={scrollRef} className="flex-1 overflow-y-auto px-4 py-3">
          <TypingIndicator />
        </div>
      )}

      {/* Input area */}
      <div className="px-3 py-2.5 border-t border-chalk-white/8">
        <div className="flex items-end gap-2 bg-chalk-board-dark/60 border border-chalk-white/8 rounded-xl px-3 py-2 focus-within:border-chalk-blue/30 transition-colors">
          <textarea
            ref={inputRef}
            value={input}
            onChange={handleInputChange}
            onKeyDown={handleKeyDown}
            placeholder="Type a message..."
            rows={1}
            className="flex-1 bg-transparent text-sm text-chalk-white placeholder-chalk-muted resize-none focus:outline-none max-h-[120px]"
          />
          <button
            onClick={handleSendClick}
            disabled={!input.trim() || loading}
            className={`p-1.5 rounded-lg transition-colors flex-shrink-0 ${
              input.trim() && !loading
                ? "text-chalk-blue hover:bg-chalk-blue/15"
                : "text-chalk-muted/30 cursor-not-allowed"
            }`}
            title="Send message"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M12 19V5m0 0l-7 7m7-7l7 7"
              />
            </svg>
          </button>
        </div>
        <p className="text-[10px] text-chalk-muted/40 mt-1.5 px-1">
          Shift+Enter for new line
        </p>
      </div>
    </div>
  );
}
