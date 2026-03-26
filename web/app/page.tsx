"use client";

import { useSync } from "../lib/use-sync";
import { CursorOverlay } from "../lib/cursor-overlay";
import { useRef, useCallback, useEffect } from "react";

function getWsUrl(): string {
  if (process.env.NEXT_PUBLIC_WS_URL) return process.env.NEXT_PUBLIC_WS_URL;
  if (typeof window === "undefined") return "ws://localhost:8081/ws";
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/ws`;
}

export default function EditorPage() {
  const {
    document,
    setDocument,
    isConnected,
    serverVersion,
    clientId,
    remoteCursors,
    setCursorPosition,
  } = useSync({ serverUrl: getWsUrl() });

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const overlayRef = useRef<HTMLDivElement>(null);
  const cursorRef = useRef<{ start: number; end: number } | null>(null);

  useEffect(() => {
    if (textareaRef.current && cursorRef.current) {
      textareaRef.current.selectionStart = cursorRef.current.start;
      textareaRef.current.selectionEnd = cursorRef.current.end;
      cursorRef.current = null;
    }
  }, [document]);

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      cursorRef.current = {
        start: e.target.selectionStart,
        end: e.target.selectionEnd,
      };
      setCursorPosition(e.target.selectionStart);
      setDocument(e.target.value);
    },
    [setDocument, setCursorPosition]
  );

  const handleSelect = useCallback(() => {
    if (textareaRef.current) {
      setCursorPosition(textareaRef.current.selectionStart);
    }
  }, [setCursorPosition]);

  const handleScroll = useCallback(() => {
    if (textareaRef.current && overlayRef.current) {
      overlayRef.current.scrollTop = textareaRef.current.scrollTop;
      overlayRef.current.scrollLeft = textareaRef.current.scrollLeft;
    }
  }, []);

  const statusSeparator = (
    <span className="text-zinc-300 dark:text-zinc-700">|</span>
  );

  return (
    <div className="min-h-screen bg-zinc-50 dark:bg-zinc-950 p-4 sm:p-8">
      <div className="max-w-3xl mx-auto">
        <header className="mb-6">
          <h1 className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
            Collaborative Editor
          </h1>
          <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
            Real-time differential synchronization
          </p>
        </header>

        <div className="flex items-center gap-4 mb-4 text-sm text-zinc-600 dark:text-zinc-400">
          <div className="flex items-center gap-1.5">
            <span
              className={`inline-block w-2 h-2 rounded-full ${
                isConnected
                  ? "bg-emerald-500"
                  : "bg-red-500 animate-pulse"
              }`}
            />
            <span>{isConnected ? "Connected" : "Disconnected"}</span>
          </div>
          {statusSeparator}
          <span>Server v{serverVersion}</span>
          {statusSeparator}
          <span className="font-mono text-xs">{clientId}</span>
          {remoteCursors.length > 0 && (
            <>
              {statusSeparator}
              <div className="flex items-center gap-1.5">
                {remoteCursors.map((c) => (
                  <span
                    key={c.client_id}
                    className="inline-block w-2 h-2 rounded-full"
                    style={{ backgroundColor: c.color }}
                    title={c.client_id}
                  />
                ))}
                <span>
                  {remoteCursors.length} other
                  {remoteCursors.length !== 1 ? "s" : ""}
                </span>
              </div>
            </>
          )}
        </div>

        <div className="relative">
          <textarea
            ref={textareaRef}
            className="w-full h-96 p-4 rounded-lg border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900 text-zinc-900 dark:text-zinc-100 font-mono text-sm leading-relaxed resize-y focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
            value={document}
            onChange={handleChange}
            onSelect={handleSelect}
            onScroll={handleScroll}
            placeholder={
              isConnected
                ? "Start typing to collaborate..."
                : "Connecting to server..."
            }
            disabled={!isConnected}
            spellCheck={false}
          />
          <div ref={overlayRef}>
            <CursorOverlay text={document} cursors={remoteCursors} />
          </div>
        </div>

        <div className="mt-2 text-xs text-zinc-400 dark:text-zinc-600">
          {document.length} characters
        </div>
      </div>
    </div>
  );
}
