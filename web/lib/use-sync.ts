"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { SyncEngine } from "./sync-engine";
import type { SyncMessage, CursorInfo } from "./protocol";

const SYNC_INTERVAL_MS = 500;
const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 10000;

interface UseSyncOptions {
  serverUrl: string;
  clientId?: string;
}

interface UseSyncResult {
  document: string;
  setDocument: (content: string) => void;
  isConnected: boolean;
  serverVersion: number;
  clientId: string;
  remoteCursors: CursorInfo[];
  setCursorPosition: (position: number) => void;
}

function generateClientId(): string {
  return `web_${Math.random().toString(36).slice(2, 10)}`;
}

export function useSync({
  serverUrl,
  clientId: providedId,
}: UseSyncOptions): UseSyncResult {
  const [clientId] = useState(() => providedId ?? generateClientId());
  const [document, setDocumentState] = useState("");
  const [isConnected, setIsConnected] = useState(false);
  const [serverVersion, setServerVersion] = useState(0);
  const [remoteCursors, setRemoteCursors] = useState<CursorInfo[]>([]);

  const engineRef = useRef<SyncEngine | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const syncIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(
    null
  );
  const reconnectDelayRef = useRef(RECONNECT_BASE_MS);
  const mountedRef = useRef(true);
  const cursorPositionRef = useRef<number | null>(null);

  const setDocument = useCallback((content: string) => {
    if (engineRef.current) {
      engineRef.current.edit(content);
    }
    setDocumentState(content);
  }, []);

  const setCursorPosition = useCallback((position: number) => {
    cursorPositionRef.current = position;
  }, []);

  const stopSyncInterval = useCallback(() => {
    if (syncIntervalRef.current) {
      clearInterval(syncIntervalRef.current);
      syncIntervalRef.current = null;
    }
  }, []);

  const startSyncInterval = useCallback(
    (ws: WebSocket, engine: SyncEngine) => {
      stopSyncInterval();
      syncIntervalRef.current = setInterval(() => {
        if (ws.readyState !== WebSocket.OPEN) return;

        const edits = engine.diffAndUpdateShadow();
        const msg: SyncMessage = {
          ClientSync: {
            client_id: clientId,
            edits,
            client_version: engine.getVersion(),
            cursor_position: cursorPositionRef.current,
          },
        };
        ws.send(JSON.stringify(msg));
      }, SYNC_INTERVAL_MS);
    },
    [clientId, stopSyncInterval]
  );

  const connect = useCallback(() => {
    if (!mountedRef.current) return;

    const ws = new WebSocket(serverUrl);
    wsRef.current = ws;

    ws.onopen = () => {
      reconnectDelayRef.current = RECONNECT_BASE_MS;
      const msg: SyncMessage = { Connect: { client_id: clientId } };
      ws.send(JSON.stringify(msg));
    };

    ws.onmessage = (event) => {
      let msg: SyncMessage;
      try {
        msg = JSON.parse(event.data);
      } catch {
        console.error("Failed to parse server message:", event.data);
        return;
      }

      if (typeof msg === "object" && msg !== null) {
        if ("ConnectOk" in msg) {
          const { document: doc, server_version } = msg.ConnectOk;
          const engine = new SyncEngine(doc.content, clientId);
          engineRef.current = engine;
          setDocumentState(doc.content);
          setServerVersion(server_version);
          setIsConnected(true);
          startSyncInterval(ws, engine);
        } else if ("ServerSync" in msg) {
          const { edits, server_version, cursors } = msg.ServerSync;
          setServerVersion(server_version);
          setRemoteCursors(cursors);
          if (edits.edits.length > 0 && engineRef.current) {
            engineRef.current.applyEdits(edits);
            setDocumentState(engineRef.current.text());
          }
        } else if ("Error" in msg) {
          console.error("Server error:", msg.Error.message);
        }
      }
    };

    ws.onclose = () => {
      setIsConnected(false);
      setRemoteCursors([]);
      stopSyncInterval();

      if (!mountedRef.current) return;

      const delay = reconnectDelayRef.current;
      reconnectDelayRef.current = Math.min(delay * 2, RECONNECT_MAX_MS);
      console.log(`Reconnecting in ${delay}ms...`);
      reconnectTimeoutRef.current = setTimeout(connect, delay);
    };

    ws.onerror = (err) => {
      console.error("WebSocket error:", err);
    };
  }, [serverUrl, clientId, startSyncInterval, stopSyncInterval]);

  useEffect(() => {
    mountedRef.current = true;
    connect();

    return () => {
      mountedRef.current = false;
      stopSyncInterval();
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
      }
      if (wsRef.current) {
        wsRef.current.close();
      }
    };
  }, [connect, stopSyncInterval]);

  return {
    document,
    setDocument,
    isConnected,
    serverVersion,
    clientId,
    remoteCursors,
    setCursorPosition,
  };
}
