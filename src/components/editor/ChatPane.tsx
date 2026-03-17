import { useState, useRef, useEffect } from "react";
import { motion } from "framer-motion";

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  timestamp: Date;
}

interface ChatPaneProps {
  messages: ChatMessage[];
  onSendMessage: (message: string) => void;
  isLoading?: boolean;
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

export function ChatPane({ messages, onSendMessage, isLoading }: ChatPaneProps) {
  const [input, setInput] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, isLoading]);

  const handleSend = () => {
    const trimmed = input.trim();
    if (!trimmed || isLoading) return;
    onSendMessage(trimmed);
    setInput("");
    if (inputRef.current) {
      inputRef.current.style.height = "auto";
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
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
      <div className="flex items-center gap-2 px-4 py-2.5 border-b border-chalk-white/8">
        <div className="w-2 h-2 rounded-full bg-chalk-green animate-pulse" />
        <span className="text-xs font-medium text-chalk-muted">Chalk AI</span>
      </div>

      {/* Messages */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-4 py-3">
        {messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}
        {isLoading && <TypingIndicator />}
      </div>

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
            onClick={handleSend}
            disabled={!input.trim() || isLoading}
            className={`p-1.5 rounded-lg transition-colors flex-shrink-0 ${
              input.trim() && !isLoading
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
