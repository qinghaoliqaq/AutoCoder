/**
 * useSidebarState — sidebar tab switching, blackboard fullscreen toggle,
 * and unread-message badge calculation.
 */

import { useState, useCallback } from 'react';
import type { ChatMessage } from '../types';

export type SidebarTab = 'explorer' | 'logs' | 'history' | 'blackboard';

export function useSidebarState(messages: ChatMessage[]) {
  const [activeSidebarTab, setActiveSidebarTab] = useState<SidebarTab | null>(null);
  const [blackboardFullscreen, setBlackboardFullscreen] = useState(false);
  const [previousSidebarTab, setPreviousSidebarTab] = useState<Exclude<SidebarTab, 'blackboard'> | null>(null);
  const [blackboardSeenMessageAt, setBlackboardSeenMessageAt] = useState(0);

  const latestAgentMessageAt = messages.reduce((latest, message) => {
    if (message.role === 'user') return latest;
    return Math.max(latest, message.timestamp);
  }, 0);

  const unreadAgentMessages = blackboardFullscreen
    ? messages.filter(message => message.role !== 'user' && message.timestamp > blackboardSeenMessageAt).length
    : 0;

  const closeBlackboardWorkspace = useCallback(() => {
    setBlackboardSeenMessageAt(latestAgentMessageAt);
    setBlackboardFullscreen(false);
    setActiveSidebarTab(previousSidebarTab);
  }, [latestAgentMessageAt, previousSidebarTab]);

  const openBlackboardWorkspace = useCallback(() => {
    setPreviousSidebarTab(activeSidebarTab && activeSidebarTab !== 'blackboard' ? activeSidebarTab : null);
    setBlackboardSeenMessageAt(latestAgentMessageAt);
    setBlackboardFullscreen(true);
    setActiveSidebarTab('blackboard');
  }, [activeSidebarTab, latestAgentMessageAt]);

  const toggleSidebarTab = useCallback((tab: Exclude<SidebarTab, 'blackboard'>) => {
    setBlackboardFullscreen(false);
    setActiveSidebarTab(prev => prev === tab ? null : tab);
  }, []);

  const toggleBlackboardWorkspace = useCallback(() => {
    if (blackboardFullscreen && activeSidebarTab === 'blackboard') {
      closeBlackboardWorkspace();
      return;
    }
    openBlackboardWorkspace();
  }, [activeSidebarTab, blackboardFullscreen, closeBlackboardWorkspace, openBlackboardWorkspace]);

  const sidebarWidth = activeSidebarTab === null || blackboardFullscreen
    ? '0px'
    : activeSidebarTab === 'blackboard'
      ? 'min(62vw, 680px)'
      : '280px';

  return {
    activeSidebarTab, setActiveSidebarTab,
    blackboardFullscreen,
    unreadAgentMessages,
    sidebarWidth,
    toggleSidebarTab, toggleBlackboardWorkspace, closeBlackboardWorkspace,
  };
}
