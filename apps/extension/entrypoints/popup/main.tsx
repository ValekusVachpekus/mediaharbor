import React, { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { browser } from 'wxt/browser';
import { request, type CollectionItem, type Job, type Response } from '@mediaharbor/shared';
import { DownloadPane, Notice } from '../../../desktop/src/panes';
import { errorText, type Api } from '../../../desktop/src/bridge';
import '../../../desktop/src/style.css';

const api: Api = async <T,>(op: string, payload?: unknown) => {
  const response: Response<T> = await browser.runtime.sendMessage({ type: 'rpc', request: request(op, payload) });
  if (!response?.ok) throw new Error(response?.error?.message || 'Расширение не получило ответ от приложения');
  return response.data as T;
};
interface Scan { source: string; items: CollectionItem[]; expected?: number; running: boolean; complete: boolean; message: string; }
function Popup() {
  const [tab, setTab] = useState('download'); const [source, setSource] = useState(''); const [loadedSource, setLoadedSource] = useState(''); const [initial, setInitial] = useState<CollectionItem[]>(); const [jobs, setJobs] = useState<Job[]>([]); const [scan, setScan] = useState<Scan>(); const [error, setError] = useState('');
  useEffect(() => {
    void (async () => { const data = await browser.storage.local.get(['pendingSource', 'spotifyImport']); const [current] = await browser.tabs.query({ active: true, currentWindow: true }); setSource(typeof data.pendingSource === 'string' ? data.pendingSource : current?.url || ''); setScan(data.spotifyImport as Scan | undefined); await browser.storage.local.remove('pendingSource'); await browser.action.setBadgeText({ text: '' }); })();
    const listener = (changes: Record<string, { newValue?: unknown }>) => { if (changes.spotifyImport) setScan(changes.spotifyImport.newValue as Scan); };
    browser.storage.onChanged.addListener(listener); return () => browser.storage.onChanged.removeListener(listener);
  }, []);
  async function refresh() { try { const page = await api<{ items: Job[] }>('jobs.list', { offset: 0 }); setJobs(page.items); setError(''); } catch (e) { setError(errorText(e)); } }
  useEffect(() => { if (tab !== 'queue') return; void refresh(); const timer = setInterval(() => void refresh(), 2000); return () => clearInterval(timer); }, [tab]);
  async function loadPage() {
    setError(''); try {
      if (!source) throw new Error('Ссылка текущей страницы недоступна');
      setLoadedSource(source);
    } catch (e) { setError(errorText(e)); }
  }
  async function scanCommand(type: string) { setError(''); try { const result = await browser.runtime.sendMessage({ type }); if (!result?.ok) throw new Error(result?.error || 'Сбор не запущен'); } catch (e) { setError(errorText(e)); } }
  return <><header className="extension-header"><strong>MediaHarbor <span style={{ color: 'var(--accent)' }}>↓</span></strong><span className="badge">Локальная загрузка</span></header><div className="extension-tabs">{([['download', 'Загрузить'], ['queue', 'Очередь'], ['spotify', 'Импорт Spotify']] as const).map(([id, name]) => <button className={tab === id ? 'selected' : ''} key={id} onClick={() => setTab(id)}>{name}</button>)}</div><div className="extension-content">{error && <Notice error>{error}</Notice>}
  {tab === 'download' && <><button disabled={!source} style={{ width: '100%', marginBottom: 12 }} onClick={() => void loadPage()}>Подставить ссылку текущей страницы</button><DownloadPane compact api={api} initialItems={initial} initialSource={loadedSource} onQueued={() => setTab('queue')}/></>}
  {tab === 'queue' && <>{!jobs.length && <p className="muted">Задач пока нет. Полная история доступна в приложении.</p>}{jobs.map(job => <article className="card" key={job.id}><strong>{job.input.title}</strong><p className="muted small">{job.stage} · {Math.round(job.progress)}%</p><div className="progress"><span style={{ width: `${job.progress}%` }}/></div>{job.error && <Notice error>{job.error}</Notice>}{job.output && <p className="file-path">{job.output}</p>}{['running', 'queued'].includes(job.status) && <button onClick={() => void api('jobs.cancel', { id: job.id }).then(refresh).catch(e => setError(errorText(e)))}>Отменить</button>}{['failed', 'cancelled', 'interrupted'].includes(job.status) && <button onClick={() => void api('jobs.retry', { id: job.id }).then(refresh).catch(e => setError(errorText(e)))}>Повторить</button>}</article>)}{!!jobs.length && <p className="muted small">Показаны последние 50 задач. Полная очередь — в программе.</p>}</>}
  {tab === 'spotify' && <section className="card"><h3>Импорт открытого плейлиста</h3><p className="muted small">Откройте плейлист или альбом Spotify. Расширение прокрутит список и соберёт названия треков. Не переключайте страницу во время сбора.</p><div className="toolbar"><button disabled={scan?.running} onClick={() => void scanCommand('scan.start')}>Собрать треки</button><button disabled={!scan?.running} onClick={() => void scanCommand('scan.stop')}>Остановить</button></div>{scan && <><div className="scan-status">{scan.message}</div><p className="small muted">{scan.source}</p>{!scan.complete && !scan.running && <Notice>Список может быть неполным. Проверьте количество треков или используйте CSV / официальный API.</Notice>}<button className="primary" disabled={scan.running || !scan.items.length} onClick={() => { setInitial(scan.items); setTab('download'); }}>{scan.complete ? 'Проверить записи' : 'Импортировать найденные записи'}</button></>}<p className="muted small">API Spotify подключается в настройках программы. Аудио будет найдено на YouTube; совпадения подтверждаются перед загрузкой.</p></section>}
  </div></>;
}
createRoot(document.getElementById('root')!).render(<Popup/>);
