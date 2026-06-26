import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { disableEscapeCapture, enableEscapeCapture } from "./api";

/**
 * Close a modal when the user presses Escape.
 *
 * On macOS the WKWebView swallows the Escape keydown (`cancelOperation:`) before any JS handler
 * runs, so a pure-JS listener never fires. We ask the native layer to capture Escape — only while
 * a modal is open — and emit a `modal-escape` event the webview listens for. A document-level
 * capture listener is kept as a fallback for Linux/Windows webviews, which do dispatch keydown.
 *
 * `onClose` is read through a ref so the native shortcut is registered once on mount and released
 * on unmount, regardless of how often the caller re-renders.
 */
export function useEscapeToClose(onClose: () => void): void {
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let mounted = true;

    void enableEscapeCapture();
    void listen("modal-escape", () => onCloseRef.current()).then((fn) => {
      if (mounted) {
        unlisten = fn;
      } else {
        fn();
      }
    });

    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onCloseRef.current();
      }
    }
    document.addEventListener("keydown", onKey, true);

    return () => {
      mounted = false;
      unlisten?.();
      document.removeEventListener("keydown", onKey, true);
      void disableEscapeCapture();
    };
  }, []);
}
