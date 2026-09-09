/**
 * Tauri `@tauri-apps/plugin-updater` mock for browser/web-server mode.
 */

export interface Update {
  version: string;
  date: string;
  body: string;
}

export async function check(): Promise<Update | null> {
  return null;
}
