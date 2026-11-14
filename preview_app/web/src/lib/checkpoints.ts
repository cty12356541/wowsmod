import type { CheckpointDiff } from './types';
export function checkpointChanges(diff: CheckpointDiff): number { return diff.added.length + diff.removed.length + diff.changed.length; }
export function createCheckpointLoader<T>(read: (signal: AbortSignal) => Promise<T>, publish: (value: T | null, error: string, busy: boolean) => void) {
  let generation = 0; let controller: AbortController | null = null; let value: T | null = null;
  return {
    load: async () => {
      const token = ++generation; controller?.abort(); const ctl = controller = new AbortController();
      publish(value, '', true);
      try { const result = await read(ctl.signal); if (token === generation) { value = result; publish(value, '', false); } }
      catch (error: any) { if (token === generation) publish(value, error.message || String(error), false); }
    },
    cancel: () => { generation++; controller?.abort(); controller = null; },
  };
}
