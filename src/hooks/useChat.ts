import { useState, useEffect, useCallback, useRef, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ── Types ───────────────────────────────────────────────────

export interface ChatConversation {
  id: string;
  title: string;
  plan_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface ChatMessage {
  id: string;
  conversation_id: string;
  role: "user" | "assistant" | "system";
  content: string;
  context_plan_ids: string | null;
  created_at: string;
}

interface RetrievedContext {
  plan_id: string;
  title: string;
  content: string;
  learning_objectives: string | null;
  distance: number;
}

interface StreamStartResponse {
  conversation_id: string;
  user_message: ChatMessage;
  context_plans: RetrievedContext[];
}

interface StreamTokenPayload {
  conversation_id: string;
  token: string;
}

interface StreamDonePayload {
  conversation_id: string;
  message_id: string;
  full_content: string;
  context_plan_ids: string | null;
}

interface StreamErrorPayload {
  conversation_id: string;
  error: string;
}

export interface AiConfig {
  has_api_key: boolean;
  base_url: string;
  model: string;
}

// ── Hook ────────────────────────────────────────────────────

/**
 * Hook for managing AI chat conversations with RAG-enhanced context.
 *
 * Supports streaming responses via Tauri events for real-time token display.
 */
export function useChat(
  planId?: string,
  planTitle?: string,
  planContentRef?: MutableRefObject<string>,
) {
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastContextPlans, setLastContextPlans] = useState<RetrievedContext[]>(
    [],
  );
  const [streamingContent, setStreamingContent] = useState<string | null>(null);

  // Track the conversation ID for stream event filtering.
  const activeConvRef = useRef<string | null>(null);

  // Load messages when conversation changes.
  useEffect(() => {
    if (!conversationId) {
      setMessages([]);
      return;
    }

    invoke<ChatMessage[]>("get_chat_messages_cmd", {
      conversationId,
    })
      .then(setMessages)
      .catch((e) => setError(`Failed to load messages: ${e}`));
  }, [conversationId]);

  // Subscribe to streaming events.
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    const setup = async () => {
      const unlisten1 = await listen<StreamTokenPayload>(
        "chat:stream_token",
        (event) => {
          if (
            activeConvRef.current &&
            event.payload.conversation_id === activeConvRef.current
          ) {
            setStreamingContent((prev) => (prev ?? "") + event.payload.token);
          }
        },
      );
      unlisteners.push(unlisten1);

      const unlisten2 = await listen<StreamDonePayload>(
        "chat:stream_done",
        (event) => {
          if (
            activeConvRef.current &&
            event.payload.conversation_id === activeConvRef.current
          ) {
            // Replace streaming content with the finalized assistant message.
            const assistantMsg: ChatMessage = {
              id: event.payload.message_id,
              conversation_id: event.payload.conversation_id,
              role: "assistant",
              content: event.payload.full_content,
              context_plan_ids: event.payload.context_plan_ids,
              created_at: new Date().toISOString(),
            };
            setMessages((prev) => {
              // Deduplicate: the useEffect that fetches messages on conversationId
              // change may have already loaded this message from the backend.
              if (prev.some((m) => m.id === assistantMsg.id)) return prev;
              return [...prev, assistantMsg];
            });
            setStreamingContent(null);
            setIsLoading(false);
          }
        },
      );
      unlisteners.push(unlisten2);

      const unlisten3 = await listen<StreamErrorPayload>(
        "chat:stream_error",
        (event) => {
          if (
            activeConvRef.current &&
            event.payload.conversation_id === activeConvRef.current
          ) {
            setError(event.payload.error);
            setStreamingContent(null);
            setIsLoading(false);
          }
        },
      );
      unlisteners.push(unlisten3);
    };

    setup();

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  const sendMessage = useCallback(
    async (message: string) => {
      if (!message.trim() || isLoading) return;

      setIsLoading(true);
      setError(null);
      setStreamingContent(null);

      try {
        const response = await invoke<StreamStartResponse>(
          "send_chat_message_stream",
          {
            input: {
              conversation_id: conversationId,
              message: message.trim(),
              plan_id: planId ?? null,
              plan_title: planTitle ?? null,
              plan_content: planContentRef?.current ?? null,
            },
          },
        );

        // Update conversation ID if new.
        if (!conversationId) {
          setConversationId(response.conversation_id);
        }

        // Track active conversation for stream filtering.
        activeConvRef.current = response.conversation_id;

        // Append user message immediately.
        setMessages((prev) => [...prev, response.user_message]);

        // Track which plans were used as context.
        setLastContextPlans(response.context_plans);

        // Streaming content will be populated by event listeners.
        setStreamingContent("");
      } catch (e) {
        setError(`${e}`);
        setIsLoading(false);
      }
    },
    [conversationId, planId, isLoading],
  );

  const loadConversation = useCallback(async (id: string) => {
    setConversationId(id);
    setError(null);
    setStreamingContent(null);
  }, []);

  const startNewConversation = useCallback(() => {
    setConversationId(null);
    setMessages([]);
    setError(null);
    setLastContextPlans([]);
    setStreamingContent(null);
    activeConvRef.current = null;
  }, []);

  return {
    conversationId,
    messages,
    isLoading,
    error,
    lastContextPlans,
    streamingContent,
    sendMessage,
    loadConversation,
    startNewConversation,
  };
}

// ── Conversations List Hook ─────────────────────────────────

/**
 * Hook for listing and managing chat conversations.
 */
export function useConversations() {
  const [conversations, setConversations] = useState<ChatConversation[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const result =
        await invoke<ChatConversation[]>("list_conversations");
      setConversations(result);
      setError(null);
    } catch (e) {
      setError(`Failed to load conversations: ${e}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const deleteConversation = useCallback(
    async (id: string) => {
      try {
        await invoke("delete_conversation", { conversationId: id });
        setConversations((prev) => prev.filter((c) => c.id !== id));
      } catch (e) {
        setError(`Failed to delete conversation: ${e}`);
      }
    },
    [],
  );

  return { conversations, loading, error, refresh, deleteConversation };
}

// ── AI Config Hook ──────────────────────────────────────────

/**
 * Hook for managing AI configuration (API key, model, etc.).
 */
export function useAiConfig() {
  const [config, setConfig] = useState<AiConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const result = await invoke<AiConfig>("get_ai_config");
      setConfig(result);
      setError(null);
    } catch (e) {
      setError(`Failed to load AI config: ${e}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const saveConfig = useCallback(
    async (updates: {
      api_key?: string;
      base_url?: string;
      model?: string;
    }) => {
      try {
        await invoke("save_ai_config", {
          apiKey: updates.api_key ?? null,
          baseUrl: updates.base_url ?? null,
          model: updates.model ?? null,
        });
        await refresh();
      } catch (e) {
        setError(`Failed to save AI config: ${e}`);
        throw e;
      }
    },
    [refresh],
  );

  return { config, loading, error, saveConfig, refresh };
}

// ── Vectorize Hook ──────────────────────────────────────────

/**
 * Hook for vectorizing lesson plans (generating embeddings).
 */
export function useVectorize() {
  const [isVectorizing, setIsVectorizing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const vectorizePlan = useCallback(async (planId: string) => {
    setIsVectorizing(true);
    setError(null);
    try {
      await invoke("vectorize_plan", { planId });
    } catch (e) {
      setError(`Failed to vectorize plan: ${e}`);
      throw e;
    } finally {
      setIsVectorizing(false);
    }
  }, []);

  const vectorizeAll = useCallback(async (): Promise<number> => {
    setIsVectorizing(true);
    setError(null);
    try {
      const count = await invoke<number>("vectorize_all_plans");
      return count;
    } catch (e) {
      setError(`Failed to vectorize plans: ${e}`);
      throw e;
    } finally {
      setIsVectorizing(false);
    }
  }, []);

  return { isVectorizing, error, vectorizePlan, vectorizeAll };
}
