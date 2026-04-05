import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { FileNode } from '../types';
import {
  SiTypescript, SiJavascript, SiReact, SiPython, SiRust, SiGo, SiOpenjdk,
  SiMarkdown, SiJson, SiHtml5, SiCss3, SiGnubash, SiDotenv, SiToml
} from 'react-icons/si';
import { VscFile, VscFolder, VscFolderOpened, VscChevronRight, VscChevronDown, VscFileMedia, VscDatabase, VscFilePdf, VscRefresh, VscLayoutSidebarLeftOff, VscFolderActive } from 'react-icons/vsc';

// ── File icon ─────────────────────────────────────────────────────────────────

function fileIcon(name: string): React.ReactNode {
  const ext = name.split('.').pop()?.toLowerCase() ?? '';
  const iconProps = { className: 'w-3.5 h-3.5 transition-transform group-hover:scale-110' };

  if (name.endsWith('.config.js') || name.endsWith('.config.ts')) return <SiJavascript className={`${iconProps.className} text-yellow-400`} />;
  if (name.endsWith('.d.ts')) return <SiTypescript className={`${iconProps.className} text-blue-400`} />;

  switch (ext) {
    case 'ts': return <SiTypescript className={`${iconProps.className} text-blue-500`} />;
    case 'tsx': return <SiReact className={`${iconProps.className} text-cyan-400`} />;
    case 'js': return <SiJavascript className={`${iconProps.className} text-yellow-400`} />;
    case 'jsx': return <SiReact className={`${iconProps.className} text-cyan-400`} />;
    case 'py': return <SiPython className={`${iconProps.className} text-blue-400`} />;
    case 'rs': return <SiRust className={`${iconProps.className} text-orange-500`} />;
    case 'go': return <SiGo className={`${iconProps.className} text-cyan-500`} />;
    case 'java': return <SiOpenjdk className={`${iconProps.className} text-orange-600`} />;
    case 'md': return <SiMarkdown className={`${iconProps.className} text-sky-400`} />;
    case 'json': return <SiJson className={`${iconProps.className} text-yellow-500`} />;
    case 'toml': return <SiToml className={`${iconProps.className} text-zinc-500`} />;
    case 'html': return <SiHtml5 className={`${iconProps.className} text-orange-500`} />;
    case 'css': return <SiCss3 className={`${iconProps.className} text-blue-400`} />;
    case 'sh': return <SiGnubash className={`${iconProps.className} text-zinc-600 dark:text-zinc-300`} />;
    case 'env': return <SiDotenv className={`${iconProps.className} text-yellow-300`} />;
    case 'png': case 'jpg': case 'jpeg': case 'gif': case 'svg': case 'webp':
      return <VscFileMedia className={`${iconProps.className} text-purple-400`} />;
    case 'pdf': return <VscFilePdf className={`${iconProps.className} text-red-500`} />;
    case 'sql': case 'db': return <VscDatabase className={`${iconProps.className} text-rose-400`} />;
    default: return <VscFile className={`${iconProps.className} text-zinc-400`} />;
  }
}

// ── TreeNode ──────────────────────────────────────────────────────────────────

function TreeNode({ node, depth }: { node: FileNode; depth: number }) {
  const [open, setOpen] = useState(depth < 2);

  const indent = depth * 14;

  if (node.is_dir) {
    return (
      <div>
        <button
          onClick={() => setOpen((v) => !v)}
          className="flex items-center gap-1.5 w-full text-left py-[3px] px-2 rounded-md outline-none
                     hover:bg-zinc-100/80 dark:hover:bg-zinc-800/50 transition-all duration-150 group"
          style={{ paddingLeft: `${indent + 6}px` }}
        >
          <span className="text-zinc-400 dark:text-zinc-500 w-3 flex-shrink-0 flex items-center justify-center transition-all group-hover:text-zinc-600 dark:group-hover:text-zinc-300">
            {open ? <VscChevronDown className="w-3 h-3" /> : <VscChevronRight className="w-3 h-3" />}
          </span>
          <span className="text-amber-500 dark:text-amber-400/80 transition-transform duration-200 group-hover:scale-110 flex items-center justify-center">
            {open ? <VscFolderOpened className="w-[15px] h-[15px]" /> : <VscFolder className="w-[15px] h-[15px]" />}
          </span>
          <span className="text-[12px] font-medium text-zinc-700 dark:text-zinc-300 truncate group-hover:text-zinc-900 dark:group-hover:text-zinc-100 transition-colors">
            {node.name}
          </span>
        </button>
        {open && node.children.map((child) => (
          <TreeNode key={child.path} node={child} depth={depth + 1} />
        ))}
      </div>
    );
  }

  return (
    <div
      className="flex items-center gap-1.5 py-[3px] px-2 rounded-md group
                 hover:bg-zinc-100/80 dark:hover:bg-zinc-800/50 transition-all duration-150 cursor-default"
      style={{ paddingLeft: `${indent + 6 + 18}px` }}
      title={node.path}
    >
      <span className="flex items-center justify-center">{fileIcon(node.name)}</span>
      <span className="text-[12px] text-zinc-600 dark:text-zinc-400 truncate group-hover:text-zinc-900 dark:group-hover:text-zinc-100 transition-colors">{node.name}</span>
    </div>
  );
}

// ── FileTreePanel ─────────────────────────────────────────────────────────────

interface FileTreePanelProps {
  workspacePath: string | null;
  onOpenProject: () => void;
  onClose: () => void;
}

export default function FileTreePanel({ workspacePath, onOpenProject, onClose }: FileTreePanelProps) {
  const [tree, setTree] = useState<FileNode[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!workspacePath) return;
    setLoading(true);
    setError(null);
    try {
      const nodes = await invoke<FileNode[]>('workspace_tree', { path: workspacePath });
      setTree(nodes);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [workspacePath]);

  useEffect(() => {
    setTree([]);
    refresh();
  }, [refresh]);

  const displayName = workspacePath ? workspacePath.split('/').pop() ?? workspacePath : null;

  return (
    <div
      className="flex flex-col h-full w-full"
    >
      <div className="w-full h-full flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 flex-shrink-0
                        border-b border-zinc-200/40 dark:border-zinc-700/40 min-h-[52px]">
          <span className="text-[11px] font-bold uppercase tracking-widest text-zinc-800 dark:text-zinc-200 select-none">
            EXPLORER
          </span>
          <div className="flex items-center gap-1.5 flex-shrink-0">
            {workspacePath && (
              <button
                onClick={refresh}
                disabled={loading}
                className="p-1.5 rounded-md text-zinc-400 hover:text-zinc-700 dark:text-zinc-500 dark:hover:text-zinc-200
                           hover:bg-zinc-200/50 dark:hover:bg-zinc-700/50 transition-colors disabled:opacity-40"
                title="刷新文件树"
              >
                <VscRefresh className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
              </button>
            )}
            <button
              onClick={onOpenProject}
              className="p-1.5 rounded-md text-zinc-400 hover:text-violet-600 dark:text-zinc-500 dark:hover:text-violet-400
                         hover:bg-violet-100 dark:hover:bg-violet-500/20 transition-colors"
              title="切换工作区"
            >
              <VscFolderActive className="w-3.5 h-3.5" />
            </button>
            <button
              onClick={onClose}
              className="p-1.5 rounded-md text-zinc-400 hover:text-rose-600 dark:text-zinc-500 dark:hover:text-rose-400
                         hover:bg-rose-100 dark:hover:bg-rose-500/20 transition-colors"
              title="收起"
            >
              <VscLayoutSidebarLeftOff className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {/* Project name row */}
        {displayName && (
          <div className="flex items-center gap-2 px-3 py-1.5 border-b border-zinc-200 dark:border-zinc-800
                        bg-zinc-100/60 dark:bg-zinc-800/40">
            <VscFolder className="w-3.5 h-3.5 text-amber-500 flex-shrink-0" />
            <span className="text-[11px] font-semibold text-zinc-600 dark:text-zinc-300 truncate uppercase tracking-wide"
              title={workspacePath ?? ''}>
              {displayName}
            </span>
          </div>
        )}

        {/* Tree / empty state */}
        <div className="flex-1 overflow-y-auto py-1.5 custom-scrollbar">
          {!workspacePath ? (
            <div className="flex flex-col items-center justify-center h-full px-4 py-10 text-center gap-3">
              <div className="w-10 h-10 rounded-xl bg-zinc-100 dark:bg-zinc-800 flex items-center justify-center
                            border border-zinc-200 dark:border-zinc-700">
                <VscFolder className="w-5 h-5 text-amber-400" />
              </div>
              <p className="text-xs text-zinc-400 dark:text-zinc-500 leading-relaxed">No folder open</p>
              <button
                onClick={onOpenProject}
                className="text-xs px-3 py-1.5 rounded-lg bg-violet-600 hover:bg-violet-700
                         text-white transition-colors font-medium"
              >
                Open Project
              </button>
            </div>
          ) : (
            <>
              {error && <p className="px-3 py-2 text-xs text-red-500">{error}</p>}
              {!error && tree.length === 0 && !loading && (
                <p className="px-3 py-4 text-xs text-zinc-400 dark:text-zinc-500 text-center">目录为空</p>
              )}
              {tree.map((node) => (
                <TreeNode key={node.path} node={node} depth={0} />
              ))}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
