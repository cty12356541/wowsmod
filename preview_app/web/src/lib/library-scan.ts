import type { ScanCandidate, ScanPreview } from './types';
export function selectedCandidates(candidates: ScanCandidate[], selected: string[]): string[] {
  const selectedSet = new Set(selected);
  return candidates.filter((candidate) => candidate.status === 'new' && selectedSet.has(candidate.candidate_id)).map((candidate) => candidate.candidate_id);
}
export function createScanLoader(fetcher: (root: string, signal: AbortSignal) => Promise<ScanPreview>, publish: (data: ScanPreview | null, error: string, loading: boolean) => void) {
  let generation = 0; let controller: AbortController | null = null;
  return {
    load: async (root: string) => {
      const token = ++generation; controller?.abort();
      const ctl = controller = new AbortController(); publish(null, '', true);
      try { const data = await fetcher(root, ctl.signal); if (token === generation) publish(data, '', false); }
      catch (error: any) { if (token === generation) publish(null, error.message || String(error), false); }
    },
    clear: () => { generation++; controller?.abort(); controller = null; publish(null, '', false); },
  };
}
