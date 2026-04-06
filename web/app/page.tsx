"use client";

import { useState, useEffect, useCallback } from "react";
import { EditorView } from "../lib/editor-view";

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

type ViewState =
  | { type: "landing" }
  | { type: "creating" }
  | { type: "session"; token: string }
  | { type: "closed" }
  | { type: "not_found" }
  | { type: "error"; message: string };

function getApiBase(): string {
  if (typeof window === "undefined") return "";
  return window.location.origin;
}

function getTokenFromPath(): string | null {
  if (typeof window === "undefined") return null;
  const match = window.location.pathname.match(/^\/s\/([A-Za-z0-9_-]+)$/);
  return match ? match[1] : null;
}

export default function Page() {
  const [view, setView] = useState<ViewState>({ type: "landing" });
  const [secretCopied, setSecretCopied] = useState(false);
  const [creatorInfo, setCreatorInfo] = useState<{
    token: string;
    secret: string;
  } | null>(null);

  const navigateFromPath = useCallback(() => {
    const token = getTokenFromPath();
    if (!token) {
      setView({ type: "landing" });
      return;
    }

    fetch(`${getApiBase()}/api/sessions/${token}`)
      .then((res) => {
        if (res.status === 404) {
          setView({ type: "not_found" });
          return null;
        }
        return res.json();
      })
      .then((data) => {
        if (!data) return;
        if (data.status === "closed") {
          setView({ type: "closed" });
        } else {
          setView({ type: "session", token });
        }
      })
      .catch((e) => setView({ type: "error", message: e.message }));
  }, []);

  useEffect(() => {
    navigateFromPath();
    window.addEventListener("popstate", navigateFromPath);
    return () => window.removeEventListener("popstate", navigateFromPath);
  }, [navigateFromPath]);

  const handleCreate = useCallback(async () => {
    setView({ type: "creating" });
    try {
      const res = await fetch(`${getApiBase()}/api/sessions`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({}),
      });
      if (!res.ok) throw new Error(await res.text());
      const data = await res.json();

      sessionStorage.setItem(`creator_secret_${data.token}`, data.creator_secret);
      setCreatorInfo({ token: data.token, secret: data.creator_secret });
      window.history.pushState(null, "", `/s/${data.token}`);
      setView({ type: "session", token: data.token });
    } catch (e) {
      setView({
        type: "error",
        message: e instanceof Error ? e.message : "Failed to create session",
      });
    }
  }, []);

  const handleSessionClosed = useCallback(() => {
    setView({ type: "closed" });
  }, []);

  const handleCopySecret = useCallback(() => {
    if (creatorInfo) {
      copyToClipboard(creatorInfo.secret);
      setSecretCopied(true);
      setTimeout(() => setSecretCopied(false), 2000);
    }
  }, [creatorInfo]);

  const handleDismissSecret = useCallback(() => {
    setCreatorInfo(null);
  }, []);

  const handleBackToHome = useCallback(() => {
    window.history.pushState(null, "", "/");
    setView({ type: "landing" });
    setCreatorInfo(null);
  }, []);

  return (
    <div className="min-h-screen bg-zinc-50 dark:bg-zinc-950 p-4 sm:p-8">
      <div className="max-w-3xl mx-auto">
        <header className="mb-6">
          <h1 className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
            {view.type === "landing" ? "Collaborative Editor" : (
              <button
                onClick={handleBackToHome}
                className="hover:text-blue-500 transition-colors cursor-pointer"
              >
                Collaborative Editor
              </button>
            )}
          </h1>
          <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
            Real-time differential synchronization
          </p>
        </header>

        {creatorInfo && view.type === "session" && (
          <div className="mb-4 p-4 rounded-lg bg-amber-50 dark:bg-amber-950 border border-amber-200 dark:border-amber-800">
            <p className="text-sm font-medium text-amber-900 dark:text-amber-100 mb-1">
              Save your creator secret — it won&apos;t be shown again
            </p>
            <p className="text-xs text-amber-700 dark:text-amber-300 mb-2">
              You need this to close the session. It&apos;s stored in this tab only.
            </p>
            <div className="flex items-center gap-2">
              <code className="text-xs bg-amber-100 dark:bg-amber-900 px-2 py-1 rounded font-mono">
                {creatorInfo.secret}
              </code>
              <button
                onClick={handleCopySecret}
                className="text-xs text-amber-700 dark:text-amber-300 hover:text-amber-900 dark:hover:text-amber-100 cursor-pointer"
              >
                {secretCopied ? "Copied!" : "Copy"}
              </button>
              <button
                onClick={handleDismissSecret}
                className="text-xs text-amber-500 hover:text-amber-700 ml-auto cursor-pointer"
              >
                Dismiss
              </button>
            </div>
          </div>
        )}

        {view.type === "landing" && (
          <div className="flex flex-col items-center justify-center py-20">
            <p className="text-zinc-600 dark:text-zinc-400 mb-6 text-center max-w-md">
              Create a private session and share the link with collaborators.
              No accounts needed.
            </p>
            <button
              onClick={handleCreate}
              className="px-6 py-3 bg-blue-600 hover:bg-blue-500 text-white font-medium rounded-lg transition-colors cursor-pointer"
            >
              Create Session
            </button>
          </div>
        )}

        {view.type === "creating" && (
          <div className="flex items-center justify-center py-20 text-zinc-500">
            Creating session...
          </div>
        )}

        {view.type === "session" && (
          <EditorView
            sessionToken={view.token}
            onSessionClosed={handleSessionClosed}
          />
        )}

        {view.type === "closed" && (
          <div className="flex flex-col items-center justify-center py-20">
            <p className="text-zinc-500 dark:text-zinc-400 mb-4">
              This session has ended.
            </p>
            <button
              onClick={handleBackToHome}
              className="text-blue-500 hover:text-blue-400 cursor-pointer"
            >
              Create a new session
            </button>
          </div>
        )}

        {view.type === "not_found" && (
          <div className="flex flex-col items-center justify-center py-20">
            <p className="text-zinc-500 dark:text-zinc-400 mb-4">
              Session not found.
            </p>
            <button
              onClick={handleBackToHome}
              className="text-blue-500 hover:text-blue-400 cursor-pointer"
            >
              Create a new session
            </button>
          </div>
        )}

        {view.type === "error" && (
          <div className="flex flex-col items-center justify-center py-20">
            <p className="text-red-500 mb-4">{view.message}</p>
            <button
              onClick={handleBackToHome}
              className="text-blue-500 hover:text-blue-400 cursor-pointer"
            >
              Back to home
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
