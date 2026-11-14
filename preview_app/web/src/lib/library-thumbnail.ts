import type { LibraryEntry, LibraryThumbnailStatus } from './types';

type Project = Pick<LibraryEntry, 'id' | 'path'>;
type Load = (id: string, signal: AbortSignal, refresh: boolean) => Promise<LibraryThumbnailStatus>;
type Listener = () => void;

interface Snapshot {
  thumbnail: LibraryThumbnailStatus | null;
  pending: boolean;
  error: string | null;
  pollingExpired: boolean;
  imageFailed: boolean;
  imageAttempt: number;
}
interface Entry {
  id: string;
  value: Snapshot;
  listeners: Set<Listener>;
  request?: AbortController;
  timer?: ReturnType<typeof setTimeout>;
  checkedAt: number;
  polls: number;
}

const initial: Snapshot = {
  thumbnail: null, pending: false, error: null, pollingExpired: false, imageFailed: false, imageAttempt: 0,
};
const pollInterval = 2000;
const maxPolls = 30;
const recheckAfter = 30000;
const isGenerating = (thumbnail: Snapshot['thumbnail']) => thumbnail?.state === 'queued' || thumbnail?.state === 'rendering';
const projectKey = ({ id, path }: Project) => JSON.stringify([id, path]);

export function representativeProject(projects: LibraryEntry[]): LibraryEntry | undefined {
  return projects.find((project) => project.available);
}

/** One cancellable status request or poll timer per observed library project. */
export class LibraryThumbnailCache {
  private readonly entries = new Map<string, Entry>();
  private readonly load: Load;

  constructor(load: Load) { this.load = load; }

  read(project?: Project): Snapshot {
    return (project && this.entries.get(projectKey(project))?.value) || initial;
  }

  subscribe(project: Project, listener: Listener): () => void {
    const key = projectKey(project);
    let entry = this.entries.get(key);
    if (!entry) {
      entry = { id: project.id, value: initial, listeners: new Set(), checkedAt: 0, polls: 0 };
      this.entries.set(key, entry);
    }
    const observed = entry;
    const first = !observed.listeners.size;
    observed.listeners.add(listener);
    const { thumbnail, error } = observed.value;
    if (first && (!thumbnail || error || isGenerating(thumbnail) || Date.now() - observed.checkedAt >= recheckAfter)) {
      observed.polls = 0;
      void this.fetch(observed);
    }
    return () => {
      observed.listeners.delete(listener);
      if (!observed.listeners.size) this.cancel(observed);
    };
  }

  revalidateVisible() {
    for (const entry of this.entries.values()) {
      if (entry.listeners.size && !entry.request && entry.timer === undefined) {
        entry.polls = 0;
        void this.fetch(entry);
      }
    }
  }

  retry(project: Project) {
    const entry = this.entries.get(projectKey(project));
    if (!entry?.listeners.size) return;
    this.cancel(entry);
    entry.polls = 0;
    const { imageFailed, imageAttempt } = entry.value;
    entry.value = { ...entry.value, imageFailed: false, imageAttempt: imageAttempt + Number(imageFailed) };
    void this.fetch(entry, true);
  }

  imageFailed(project: Project, url: string) {
    const entry = this.entries.get(projectKey(project));
    if (entry?.value.thumbnail?.url === url) this.update(entry, { imageFailed: true });
  }

  private update(entry: Entry, patch: Partial<Snapshot>) {
    entry.value = { ...entry.value, ...patch };
    entry.listeners.forEach((listener) => listener());
  }

  private cancel(entry: Entry) {
    clearTimeout(entry.timer);
    entry.timer = undefined;
    entry.request?.abort();
    entry.request = undefined;
    if (entry.value.pending) entry.value = { ...entry.value, pending: false };
  }

  private async fetch(entry: Entry, refresh = false) {
    const request = new AbortController();
    entry.request = request;
    this.update(entry, { pending: true, error: null, pollingExpired: false });
    let thumbnail: LibraryThumbnailStatus;
    try {
      thumbnail = await this.load(entry.id, request.signal, refresh);
    } catch (error) {
      if (entry.request !== request) return;
      entry.request = undefined;
      this.update(entry, { pending: false, error: error instanceof Error ? error.message : String(error) });
      return;
    }
    if (entry.request !== request) return;
    entry.request = undefined;
    entry.checkedAt = Date.now();
    const polling = isGenerating(thumbnail);
    const pollingExpired = polling && entry.polls >= maxPolls;
    if (polling && !pollingExpired) {
      entry.timer = setTimeout(() => {
        entry.timer = undefined;
        entry.polls++;
        void this.fetch(entry);
      }, pollInterval);
    }
    this.update(entry, {
      thumbnail, pending: false, pollingExpired,
      imageFailed: thumbnail.url === entry.value.thumbnail?.url && entry.value.imageFailed,
      imageAttempt: thumbnail.url === entry.value.thumbnail?.url ? entry.value.imageAttempt : 0,
    });
  }
}
