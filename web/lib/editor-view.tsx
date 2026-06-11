"use client";

import { useSync } from "./use-sync";
import { Editor, type EditorHandle } from "./editor";
import { byteToCharOffset } from "./diff";
import { useRef, useState, useCallback } from "react";
import type { CursorInfo } from "./protocol";

function copyToClipboard(text: string) {
  if (navigator.clipboard?.writeText) {
    navigator.clipboard.writeText(text).catch(() => fallbackCopy(text));
  } else {
    fallbackCopy(text);
  }
}

function fallbackCopy(text: string) {
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.style.position = "fixed";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.select();
  document.execCommand("copy");
  document.body.removeChild(textarea);
}

function getWsUrl(): string {
  if (process.env.NEXT_PUBLIC_WS_URL) return process.env.NEXT_PUBLIC_WS_URL;
  if (typeof window === "undefined") return "ws://localhost:8081/ws";
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/ws`;
}

function cursorsToCharOffsets(
  cursors: CursorInfo[],
  docText: string
): { clientId: string; position: number; color: string }[] {
  return cursors.map((c) => ({
    clientId: c.client_id,
    position: byteToCharOffset(docText, c.position),
    color: c.color,
  }));
}

interface EditorViewProps {
  sessionToken?: string;
  onSessionClosed?: () => void;
}

export function EditorView({ sessionToken, onSessionClosed }: EditorViewProps) {
  const editorRef = useRef<EditorHandle>(null);
  const [linkCopied, setLinkCopied] = useState(false);
  const [charCount, setCharCount] = useState(0);
  const [closePrompt, setClosePrompt] = useState(false);
  const [secretInput, setSecretInput] = useState("");
  const [closeError, setCloseError] = useState<string | null>(null);
  const [closing, setClosing] = useState(false);

  const handleRemoteEdits = useCallback(
    (edits: import("./diff").EditList) => {
      editorRef.current?.applyRemoteEdits(edits);
    },
    []
  );

  const {
    document: syncDoc,
    setDocument,
    isConnected,
    serverVersion,
    clientId,
    remoteCursors,
    setCursorPosition,
  } = useSync({
    serverUrl: getWsUrl(),
    sessionToken,
    onRemoteEdits: handleRemoteEdits,
  });

  // Only update cursor decorations when positions actually change by value.
  // Between updates, CodeMirror's DecorationSet.map(tr.changes) keeps
  // decorations in sync with local typing.
  const lastCursorsKeyRef = useRef("");
  const cursorsKey = remoteCursors
    .map((c) => `${c.client_id}:${c.position}`)
    .join(",");
  if (cursorsKey !== lastCursorsKeyRef.current) {
    lastCursorsKeyRef.current = cursorsKey;
    editorRef.current?.updateRemoteCursors(
      cursorsToCharOffsets(remoteCursors, syncDoc)
    );
  }

  const handleLocalChange = useCallback(
    (content: string) => {
      setDocument(content);
      setCharCount(content.length);
    },
    [setDocument]
  );

  const handleCursorChange = useCallback(
    (position: number) => {
      setCursorPosition(position);
    },
    [setCursorPosition]
  );

  const handleShare = useCallback(() => {
    if (sessionToken && typeof window !== "undefined") {
      const url = `${window.location.origin}/s/${sessionToken}`;
      copyToClipboard(url);
      setLinkCopied(true);
      setTimeout(() => setLinkCopied(false), 2000);
    }
  }, [sessionToken]);

  const closeWithSecret = useCallback(
    async (secret: string) => {
      if (!sessionToken || typeof window === "undefined") return;
      setClosing(true);
      setCloseError(null);
      try {
        const res = await fetch(`/api/sessions/${sessionToken}/close`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ creator_secret: secret }),
        });

        if (res.ok) {
          sessionStorage.removeItem(`creator_secret_${sessionToken}`);
          onSessionClosed?.();
          return;
        }

        if (res.status === 403) {
          setCloseError("Invalid creator secret.");
        } else if (res.status === 410) {
          setCloseError("This session is already closed.");
        } else if (res.status === 404) {
          setCloseError("Session not found.");
        } else {
          setCloseError("Failed to close session. Try again.");
        }
      } catch {
        setCloseError("Couldn't reach the server. Try again.");
      } finally {
        setClosing(false);
      }
    },
    [sessionToken, onSessionClosed]
  );

  const handleCloseClick = useCallback(() => {
    if (!sessionToken || typeof window === "undefined") return;
    const stored = sessionStorage.getItem(`creator_secret_${sessionToken}`);
    if (stored) {
      closeWithSecret(stored);
    } else {
      // Tab was reopened (or it's a different device) — the secret isn't in
      // sessionStorage, so ask the creator to paste the secret they saved.
      setCloseError(null);
      setClosePrompt(true);
    }
  }, [sessionToken, closeWithSecret]);

  const handleSecretSubmit = useCallback(
    (e: React.FormEvent) => {
      e.preventDefault();
      const trimmed = secretInput.trim();
      if (trimmed) closeWithSecret(trimmed);
    },
    [secretInput, closeWithSecret]
  );

  const statusSeparator = (
    <span className="text-zinc-300 dark:text-zinc-700">|</span>
  );

  return (
    <>
      <div className="flex items-center gap-4 mb-4 text-sm text-zinc-600 dark:text-zinc-400 flex-wrap">
        <div className="flex items-center gap-1.5">
          <span
            className={`inline-block w-2 h-2 rounded-full ${
              isConnected ? "bg-emerald-500" : "bg-red-500 animate-pulse"
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
        {sessionToken && (
          <>
            {statusSeparator}
            <button
              onClick={handleShare}
              className="text-blue-500 hover:text-blue-400 cursor-pointer"
            >
              {linkCopied ? "Copied!" : "Copy link"}
            </button>
            {statusSeparator}
            <button
              onClick={handleCloseClick}
              disabled={closing}
              className="text-red-500 hover:text-red-400 cursor-pointer disabled:opacity-50 disabled:cursor-default"
            >
              {closing ? "Closing…" : "Close session"}
            </button>
          </>
        )}
      </div>

      {closePrompt && (
        <form
          onSubmit={handleSecretSubmit}
          className="flex items-center gap-2 mb-4 text-sm flex-wrap"
        >
          <input
            type="text"
            autoFocus
            value={secretInput}
            onChange={(e) => setSecretInput(e.target.value)}
            placeholder="Paste your creator secret"
            className="font-mono text-xs px-2 py-1 rounded border border-zinc-300 dark:border-zinc-700 bg-white dark:bg-zinc-900 text-zinc-800 dark:text-zinc-200 min-w-[20rem]"
          />
          <button
            type="submit"
            disabled={closing || !secretInput.trim()}
            className="text-red-500 hover:text-red-400 cursor-pointer disabled:opacity-50 disabled:cursor-default"
          >
            {closing ? "Closing…" : "Confirm close"}
          </button>
          <button
            type="button"
            onClick={() => {
              setClosePrompt(false);
              setSecretInput("");
              setCloseError(null);
            }}
            className="text-zinc-500 hover:text-zinc-400 cursor-pointer"
          >
            Cancel
          </button>
          {closeError && <span className="text-red-500">{closeError}</span>}
        </form>
      )}

      {closeError && !closePrompt && (
        <div className="mb-4 text-sm text-red-500">{closeError}</div>
      )}

      {isConnected ? (
        <Editor
          ref={editorRef}
          initialContent={syncDoc}
          onLocalChange={handleLocalChange}
          onCursorChange={handleCursorChange}
          disabled={false}
          placeholder="Start typing to collaborate..."
        />
      ) : (
        <div className="w-full h-96 p-4 rounded-lg border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900 text-zinc-400 dark:text-zinc-600 font-mono text-sm">
          Connecting to server...
        </div>
      )}

      <div className="mt-2 text-xs text-zinc-400 dark:text-zinc-600">
        {charCount} characters
      </div>
    </>
  );
}
