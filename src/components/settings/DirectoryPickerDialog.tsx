import { useState, useEffect, useCallback } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Loader2, Folder, ArrowUp, ChevronRight } from "lucide-react";
import {
  resolvePendingPick,
  getPendingPickDefaultPath,
} from "@/lib/api/tauri-mock/core";

const API_BASE = "http://127.0.0.1:2891";

// ── Directory listing response from the backend ──

interface ListDirResponse {
  path: string;
  parent: string | null;
  directories: string[];
}

// ── Internal dialog component ──

interface DirectoryPickerDialogProps {
  isOpen: boolean;
  initialPath: string;
  onConfirm: (path: string) => void;
  onCancel: () => void;
}

function DirectoryPickerDialog({
  isOpen,
  initialPath,
  onConfirm,
  onCancel,
}: DirectoryPickerDialogProps) {
  const [currentPath, setCurrentPath] = useState(initialPath);
  const [parentPath, setParentPath] = useState<string | null>(null);
  const [directories, setDirectories] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Reset path when dialog opens with a new initialPath
  useEffect(() => {
    if (isOpen) {
      setCurrentPath(initialPath);
    }
  }, [isOpen, initialPath]);

  // Load directory listing when path changes
  useEffect(() => {
    if (!isOpen) return;
    loadDirectory(currentPath);
  }, [isOpen, currentPath]);

  const loadDirectory = async (path: string) => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetch(`${API_BASE}/api/invoke`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          command: "list_directory",
          args: { path },
        }),
      });
      const body = await res.json();
      if (!body.success) {
        throw new Error(body.error || "Unknown error");
      }
      const data: ListDirResponse = body.data;
      setDirectories(data.directories);
      setParentPath(data.parent);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to load directory",
      );
      setDirectories([]);
    } finally {
      setLoading(false);
    }
  };

  const navigateTo = useCallback(
    (dir: string) => {
      const newPath =
        currentPath === "/" ? `/${dir}` : `${currentPath}/${dir}`;
      setCurrentPath(newPath);
    },
    [currentPath],
  );

  const navigateToParent = useCallback(() => {
    if (parentPath) {
      setCurrentPath(parentPath);
    }
  }, [parentPath]);

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <DialogContent className="max-w-lg max-h-[70vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>选择目录</DialogTitle>
        </DialogHeader>

        {/* Current path with up-button */}
        <div className="flex items-center gap-2 px-1">
          <Button
            variant="ghost"
            size="sm"
            onClick={navigateToParent}
            disabled={!parentPath}
            className="h-8 w-8 p-0 shrink-0"
            title="上级目录"
          >
            <ArrowUp className="h-4 w-4" />
          </Button>
          <div className="flex-1 truncate text-sm text-muted-foreground bg-muted rounded px-2 py-1 select-all">
            {currentPath}
          </div>
        </div>

        {/* Directory listing */}
        <div className="flex-1 min-h-0 mt-2">
          <ScrollArea className="h-64">
            {loading ? (
              <div className="flex items-center justify-center h-full">
                <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
              </div>
            ) : error ? (
              <div className="p-4 text-sm text-destructive">{error}</div>
            ) : directories.length === 0 ? (
              <div className="p-4 text-sm text-muted-foreground text-center">
                空目录
              </div>
            ) : (
              <div className="space-y-0.5">
                {directories.map((dir) => (
                  <button
                    key={dir}
                    onClick={() => navigateTo(dir)}
                    className="w-full flex items-center gap-2 px-2 py-1.5 text-sm rounded hover:bg-accent hover:text-accent-foreground transition-colors text-left"
                  >
                    <Folder className="h-4 w-4 shrink-0 text-muted-foreground" />
                    <span className="truncate">{dir}</span>
                    <ChevronRight className="h-3 w-3 ml-auto shrink-0 text-muted-foreground" />
                  </button>
                ))}
              </div>
            )}
          </ScrollArea>
        </div>

        <DialogFooter className="mt-4 gap-2">
          <Button variant="outline" onClick={onCancel}>
            取消
          </Button>
          <Button onClick={() => onConfirm(currentPath)}>
            选择当前目录
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ── Global dialog that listens for the pick-directory event ──

export function DirectoryPickerGlobal() {
  const [isOpen, setIsOpen] = useState(false);
  const [defaultPath, setDefaultPath] = useState("/");

  useEffect(() => {
    const handler = () => {
      setDefaultPath(getPendingPickDefaultPath() || "/");
      setIsOpen(true);
    };
    window.addEventListener("cc-switch:pick-directory", handler);
    return () => window.removeEventListener("cc-switch:pick-directory", handler);
  }, []);

  const handleConfirm = useCallback((path: string) => {
    setIsOpen(false);
    resolvePendingPick(path);
  }, []);

  const handleCancel = useCallback(() => {
    setIsOpen(false);
    resolvePendingPick(null);
  }, []);

  return (
    <DirectoryPickerDialog
      isOpen={isOpen}
      initialPath={defaultPath}
      onConfirm={handleConfirm}
      onCancel={handleCancel}
    />
  );
}
