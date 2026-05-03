/**
 * useUserQuestion — connects the AskUserQuestion backend tool to the UI.
 *
 * Backend lifecycle (see src-tauri/src/tools/ask_user_question/):
 *   1. Tool runs   → emits `user-question-pending` with the question payload
 *                    and registers a oneshot sender keyed by `request_id`.
 *   2. User replies → frontend invokes `submit_user_answer` with the same id
 *                    and the chosen text.
 *   3. Backend resolves the oneshot, the agent loop resumes with the reply.
 *   4. Backend timeouts / cancellations → emits `user-question-cancelled`
 *                    so the UI can clear the prompt without leaving it
 *                    stuck waiting for a reply that will never arrive.
 *
 * The reducer is exported separately so it can be unit-tested without
 * standing up a Tauri runtime — all the interesting state transitions
 * (pending overrides, mismatched cancellation, double-submit) live there.
 */

import { useCallback, useEffect, useReducer, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';

export interface PendingQuestion {
  request_id: string;
  agent_id: string;
  question: string;
  options: string[];
}

export interface UserQuestionState {
  active: PendingQuestion | null;
}

export type UserQuestionEvent =
  | { type: 'pending'; payload: PendingQuestion }
  | { type: 'cancelled'; request_id: string }
  | { type: 'submitted'; request_id: string };

/**
 * Pure state transitions for user-question lifecycle events.
 *
 *   * `pending` — replace any in-flight question. The backend always
 *     keys by `request_id`, so a fresh pending implies the previous
 *     one was already resolved or abandoned at the backend layer.
 *   * `cancelled` / `submitted` — clear only when the id matches.
 *     Late events for a since-replaced question must not blank a new
 *     active prompt.
 */
export function userQuestionReducer(
  state: UserQuestionState,
  event: UserQuestionEvent,
): UserQuestionState {
  switch (event.type) {
    case 'pending':
      return { active: event.payload };
    case 'cancelled':
    case 'submitted':
      if (state.active?.request_id === event.request_id) {
        return { active: null };
      }
      return state;
  }
}

const INITIAL_STATE: UserQuestionState = { active: null };

export interface UseUserQuestionResult {
  activeQuestion: PendingQuestion | null;
  /**
   * Send the user's reply to the backend. Clears the UI on success or
   * failure — a backend rejection (request id already resolved, registry
   * missing) means there's nothing the user can do from here, so showing
   * a stuck prompt would be worse than dismissing it.
   */
  submitAnswer: (answer: string) => Promise<void>;
}

export function useUserQuestion(): UseUserQuestionResult {
  const [state, dispatch] = useReducer(userQuestionReducer, INITIAL_STATE);
  // Tracks whether the hook's owning component is still mounted. The
  // submitAnswer closure can be invoked from a button click whose
  // backend round-trip outlives the modal — the post-invoke dispatch
  // would then run against an unmounted component and trigger React's
  // "state update on unmounted" warning. The flag is also reused by
  // the event-listener subscription cleanup.
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    let unlistenPending: (() => void) | null = null;
    let unlistenCancelled: (() => void) | null = null;

    (async () => {
      const w = getCurrentWebviewWindow();
      // Subscribe in parallel — neither subscribe call depends on the
      // other and `await Promise.all` halves the time-to-first-event.
      const [u1, u2] = await Promise.all([
        w.listen<PendingQuestion>('user-question-pending', (e) => {
          if (mountedRef.current) dispatch({ type: 'pending', payload: e.payload });
        }),
        w.listen<{ request_id: string }>('user-question-cancelled', (e) => {
          if (mountedRef.current)
            dispatch({ type: 'cancelled', request_id: e.payload.request_id });
        }),
      ]);
      // If we unmounted while subscribing, drop the listeners now.
      if (!mountedRef.current) {
        u1();
        u2();
        return;
      }
      unlistenPending = u1;
      unlistenCancelled = u2;
    })();

    return () => {
      mountedRef.current = false;
      unlistenPending?.();
      unlistenCancelled?.();
    };
  }, []);

  const submitAnswer = useCallback(
    async (answer: string) => {
      const active = state.active;
      if (!active) return;
      try {
        await invoke('submit_user_answer', {
          payload: { request_id: active.request_id, answer },
        });
      } catch (err) {
        // Backend lookup miss — typically means the agent already gave
        // up (timeout / cancel) before the user could reply. Surface to
        // the console for debugging but don't block the UI from
        // recovering.
        // eslint-disable-next-line no-console
        console.warn('submit_user_answer failed:', err);
      } finally {
        // Skip the post-invoke dispatch if the component unmounted
        // mid-flight — otherwise React logs a "state update on
        // unmounted" warning in dev, and the dispatched action would
        // hit a stale reducer instance with no observable effect.
        if (mountedRef.current) {
          dispatch({ type: 'submitted', request_id: active.request_id });
        }
      }
    },
    [state.active],
  );

  return { activeQuestion: state.active, submitAnswer };
}
