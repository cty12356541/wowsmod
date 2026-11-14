import type { CompatibilityInfo, VerificationRecord } from './types';

export function matchingVerification(info: CompatibilityInfo): VerificationRecord | null {
  if (!info.current_revision || !info.target_build) return null;
  // The server preserves append order, including same-second saves and clock rollback.
  return [...info.records].reverse().find((record) => record.project_id === info.project_id &&
    record.revision === info.current_revision && record.build === info.target_build) || null;
}
export function buildDiffers(projectBuild: string | null, targetBuild: string | null): boolean {
  return !!projectBuild && !!targetBuild && projectBuild !== targetBuild;
}
export function createCompatibilityLoader(fetcher: (id: string, signal: AbortSignal) => Promise<CompatibilityInfo>, publish: (data: CompatibilityInfo | null, error: string, loading: boolean) => void) {
  let generation = 0; let controller: AbortController | null = null;
  return {
    load: async (id: string) => {
      const token = ++generation; controller?.abort();
      const ctl = controller = new AbortController(); publish(null, '', true);
      try {
        const result = await fetcher(id, ctl.signal);
        if (result.project_id !== id) throw new Error('Project identity mismatch');
        if (token === generation) publish(result, '', false);
      } catch (error: any) { if (token === generation) publish(null, error.message || String(error), false); }
    },
    cancel: () => { generation++; controller?.abort(); controller = null; },
  };
}
