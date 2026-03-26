/**
 * TypeScript types matching the Rust SyncMessage enum's serde JSON serialization.
 *
 * Serde uses externally-tagged representation by default:
 *   SyncMessage::Connect { client_id } → { "Connect": { "client_id": "..." } }
 *   SyncMessage::Ping                  → "Ping"
 */

import type { EditList } from "./diff";

export interface Document {
  content: string;
  version: number;
}

export interface CursorInfo {
  client_id: string;
  position: number;
  color: string;
}

export type SyncMessage =
  | { Connect: { client_id: string } }
  | {
      ClientSync: {
        client_id: string;
        edits: EditList;
        client_version: number;
        cursor_position: number | null;
      };
    }
  | {
      ServerSync: {
        edits: EditList;
        server_version: number;
        cursors: CursorInfo[];
      };
    }
  | { ConnectOk: { server_version: number; document: Document } }
  | { Error: { message: string } }
  | { Disconnect: { client_id: string } }
  | "Ping"
  | "Pong";
