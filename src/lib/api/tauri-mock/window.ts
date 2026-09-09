/**
 * Tauri `@tauri-apps/api/window` mock for browser/web-server mode.
 */

class WebWindow {
  async minimize(): Promise<void> {}
  async maximize(): Promise<void> {}
  async unminimize(): Promise<void> {}
  async show(): Promise<void> {}
  async hide(): Promise<void> {}
  async close(): Promise<void> {}
  async setDecorations(_decorations: boolean): Promise<void> {}
  async setFocus(): Promise<void> {}
  async setSkipTaskbar(_skip: boolean): Promise<void> {}
  async setTitle(_title: string): Promise<void> {}
  async setResizable(_resizable: boolean): Promise<void> {}
  async setSize(_size: { width: number; height: number }): Promise<void> {}
  async setMinSize(_size: { width: number; height: number }): Promise<void> {}
  async setMaxSize(_size: { width: number; height: number }): Promise<void> {}
  async setPosition(_pos: { x: number; y: number }): Promise<void> {}
  async center(): Promise<void> {}
  async onResized(_cb: () => void): Promise<() => void> {
    return () => {};
  }
  async onMoved(_cb: () => void): Promise<() => void> {
    return () => {};
  }
  async onCloseRequested(_cb: () => void): Promise<() => void> {
    return () => {};
  }
  async onFocusChanged(_cb: () => void): Promise<() => void> {
    return () => {};
  }
  innerPosition(): { x: number; y: number } {
    return { x: 0, y: 0 };
  }
  outerPosition(): { x: number; y: number } {
    return { x: 0, y: 0 };
  }
  innerSize(): { width: number; height: number } {
    return { width: window.innerWidth, height: window.innerHeight };
  }
  outerSize(): { width: number; height: number } {
    return { width: window.outerWidth, height: window.outerHeight };
  }
  scaleFactor(): number {
    return window.devicePixelRatio || 1;
  }
  get label(): string {
    return "main";
  }
}

let currentWindow: WebWindow | null = null;

export function getCurrentWindow(): WebWindow {
  if (!currentWindow) {
    currentWindow = new WebWindow();
  }
  return currentWindow;
}

export function getAllWindows(): WebWindow[] {
  return [getCurrentWindow()];
}

export type WindowLabel = string;
