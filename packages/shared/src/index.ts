export const PROTOCOL_VERSION = 1;
export type JobStatus = 'queued' | 'running' | 'completed' | 'failed' | 'cancelled' | 'interrupted';
export interface MediaCandidate { id: string; title: string; url: string; artist?: string; duration?: number; thumbnail?: string; height?: number; }
export interface CollectionItem extends MediaCandidate { position?: number; selected?: boolean; candidate?: MediaCandidate; candidates?: MediaCandidate[]; error?: string; }
export interface CompressionOptions { format: 'mp4' | 'mkv' | 'mp3' | 'm4a' | 'opus'; preset: 'quality' | 'balanced' | 'small'; height?: number; audioBitrate: number; crf?: number; targetMb?: number; }
export interface DownloadOptions { mode: 'video' | 'audio'; format: 'mp4' | 'mkv' | 'mp3' | 'm4a' | 'opus' | 'original'; height: number; audioBitrate: number; }
export interface JobInput { kind: 'download' | 'compress'; source: string; title: string; download?: DownloadOptions; compression?: CompressionOptions; }
export interface Job { id: string; input: JobInput; status: JobStatus; stage: string; progress: number; createdAt: number; updatedAt: number; output?: string; original?: string; error?: string; warning?: string; }
export interface Settings { outputDir: string; theme: 'dark' | 'light' | 'system'; spotifyClientId: string; toolsDir: string; }
export interface ToolInfo { name: string; available: boolean; version: string; }
export interface Request { version: number; requestId: string; op: string; payload?: unknown; }
export interface Response<T = unknown> { version: number; requestId: string; ok: boolean; data?: T; error?: { code: string; message: string }; }
export interface JobEvent { version: 1; event: 'job.updated'; job: Job; }
export function request(op: string, payload?: unknown): Request { return { version: 1, requestId: crypto.randomUUID(), op, payload }; }
