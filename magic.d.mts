import type { AudioSession } from './index.js';

export type MagicState = 'active' | 'inactive';

export interface WatchAppOptions {
  /** The loudest session's volume changed. */
  onVolume?(volume: number): void;
  /** Whether any session is muted changed. */
  onMute?(muted: boolean): void;
  /** Whether any session is active changed. Fires for jscaw's own changes too. */
  onState?(state: MagicState): void;
  /** A matching session appeared, expired or disconnected. */
  onSessionsChanged?(sessions: AudioSession[]): void;
  /** Also fire `onVolume`/`onMute` for changes made through this process's jscaw. Default `false`. */
  includeSelf?: boolean;
}

export interface MagicApp {
  /** The tracked sessions, as last seen. */
  readonly sessions: AudioSession[];
  /** Get: the loudest session's volume, `null` with no sessions. Set: applies to every session. */
  volume: number | null;
  /** Get: `true` if any session is muted, `null` with no sessions. Set: applies to every session. */
  mute: boolean | null;
  readonly state: MagicState;
  toggleMute(): void;
  /** Moves the volume by `step` (default `0.1`, negative to lower), clamped to 0..1. */
  stepVolume(step?: number): void;
  /** Stops watching; the process can then exit. Safe to call more than once. */
  dispose(): void;
}

/**
 * Watches every default-device session of `exeNames` (case-insensitive) as they come and go,
 * and controls them as one. Keeps the process alive until `dispose()`.
 */
export function watchApp(exeNames: string | string[], options?: WatchAppOptions): MagicApp;
