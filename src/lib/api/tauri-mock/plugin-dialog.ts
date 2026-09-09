/**
 * Tauri `@tauri-apps/plugin-dialog` mock for browser/web-server mode.
 */

interface MessageOptions {
  title?: string;
  kind?: "info" | "warning" | "error";
}

export async function message(
  message: string,
  options?: MessageOptions,
): Promise<void> {
  console.log(`[Dialog] ${options?.kind ?? "info"}: ${message}`);
  alert(message);
}

export async function ask(
  message: string,
  _options?: MessageOptions,
): Promise<boolean> {
  return confirm(message);
}

export async function confirm(
  message: string,
  _options?: MessageOptions,
): Promise<boolean> {
  return window.confirm(message);
}
