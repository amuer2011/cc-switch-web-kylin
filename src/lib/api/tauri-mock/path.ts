/**
 * Tauri `@tauri-apps/api/path` mock for browser/web-server mode.
 */

export async function homeDir(): Promise<string> {
  return "/home/user";
}

export async function appConfigDir(): Promise<string> {
  return "/home/user/.cc-switch";
}

export async function join(...paths: string[]): Promise<string> {
  return paths.join("/");
}
