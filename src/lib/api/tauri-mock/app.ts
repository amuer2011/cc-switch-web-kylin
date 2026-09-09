/**
 * Tauri `@tauri-apps/api/app` mock for browser/web-server mode.
 */

export async function getVersion(): Promise<string> {
  try {
    const res = await fetch("/api/invoke", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ command: "health", args: {} }),
    });
    const body = await res.json();
    if (body.success && body.data?.version) {
      return body.data.version;
    }
  } catch {
    // fall through
  }
  return "3.14.1";
}

export async function getName(): Promise<string> {
  return "cc-switch-web";
}

export async function getTauriVersion(): Promise<string> {
  return "0.0.0";
}
