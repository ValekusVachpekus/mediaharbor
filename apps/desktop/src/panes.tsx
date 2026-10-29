import React, { useEffect, useRef, useState } from 'react';
import type { CollectionItem, CompressionOptions, DownloadOptions, JobInput, MediaCandidate } from '@mediaharbor/shared';
import { importTracks } from '@mediaharbor/shared/import';
import type { Api } from './bridge';
import { errorText, invoke } from './bridge';

export const defaultCompression: CompressionOptions = { format: 'mp4', preset: 'balanced', height: 1080, audioBitrate: 192 };
export const defaultDownload: DownloadOptions = { mode: 'video', format: 'mp4', height: 1080, audioBitrate: 192 };
export function duration(seconds?: number) { if (!seconds) return '—'; return `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, '0')}`; }
export function Notice({ children, error = false }: { children: React.ReactNode; error?: boolean }) { return <div role={error ? 'alert' : 'status'} className={`notice ${error ? 'error' : ''}`}>{children}</div>; }
export function SourceLink({ url }: { url: string }) {
  if (!/^https:\/\/(?:www\.youtube\.com|open\.spotify\.com)\//.test(url)) return null;
  const spotify = url.includes('open.spotify.com');
  return <a className="source-link" href={url} target="_blank" rel="noreferrer" onClick={e => { if (window.__TAURI__) { e.preventDefault(); void invoke('open_external', { url }).catch(() => {}); } }}>{spotify && <svg aria-hidden="true" width="12" height="12" viewBox="0 0 24 24"><circle cx="12" cy="12" r="12" fill="#1ed760"/><path d="M5 9c5-2 10-1 14 1M6 13c4-1.5 8-1 11 1M7 17c3-1 6-.8 9 .8" fill="none" stroke="#10251e" strokeWidth="1.8" strokeLinecap="round"/></svg>}{spotify ? 'Spotify' : 'YouTube'} ↗</a>;
}
export function CompressionFields({ value, onChange, audioOnly = false }: { value: CompressionOptions; onChange: (v: CompressionOptions) => void; audioOnly?: boolean }) {
  const update = (patch: Partial<CompressionOptions>) => onChange({ ...value, ...patch });
  return <div className="profile-fields">
    <div className="presets">{[['quality', 'Высокое качество'], ['balanced', 'Баланс'], ['small', 'Меньше размер']].map(([key, title]) => <button type="button" className={value.preset === key ? 'selected' : ''} key={key} onClick={() => update({ preset: key as CompressionOptions['preset'], crf: undefined, audioBitrate: key === 'quality' ? 256 : key === 'small' ? 96 : 192 })}>{title}</button>)}</div>
    <div className="field-grid">
      <label>Формат<select value={value.format} onChange={e => update({ format: e.target.value as CompressionOptions['format'] })}>{(audioOnly ? ['mp3', 'm4a', 'opus'] : ['mp4', 'mkv', 'mp3', 'm4a', 'opus']).map(f => <option key={f} value={f}>{f.toUpperCase()}</option>)}</select></label>
      {['mp4', 'mkv'].includes(value.format) && <label>Максимальное разрешение<select value={value.height || 0} onChange={e => update({ height: Number(e.target.value) || undefined })}>{[[0, 'Как в исходнике'], [2160, '2160p · 4K'], [1440, '1440p'], [1080, '1080p'], [720, '720p'], [480, '480p']].map(([n, title]) => <option key={n} value={n}>{title}</option>)}</select></label>}
      <label>Аудио, кбит/с<select value={value.audioBitrate} onChange={e => update({ audioBitrate: Number(e.target.value) })}>{[64, 96, 128, 192, 256, 320].map(n => <option key={n}>{n}</option>)}</select></label>
    </div>
    <details><summary>Дополнительные параметры</summary><div className="field-grid">
      <label>Целевой размер, МБ<input type="number" min="0.1" step="0.1" placeholder="Не задан" value={value.targetMb ?? ''} onChange={e => update({ targetMb: e.target.value ? Number(e.target.value) : undefined })}/></label>
      {['mp4', 'mkv'].includes(value.format) && <label>CRF · меньше — лучше<input type="number" min="0" max="40" disabled={!!value.targetMb} placeholder={value.preset === 'quality' ? '18' : value.preset === 'small' ? '28' : '23'} value={value.crf ?? ''} onChange={e => update({ crf: e.target.value ? Number(e.target.value) : undefined })}/></label>}
    </div><p className="muted">Целевой размер — оценка. Для видео используются два прохода; исходники сохраняются.</p></details>
  </div>;
}

interface Page { items: MediaCandidate[]; total: number; collectionId?: string; nextOffset?: number | null; }
export function DownloadPane({ api, onQueued, initialItems, initialSource, compact = false }: { api: Api; onQueued: () => void; initialItems?: CollectionItem[]; initialSource?: string; compact?: boolean }) {
  const [source, setSource] = useState(''); const [items, setItems] = useState<CollectionItem[]>(initialItems || []);
  const [options, setOptions] = useState(defaultDownload); const [compression, setCompression] = useState(defaultCompression);
  const [post, setPost] = useState(false); const [busy, setBusy] = useState(''); const [error, setError] = useState(''); const [message, setMessage] = useState('');
  const [filter, setFilter] = useState(''); const [limit, setLimit] = useState(50); const fileRef = useRef<HTMLInputElement>(null);
  const stopped = useRef(false);
  useEffect(() => { if (initialItems) { setItems(initialItems); setOptions({ ...defaultDownload, mode: 'audio', format: 'mp3' }); } }, [initialItems]);
  useEffect(() => { if (initialSource) setSource(initialSource); }, [initialSource]);
  useEffect(() => { if (options.mode === 'audio' && ['mp4', 'mkv'].includes(compression.format)) setCompression(current => ({ ...current, format: 'mp3' })); }, [options.mode, compression.format]);
  useEffect(() => () => { stopped.current = true; }, []);
  async function action(label: string, fn: () => Promise<void>) { setBusy(label); setError(''); setMessage(''); try { await fn(); } catch (e) { setError(errorText(e)); } finally { setBusy(''); } }
  const youtube = (url: string) => { try { return ['youtube.com', 'www.youtube.com', 'music.youtube.com', 'm.youtube.com', 'youtu.be'].includes(new URL(url).hostname); } catch { return false; } };
  function rows(values: MediaCandidate[]) { return values.map((v, i) => ({ ...v, position: i + 1, selected: youtube(v.url), candidate: youtube(v.url) ? v : undefined })); }
  async function load() {
    await action('Получение сведений…', async () => {
      let next: CollectionItem[] = [];
      if (/^https:\/\/(open\.spotify\.com|spotify\.link)\//i.test(source)) {
        setOptions({ ...defaultDownload, mode: 'audio', format: 'mp3' });
        const status = await api<{ connected: boolean }>('spotify.status');
        if (status.connected) {
          let offset: number | null | undefined = 0;
          do { const page: Page = await api('spotify.collection', { source, offset }); next.push(...rows(page.items)); offset = page.nextOffset; } while (offset != null);
        } else { next = rows([await api<MediaCandidate>('spotify.title', { source })]); }
        setMessage('Выберите записи на YouTube. Если название неполное, уточните запрос у трека.');
      } else if (/^https:\/\//i.test(source)) {
        let page: Page = await api('media.analyze', { source }); const collectionId = page.collectionId; next.push(...rows(page.items));
        while (page.nextOffset != null) { page = await api('collection.page', { collectionId, offset: page.nextOffset }); next.push(...rows(page.items)); }
      } else {
        const candidates = await api<MediaCandidate[]>('media.search', { query: source });
        next = [{ id: crypto.randomUUID(), title: source, url: '', selected: false, candidates }]; setOptions({ ...defaultDownload, mode: 'audio', format: 'mp3' });
      }
      // Rows can contain repeated tracks; use position as part of the UI identity.
      setItems(next.map((v, i) => ({ ...v, position: i + 1 }))); setLimit(50);
      if (!next.length) setMessage('Ничего не найдено. Уточните название или проверьте доступность ссылки.');
    });
  }
  async function find(index: number, query?: string) {
    let row = items[index];
    if (!row) return;
    if (!query && row.title === row.url && /^https:\/\/(open\.spotify\.com|spotify\.link)\//.test(row.url)) {
      const status=await api<{connected:boolean}>('spotify.status');
      const metadata=status.connected ? (await api<Page>('spotify.collection',{source:row.url,offset:0})).items[0] : await api<MediaCandidate>('spotify.title',{source:row.url});
      if (metadata) {row={...row,...metadata};const resolved=row;setItems(current=>current.map((v,i)=>i===index?resolved:v));}
    }
    const candidates = await api<MediaCandidate[]>('media.search', { query: query || `${row.artist || ''} ${row.title}`.trim() });
    // Sort by duration and prefer recordings without unexpected live/cover/remix markers.
    candidates.sort((a, b) => score(b, row) - score(a, row));
    setItems(current => current.map((v, i) => i === index ? { ...v, candidates, error: candidates.length ? undefined : 'Совпадений нет' } : v));
  }
  function score(c: MediaCandidate, row: CollectionItem) {
    const words = `${row.artist || ''} ${row.title}`.toLowerCase().split(/\s+/).filter(w => w.length > 2);
    const name = `${c.title} ${c.artist || ''}`.toLowerCase();
    return words.filter(w => name.includes(w)).length * 5 - (row.duration && c.duration ? Math.min(30, Math.abs(row.duration - c.duration)) : 0) - (/live|cover|remix/i.test(name) && !/live|cover|remix/i.test(row.title) ? 15 : 0);
  }
  async function findAll() {
    await action('Подбор записей…', async () => {
      stopped.current = false;
      for (let i = 0; i < items.length && !stopped.current; i++) {
        if (items[i]?.candidate) continue;
        setBusy(`Поиск ${i + 1} из ${items.length}…`);
        try { await find(i); } catch (e) { setItems(current => current.map((v, n) => n === i ? { ...v, error: errorText(e) } : v)); }
      }
    });
  }
  async function enqueue() {
    await action('Добавление в очередь…', async () => {
      const selected = items.filter(v => v.selected && v.candidate);
      for (let offset = 0; offset < selected.length; offset += 50) {
        const batch: JobInput[] = selected.slice(offset, offset + 50).map(v => ({ kind: 'download', title: v.title, source: v.candidate!.url, download: options, compression: post ? compression : undefined }));
        await api('jobs.create', { items: batch });
        const done = new Set(selected.slice(offset, offset + 50));
        setItems(current => current.map(v => done.has(v) ? { ...v, selected: false } : v));
      }
      setItems(current => current.map(v => v.selected && v.candidate ? { ...v, selected: false } : v));
      setMessage(`Добавлено задач: ${selected.length}`); onQueued();
    });
  }
  const visible = items.map((row, index) => ({ row, index })).filter(({ row }) => `${row.title} ${row.artist || ''}`.toLowerCase().includes(filter.toLowerCase()));
  const selected = items.filter(v => v.selected && v.candidate).length;
  return <>
    <section className="card source-card"><div className="eyebrow">YOUTUBE · SPOTIFY · ПОИСК ПО НАЗВАНИЮ</div><h2>Что сохраним сегодня?</h2><p className="muted">Вставьте ссылку на видео, трек или плейлист. Или просто введите название песни.</p>
      <form onSubmit={e => { e.preventDefault(); void load(); }} className="source-form"><input aria-label="Ссылка или название" placeholder="Ссылка или исполнитель — название…" value={source} onChange={e => setSource(e.target.value)} required maxLength={8192}/><button className="primary" disabled={!!busy || !source.trim()}>Найти <span>↗</span></button></form>
      <div className="source-footer"><span className="muted">Файлы сохраняются на вашем компьютере</span><button className="link" type="button" disabled={!!busy} onClick={() => fileRef.current?.click()}>Импорт CSV / TXT</button><input hidden ref={fileRef} type="file" accept=".csv,.txt" onChange={e => { const file = e.target.files?.[0]; if (file) void action('Чтение файла…', async () => { if(file.size>10_000_000) throw new Error('Файл больше 10 МБ'); setItems(rows(importTracks(await file.text(), file.name))); setOptions({ ...defaultDownload, mode: 'audio', format: 'mp3' }); }); e.target.value = ''; }}/></div>
    </section>
    {error && <Notice error>{error}</Notice>}{message && <Notice>{message}</Notice>}
    <section className="card"><div className="section-line"><h3>Параметры загрузки</h3><span className="badge">Исходники сохраняются</span></div>
      <div className="field-grid"><label>Сохранить<select value={options.mode} onChange={e => setOptions({ ...options, mode: e.target.value as 'video' | 'audio', format: e.target.value === 'video' ? 'mp4' : 'mp3' })}><option value="video">Видео с аудио</option><option value="audio">Только аудио</option></select></label>
      <label>Формат<select value={options.format} onChange={e => setOptions({ ...options, format: e.target.value as DownloadOptions['format'] })}>{(options.mode === 'video' ? ['mp4', 'mkv'] : ['mp3', 'm4a', 'opus', 'original']).map(f => <option key={f} value={f}>{f === 'original' ? 'Исходный поток' : f.toUpperCase()}</option>)}</select></label>
      {options.mode === 'video' ? <label>Разрешение до<select value={options.height} onChange={e => setOptions({ ...options, height: Number(e.target.value) })}>{[4320, 2160, 1440, 1080, 720, 480].map(n => <option key={n} value={n}>{n}p</option>)}</select></label> : <label>Битрейт<select disabled={options.format === 'original'} value={options.audioBitrate} onChange={e => setOptions({ ...options, audioBitrate: Number(e.target.value) })}>{[96, 128, 192, 256, 320].map(n => <option key={n} value={n}>{n} кбит/с</option>)}</select></label>}</div>
      <p className="muted small">Перекодирование и высокий битрейт не повышают качество исходной записи.</p>
      <label className="check"><input type="checkbox" checked={post} onChange={e => setPost(e.target.checked)}/>Сжать после скачивания</label>{post && <CompressionFields value={compression} onChange={setCompression} audioOnly={options.mode === 'audio'}/>}
    </section>
    {!!items.length && <section className="card results"><div className="section-line"><h3>Проверка записей <span className="count">{items.length}</span></h3><input className="filter" aria-label="Фильтр записей" placeholder="Фильтр…" value={filter} onChange={e => setFilter(e.target.value)}/></div>
      <div className="toolbar"><button disabled={!!busy} onClick={() => setItems(current => current.map(v => ({ ...v, selected: !!v.candidate })))}>Выбрать найденные</button><button disabled={!!busy} onClick={() => void findAll()}>Найти варианты</button><button disabled={!!busy || !items.some(v => v.candidates?.length)} onClick={() => setItems(current => current.map(v => v.candidates?.length && !v.candidate ? { ...v, candidate: v.candidates[0], selected: true } : v))}>Подтвердить первые варианты</button></div>
      {visible.slice(0, limit).map(({ row, index }) => <article className="track-row" key={`${row.id}-${index}`}><div className="track-main"><input aria-label={`Выбрать ${row.title}`} type="checkbox" disabled={!row.candidate || !!busy} checked={!!row.selected} onChange={e => setItems(current => current.map((v, i) => i === index ? { ...v, selected: e.target.checked } : v))}/><span className="track-number">{index + 1}</span>{row.thumbnail && !compact && <img alt="" loading="lazy" src={row.thumbnail}/>}<div className="track-title"><strong>{row.title}</strong><span className="muted">{row.artist || 'Исполнитель не указан'} · {duration(row.duration)} <SourceLink url={row.url}/></span>{row.candidate && <span className="matched">↳ {row.candidate.title}</span>}</div><button disabled={!!busy} onClick={() => void action('Поиск вариантов…', () => find(index))}>Варианты</button></div>
      {row.error && <Notice error>{row.error}</Notice>}
      {!!row.candidates?.length && <div className="candidates">{row.candidates.map(c => <div className="candidate-wrapper" key={c.id}><button className={`candidate ${row.candidate?.id === c.id ? 'selected' : ''}`} disabled={!!busy} onClick={() => setItems(current => current.map((v, i) => i === index ? { ...v, candidate: c, selected: true } : v))}><strong>{c.title}</strong><span>{c.artist} · {duration(c.duration)} {row.candidate?.id === c.id ? '✓' : ''}</span></button><SourceLink url={c.url}/></div>)}<form onSubmit={e => { e.preventDefault(); const query = new FormData(e.currentTarget).get('query') as string; void action('Поиск…', () => find(index, query)); }} className="source-form"><input name="query" aria-label="Уточнить запрос" defaultValue={`${row.artist || ''} ${row.title}`.trim()}/><button disabled={!!busy}>Уточнить</button></form></div>}</article>)}
      {visible.length > limit && <button onClick={() => setLimit(n => n + 50)}>Показать ещё 50</button>}
      <div className="download-footer"><span className="muted">Выбрано {selected} из {items.length}</span><button className="primary" disabled={!!busy || !selected} onClick={() => void enqueue()}>↓ Скачать выбранное</button></div>
    </section>}
    {busy && <div className="working" role="status"><span className="spinner"/>{busy}{busy.startsWith('Поиск ') && <button onClick={() => { stopped.current = true; }}>Остановить подбор</button>}</div>}
  </>;
}
