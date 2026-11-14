import type { ComparisonStatus, LibraryEntry } from './types';
const key = (entry?: LibraryEntry) => entry?.project?.ship_key || entry?.ship_metadata.key || '';
export function comparableProjects(entries: LibraryEntry[], leftId: string): LibraryEntry[] {
  const left = entries.find((entry) => entry.id === leftId); const ship = key(left);
  return !left?.available || !ship ? [] : entries.filter((entry) => entry.id !== leftId && entry.available && key(entry) === ship);
}
export function validPair(entries: LibraryEntry[], leftId: string, rightId: string): boolean {
  return comparableProjects(entries, leftId).some((entry) => entry.id === rightId);
}
export interface ComparisonTransport { create: (left: string, right: string, refresh: boolean, signal: AbortSignal) => Promise<ComparisonStatus>; status: (id: string, signal: AbortSignal) => Promise<ComparisonStatus> }
function pause(signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    signal.throwIfAborted();
    const abort = () => { clearTimeout(timer); reject(signal.reason); };
    const timer = setTimeout(() => { signal.removeEventListener('abort', abort); resolve(); }, 1000);
    signal.addEventListener('abort', abort, { once: true });
  });
}
export function createComparisonLoader(api: ComparisonTransport, publish: (status: ComparisonStatus | null, error: string, expired: boolean) => void, wait: (signal: AbortSignal) => Promise<void> = pause) {
  let generation = 0; let controller: AbortController | null = null;
  return {
    load: async (left: string, right: string, refresh = false) => {
      const token = ++generation; controller?.abort(); const ctl = controller = new AbortController();
      publish(null, '', false);
      try {
        let status = await api.create(left, right, refresh, ctl.signal);
        for (let poll = 0; token === generation; poll++) {
          if (status.left.project_id !== left || status.right.project_id !== right) throw new Error('Comparison identity mismatch');
          if (status.state === 'ready' && (!status.left.url || !status.right.url || !status.camera || status.stale)) throw new Error('Incomplete comparison images');
          const pending = status.state === 'queued' || status.state === 'rendering';
          publish(status, '', pending && poll >= 120);
          if (!pending || poll >= 120) break;
          await wait(ctl.signal); if (token !== generation) break;
          status = await api.status(status.id, ctl.signal);
        }
      } catch (error: any) { if (token === generation) publish(null, error?.message || String(error), false); }
    },
    cancel: () => { generation++; controller?.abort(); controller = null; },
  };
}
