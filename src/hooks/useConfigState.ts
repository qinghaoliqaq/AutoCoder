/**
 * useConfigState — configuration detection, editor modal, and persistence.
 */

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ConfigStatus, ConfigDraft } from '../types';

export function useConfigState() {
  const [configStatus, setConfigStatus] = useState<ConfigStatus | null>(null);
  const [showConfigEditor, setShowConfigEditor] = useState(false);
  const [configDraft, setConfigDraft] = useState<ConfigDraft | null>(null);
  const [configSaving, setConfigSaving] = useState(false);
  const [configError, setConfigError] = useState<string | null>(null);
  const [configUpdating, setConfigUpdating] = useState(false);

  const runDetection = useCallback(async () => {
    try {
      const cfgStatus = await invoke<ConfigStatus>('get_config');
      setConfigStatus(cfgStatus);
    } catch (err) {
      console.error('Init error:', err);
      setConfigStatus({
        configured: false,
        base_url: '',
        model: '',
        api_format: 'openai',
        api_key_hint: '',
        vendored_skills: true,
        max_parallel_subtasks: 5,
        execution_access_mode: 'sandbox',
        director_provider: 'openai',
        agent_provider: '',
        agent_second_provider: '',
      });
    }
  }, []);

  useEffect(() => { runDetection(); }, [runDetection]);

  const handleOpenConfigEditor = useCallback(async () => {
    setConfigError(null);
    setConfigDraft(null);
    setShowConfigEditor(true);
    try {
      const draft = await invoke<ConfigDraft>('get_config_form');
      setConfigDraft(draft);
    } catch (err) {
      setConfigDraft(null);
      setConfigError(String(err));
    }
  }, []);

  const handleSaveConfig = useCallback(async () => {
    if (!configDraft || configSaving) return;
    setConfigSaving(true);
    setConfigError(null);
    try {
      const status = await invoke<ConfigStatus>('save_config', { config: configDraft });
      setConfigStatus(status);
      setShowConfigEditor(false);
    } catch (err) {
      setConfigError(String(err));
    } finally {
      setConfigSaving(false);
    }
  }, [configDraft, configSaving]);

  const handleToggleExecutionAccess = useCallback(async (mode: ConfigStatus['execution_access_mode']) => {
    if (configUpdating) return;
    setConfigUpdating(true);
    setConfigError(null);
    try {
      const status = await invoke<ConfigStatus>('set_execution_access_mode', { mode });
      setConfigStatus(status);
      setConfigDraft(prev => (prev ? { ...prev, execution_access_mode: mode } : prev));
    } catch (err) {
      setConfigError(String(err));
    } finally {
      setConfigUpdating(false);
    }
  }, [configUpdating]);

  const closeConfigEditor = useCallback(() => {
    if (configSaving) return;
    setShowConfigEditor(false);
    setConfigError(null);
  }, [configSaving]);

  return {
    configStatus,
    showConfigEditor, configDraft, configSaving, configError, configUpdating,
    setConfigDraft,
    handleOpenConfigEditor, handleSaveConfig, handleToggleExecutionAccess, closeConfigEditor,
  };
}
