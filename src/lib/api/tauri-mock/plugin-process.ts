/**
 * Tauri `@tauri-apps/plugin-process` mock for browser/web-server mode.
 */

export async function exit(_code?: number): Promise<void> {
  console.warn("[Process] exit() called — would exit in Tauri mode");
}

export async function relaunch(): Promise<void> {
  window.location.reload();
}
