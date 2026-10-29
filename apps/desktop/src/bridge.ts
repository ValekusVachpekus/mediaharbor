import { request, type Response } from '@mediaharbor/shared';
export type Api = <T = unknown>(op: string, payload?: unknown) => Promise<T>;
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
declare global { interface Window { __TAURI__?: { core: { invoke: Invoke } }; } }
export const invoke: Invoke = (command, args) => {
  if (!window.__TAURI__) return Promise.reject(new Error('Откройте установленное приложение MediaHarbor. В браузере доступен только просмотр интерфейса.'));
  return window.__TAURI__.core.invoke(command, args);
};
export const api: Api = async <T>(op: string, payload?: unknown) => {
  const response = await invoke<Response<T>>('rpc', { request: request(op, payload) });
  if (!response.ok) throw new Error(response.error?.message || 'Неизвестная ошибка');
  return response.data as T;
};
export const errorText = (error: unknown) => error instanceof Error ? error.message : String(error);
