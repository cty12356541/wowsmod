import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { ImageOff, RefreshCw, Ship } from 'lucide-react';
import type { LibraryThumbnailCache } from '../lib/library-thumbnail';
import { t } from '../lib/i18n';
import type { LibraryEntry } from '../lib/types';

export function LibraryThumbnail({ project, cache }: { project?: LibraryEntry; cache: LibraryThumbnailCache }) {
  const cover = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(false);
  const id = project?.available ? project.id : undefined;
  const path = project?.path;
  const source = useMemo(() => id && path ? { id, path } : undefined, [id, path]);

  useEffect(() => {
    const element = cover.current;
    if (!element) return;
    let near = false;
    const update = () => setActive(near && !document.hidden);
    document.addEventListener('visibilitychange', update);
    const observer = new IntersectionObserver(([entry]) => {
      near = entry.isIntersecting;
      update();
    }, { rootMargin: '240px 0px' });
    observer.observe(element);
    return () => {
      observer.disconnect();
      document.removeEventListener('visibilitychange', update);
    };
  }, []);

  const subscribe = useCallback((listener: () => void) => {
    if (source && active) return cache.subscribe(source, listener);
    return () => {};
  }, [cache, source, active]);
  const read = useCallback(() => cache.read(source), [cache, source]);
  const { thumbnail, pending, error, pollingExpired, imageFailed, imageAttempt } = useSyncExternalStore(subscribe, read, read);
  const url = thumbnail?.url;
  const image = url && !imageFailed ? url + (imageAttempt ? `?retry=${imageAttempt}` : '') : null;
  const failed = !!error || thumbnail?.state === 'failed' || imageFailed;
  const missing = !source || thumbnail?.state === 'missing';
  const generating = pending || (!pollingExpired && (thumbnail?.state === 'queued' || thumbnail?.state === 'rendering'));
  const status = error ? 'libThumbFailed'
    : generating ? (thumbnail?.stale && image ? 'libThumbUpdating' : 'libThumbGenerating')
    : failed ? 'libThumbFailed' : pollingExpired ? 'libThumbPending' : missing ? 'libThumbMissing' : '';
  const retry = source && !pending && (failed || pollingExpired || missing);

  return <div className="library-thumbnail" ref={cover}>
    {image ? <img key={image} src={image} alt="" width={640} height={360} loading="lazy" decoding="async" onError={() => source && cache.imageFailed(source, url!)} />
      : <div className="library-thumbnail-placeholder" aria-hidden="true">{failed ? <ImageOff size={24} /> : <Ship size={26} />}</div>}
    {status && <div className="library-thumbnail-feedback">
      <span className="library-thumbnail-status" role="status" title={error || thumbnail?.reason || undefined}>{t(status)}</span>
      {retry && <button type="button" className="btn library-thumbnail-retry" onClick={() => cache.retry(source)}>
        <RefreshCw size={12} aria-hidden="true" />{t('libThumbRetry')}
      </button>}
    </div>}
  </div>;
}
