import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

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

interface SendMessageResponse {
  conversation_id: string;
  user_message: ChatMessage;
  assistant_message: ChatMessage;
  context_plans: RetrievedContext[];
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
 * Handles conversation lifecycle, message sending, and state management.
 */
export function useChat(planId?: string) {
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastContextPlans, setLastContextPlans] = useState<RetrievedContext[]>(
    [],
  );

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

  const sendMessage = useCallback(
    async (message: string) => {
      if (!message.trim() || isLoading) return;

      setIsLoading(true);
      setError(null);

      try {
        const response = await invoke<SendMessageResponse>(
          "send_chat_message",
          {
            input: {
              conversation_id: conversationId,
              message: message.trim(),
              plan_id: planId ?? null,
            },
          },
        );

        // Update conversation ID if new.
        if (!conversationId) {
          setConversationId(response.conversation_id);
        }

        // Append both messages.
        setMessages((prev) => [
          ...prev,
          response.user_message,
          response.assistant_message,
        ]);

        // Track which plans were used as context.
        setLastContextPlans(response.context_plans);
      } catch (e) {
        setError(`${e}`);
      } finally {
        setIsLoading(false);
      }
    },
    [conversationId, planId, isLoading],
  );

  const loadConversation = useCallback(async (id: string) => {
    setConversationId(id);
    setError(null);
  }, []);

  const startNewConversation = useCallback(() => {
    setConversationId(null);
    setMessages([]);
    setError(null);
    setLastContextPlans([]);
  }, []);

  return {
    conversationId,
    messages,
    isLoading,
    error,
    lastContextPlans,
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
