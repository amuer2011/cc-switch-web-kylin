/**
 * Tauri `@tauri-apps/api/event` mock for browser/web-server mode.
 */

export type UnlistenFn = () => void;

interface Event<T> {
  payload: T;
  id: number;
}

export async function listen<T>(
  _event: string,
  _handler: (event: Event<T>) => void | Promise<void>,
): Promise<UnlistenFn> {
  return () => {};
}

export async function once<T>(
  _event: string,
  _handler: (event: Event<T>) => void | Promise<void>,
): Promise<UnlistenFn> {
  return () => {};
}

export async function emit(_event: string, _payload?: unknown): Promise<void> {
  // no-op in web mode
}
