/**
 * Session persistence — auto-save, load, and delete chat sessions.
 *
 * Extracted from App.tsx to keep the main component focused on layout
 * and Director-loop orchestration.
 */

import { useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ChatMessage, ToolLog, BlackboardEvent, SessionMeta, Session } from '../types';
import { makeSessionId, syncSessionIdentity } from '../utils';

export interface SessionManagerDeps {
  workspace: string | null;
  currentSessionId: string;
  messages: ChatMessage[];
  sessions: SessionMeta[];

  // Refs (stable across renders)
  messagesRef: React.MutableRefObject<ChatMessage[]>;
  toolLogsRef: React.MutableRefObject<ToolLog[]>;
  blackboardEventsRef: React.MutableRefObject<BlackboardEvent[]>;
  projectContextRef: React.MutableRefObject<string | null>;
  projectContextMetaRef: React.MutableRefObject<{ source: 'auto' | 'manual' | null; workspace: string | null }>;
  sessionIdRef: React.MutableRefObject<string>;
  planReportRef: React.MutableRefObject<string>;

  // State setters
  setMessages: React.Dispatch<React.SetStateAction<ChatMessage[]>>;
  setToolLogs: React.Dispatch<React.SetStateAction<ToolLog[]>>;
  setBlackboardEvents: React.Dispatch<React.SetStateAction<BlackboardEvent[]>>;
  setCurrentSessionId: React.Dispatch<React.SetStateAction<string>>;
  setSessions: React.Dispatch<React.SetStateAction<SessionMeta[]>>;
  setWorkspace: React.Dispatch<React.SetStateAction<string | null>>;
  setProjectContext: React.Dispatch<React.SetStateAction<string | null>>;
}

export interface SessionManagerActions {
  persistSessionNow: () => Promise<void>;
  flushPendingSessionSave: () => Promise<void>;
  handleLoadSession: (sessionId: string) => Promise<void>;
  handleDeleteSession: (sessionId: string) => Promise<void>;
}

export function useSessionManager(deps: SessionManagerDeps): SessionManagerActions {
  const {
    workspace, currentSessionId, messages, sessions,
    messagesRef, toolLogsRef, blackboardEventsRef, projectContextRef,
    projectContextMetaRef, sessionIdRef, planReportRef,
    setMessages, setToolLogs, setBlackboardEvents, setCurrentSessionId,
    setSessions, setWorkspace, setProjectContext,
  } = deps;

  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Keep stable refs so callbacks don't capture stale values.
  const workspaceRef = useRef(workspace);
  workspaceRef.current = workspace;
  const sessionsRef = useRef(sessions);
  sessionsRef.current = sessions;

  const persistSessionNow = useCallback(async () => {
    if (messagesRef.current.length === 0) return;

    const title = messagesRef.current.find(m => m.role === 'user')?.content.slice(0, 60) ?? '新对话';
    const ws = workspaceRef.current;
    // Use sessionsRef to avoid stale closure across the 1.5s debounce window.
    const existingMeta = sessionsRef.current.find(session => session.id === sessionIdRef.current);
    const latestMessageAt = messagesRef.current.reduce((latest, message) => Math.max(latest, message.timestamp), 0);
    const latestToolLogAt = toolLogsRef.current.reduce((latest, log) => Math.max(latest, log.timestamp), 0);
    const updatedAt = Math.max(existingMeta?.updated_at ?? 0, latestMessageAt, latestToolLogAt);
    const directorHistory = await invoke<unknown[]>('get_director_history');

    await invoke('save_session', {
      workspace: ws,
      session: {
        id: sessionIdRef.current,
        title,
        workspace_path: ws,
        created_at: messagesRef.current[0].timestamp,
        updated_at: updatedAt,
        message_count: messagesRef.current.length,
        messages: messagesRef.current,
        tool_logs: toolLogsRef.current,
        blackboard_events: blackboardEventsRef.current,
        project_context: projectContextRef.current,
        project_context_source: projectContextMetaRef.current.source,
        director_history: directorHistory,
      },
    });

    const list = await invoke<SessionMeta[]>('list_sessions', { workspace: ws });
    setSessions(list);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const flushPendingSessionSave = useCallback(async () => {
    if (saveTimerRef.current) {
      clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
    }
    try {
      await persistSessionNow();
    } catch (err) {
      console.error('auto-save error:', err);
    }
  }, [persistSessionNow]);

  // Debounced auto-save: fires 1.5s after any messages/toolLogs/blackboard changes.
  useEffect(() => {
    if (messages.length === 0) return;
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    saveTimerRef.current = setTimeout(async () => {
      try {
        await persistSessionNow();
      } catch (err) {
        console.error('auto-save error:', err);
      }
    }, 1500);
    return () => { if (saveTimerRef.current) clearTimeout(saveTimerRef.current); };
  }, [messages, persistSessionNow]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleLoadSession = useCallback(async (sessionId: string) => {
    try {
      if (sessionId !== currentSessionId) {
        await flushPendingSessionSave();
      }
      const s = await invoke<Session>('load_session', { workspace: workspaceRef.current, sessionId });
      const restoredWorkspace = s.workspace_path ?? workspace;
      setMessages(s.messages);
      setToolLogs(s.tool_logs as ToolLog[]);
      setBlackboardEvents(s.blackboard_events || []);
      syncSessionIdentity(s.id, sessionIdRef, setCurrentSessionId);
      projectContextRef.current = s.project_context ?? null;
      setProjectContext(s.project_context ?? null);
      projectContextMetaRef.current = s.project_context
        ? {
          source: s.project_context_source === 'auto' || s.project_context_source === 'manual'
            ? s.project_context_source
            : 'manual',
          workspace: restoredWorkspace,
        }
        : { source: null, workspace: null };
      setWorkspace(restoredWorkspace);
      await invoke('restore_director_history', { history: s.director_history ?? [] });
    } catch (err) {
      console.error('load_session error:', err);
    }
  }, [workspace, currentSessionId, flushPendingSessionSave]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleDeleteSession = useCallback(async (sessionId: string) => {
    try {
      // Use workspaceRef to avoid stale closure after workspace switch.
      await invoke('delete_session', { workspace: workspaceRef.current, sessionId });
      setSessions(prev => prev.filter(s => s.id !== sessionId));
      if (sessionId === currentSessionId) {
        setMessages([]);
        setToolLogs([]);
        setBlackboardEvents([]);
        planReportRef.current = '';
        syncSessionIdentity(makeSessionId(), sessionIdRef, setCurrentSessionId);
        await invoke('clear_history');
      }
    } catch (err) {
      console.error('delete_session error:', err);
    }
  }, [currentSessionId]); // eslint-disable-line react-hooks/exhaustive-deps

  return { persistSessionNow, flushPendingSessionSave, handleLoadSession, handleDeleteSession };
}
