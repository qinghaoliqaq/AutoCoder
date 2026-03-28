export function relativeTime(ts: number): string {
  const diff = Date.now() - ts;
  const m = Math.floor(diff / 60000);
  const h = Math.floor(diff / 3600000);
  const d = Math.floor(diff / 86400000);
  if (m < 1)  return '刚刚';
  if (m < 60) return `${m}分钟前`;
  if (h < 24) return `${h}小时前`;
  if (d < 7)  return `${d}天前`;
  return new Date(ts).toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
}

export function getGroup(ts: number): string {
  const now = new Date();
  const date = new Date(ts);
  const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const diff = todayStart - new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const days = Math.round(diff / 86400000);
  if (days === 0) return '今天';
  if (days === 1) return '昨天';
  if (days <= 7)  return '最近 7 天';
  if (days <= 30) return '最近 30 天';
  return date.toLocaleDateString('zh-CN', { year: 'numeric', month: 'long' });
}

export const makeId = (() => {
  let counter = 0;
  return () => `msg-${Date.now()}-${++counter}`;
})();

export const makeSessionId = () =>
  `sess-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
