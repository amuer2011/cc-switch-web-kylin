/**
 * Tauri `@tauri-apps/api/core` mock for browser/web-server mode.
 * Replaces Tauri's IPC invoke() with HTTP fetch() against the REST API server.
 */

const API_BASE = "http://127.0.0.1:2891";

interface InvokeResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

// ── Directory picker interception ──
// In web-server mode there's no native file dialog, so we show a custom
// directory browser dialog when `pick_directory` is invoked.

interface PendingPick {
  resolve: (path: string | null) => void;
  defaultPath: string | null;
}

let pendingPick: PendingPick | null = null;

/** Called by DirectoryPickerGlobal after user selects/cancels. */
export function resolvePendingPick(value: string | null) {
  pendingPick?.resolve(value);
  pendingPick = null;
}

/** Called by DirectoryPickerGlobal to read the default path from the current request. */
export function getPendingPickDefaultPath(): string | null {
  return pendingPick?.defaultPath ?? null;
}

function pickDirectoryViaDialog(defaultPath?: string): Promise<string | null> {
  return new Promise<string | null>((resolve) => {
    pendingPick = { resolve, defaultPath: defaultPath ?? null };
    window.dispatchEvent(new CustomEvent("cc-switch:pick-directory"));
  });
}

// ── SQL file import state ──
// When the user picks a .sql file for import, we read the content in the browser
// and store it here. The subsequent `import_config_from_file` invoke then sends
// this content to the backend via the `import_sql_content` API instead of a file path.

let pendingImportSqlContent: string | null = null;

/** Open a hidden `<input type="file">` to pick a .sql file and read its content. */
function openFileDialogViaBrowser(): Promise<string | null> {
  return new Promise<string | null>((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".sql";

    input.addEventListener("change", () => {
      const file = input.files?.[0];
      if (!file) {
        resolve(null);
        return;
      }
      const reader = new FileReader();
      reader.addEventListener("load", () => {
        pendingImportSqlContent = reader.result as string;
        // Return a pseudo-path so the caller has a truthy "selected file" value.
        resolve(file.name);
      });
      reader.addEventListener("error", () => {
        pendingImportSqlContent = null;
        resolve(null);
      });
      reader.readAsText(file);
    });

    input.click();
  });
}

/** Trigger a browser download of the given text content as a .sql file. */
function downloadSqlContent(content: string, filename: string) {
  const blob = new Blob([content], { type: "application/sql" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

// ── ZIP file install state ──
// When the user picks a .zip/.skill file for install-from-zip, we read it as base64
// in the browser and store it here. The subsequent `install_skills_from_zip` invoke
// sends this base64 content to the backend instead of a file path.

let pendingInstallZipBase64: string | null = null;

/** Open a hidden `<input type="file">` to pick a .zip/.skill file and read as base64. */
function openZipFileViaBrowser(): Promise<string | null> {
  return new Promise<string | null>((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".zip,.skill";

    input.addEventListener("change", () => {
      const file = input.files?.[0];
      if (!file) {
        resolve(null);
        return;
      }
      const reader = new FileReader();
      reader.addEventListener("load", () => {
        const result = reader.result as string;
        // result is a data URL like "data:application/zip;base64,AAAA..."
        const base64 = result.split(",")[1];
        pendingInstallZipBase64 = base64;
        resolve(file.name);
      });
      reader.addEventListener("error", () => {
        pendingInstallZipBase64 = null;
        resolve(null);
      });
      reader.readAsDataURL(file);
    });

    input.click();
  });
}

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  // Intercept pick_directory to show a custom directory browser dialog.
  // Tauri mode uses a native OS dialog; web-server mode has no such capability.
  if (command === "pick_directory") {
    return pickDirectoryViaDialog(args?.defaultPath as string | undefined) as Promise<T>;
  }

  // Intercept open_file_dialog — browser-based file picker for .sql files
  if (command === "open_file_dialog") {
    return openFileDialogViaBrowser() as Promise<T>;
  }

  // Intercept save_file_dialog — return the default filename (triggers download later)
  if (command === "save_file_dialog") {
    const defaultName = (args?.defaultName as string) || "cc-switch-export.sql";
    return Promise.resolve(defaultName) as Promise<T>;
  }

  // Intercept open_zip_file_dialog — browser-based file picker for .zip/.skill files
  if (command === "open_zip_file_dialog") {
    return openZipFileViaBrowser() as Promise<T>;
  }

  // Intercept install_skills_from_zip — send base64 content instead of file path
  if (command === "install_skills_from_zip") {
    const content = pendingInstallZipBase64;
    pendingInstallZipBase64 = null; // consume
    if (!content) {
      throw new Error("请先选择 ZIP 文件");
    }
    const currentApp = args?.currentApp as string;
    const res = await fetch(`${API_BASE}/api/invoke`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        command: "install_skills_from_zip",
        args: { content, currentApp },
      }),
    });
    const body = await res.json();
    if (!body.success) {
      throw new Error(body.error ?? "Install from ZIP failed");
    }
    return body.data as T;
  }

  // Intercept import_config_from_file — use stored SQL content instead of a file path
  if (command === "import_config_from_file") {
    const content = pendingImportSqlContent;
    pendingImportSqlContent = null; // consume
    if (!content) {
      throw new Error("请选择有效的 SQL 备份文件");
    }
    const res = await fetch(`${API_BASE}/api/invoke`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ command: "import_sql_content", args: { content } }),
    });
    const body = await res.json();
    if (!body.success) {
      throw new Error(body.error ?? "Import failed");
    }
    return body.data as T;
  }

  // Intercept export_config_to_file — get SQL content via API and trigger browser download
  if (command === "export_config_to_file") {
    const res = await fetch(`${API_BASE}/api/invoke`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ command: "export_sql_string", args: {} }),
    });
    const body = await res.json();
    if (!body.success) {
      throw new Error(body.error ?? "Export failed");
    }
    const filePath = (args?.filePath as string) || "cc-switch-export.sql";
    const content = body.data?.content as string;
    if (content) {
      downloadSqlContent(content, filePath);
    }
    return { success: true, filePath } as T;
  }

  const res = await fetch(`${API_BASE}/api/invoke`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ command, args }),
  });

  if (!res.ok) {
    throw new Error(`HTTP ${res.status}: ${res.statusText}`);
  }

  const body: InvokeResponse<T> = await res.json();

  if (!body.success) {
    throw new Error(body.error ?? "Unknown error");
  }

  return body.data as T;
}
