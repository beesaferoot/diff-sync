/**
 * TypeScript SyncEngine mirroring the Rust SyncEngine in src/sync.rs.
 *
 * Maintains a document (current working copy) and a shadow (last state agreed
 * upon with the server). The diff between shadow and document captures local
 * changes; incoming edits from the server are applied to both.
 */

import { diff, patch, type EditList } from "./diff";

export class SyncEngine {
  private document: string;
  private shadow: string;
  private version: number;
  public readonly nodeId: string;

  constructor(content: string, nodeId: string) {
    this.document = content;
    this.shadow = content;
    this.version = 0;
    this.nodeId = nodeId;
  }

  text(): string {
    return this.document;
  }

  getVersion(): number {
    return this.version;
  }

  edit(newContent: string): void {
    this.document = newContent;
  }

  /** Diff document against shadow, advance shadow, return outgoing edits. */
  diffAndUpdateShadow(): EditList {
    const edits = diff(this.shadow, this.document);
    this.shadow = this.document;
    return edits;
  }

  /** Apply incoming server edits to both shadow and document. */
  applyEdits(editList: EditList): void {
    if (editList.edits.length === 0) return;
    this.shadow = patch(this.shadow, editList);
    this.document = patch(this.document, editList);
    this.version++;
  }
}
