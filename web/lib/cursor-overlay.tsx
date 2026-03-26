"use client";

import { useMemo } from "react";
import type { CursorInfo } from "./protocol";

interface CursorOverlayProps {
  text: string;
  cursors: CursorInfo[];
}

/**
 * Renders remote cursor name tags as an overlay on top of a textarea.
 *
 * Uses a transparent mirrored div with identical font/padding/sizing,
 * positioned absolutely over the textarea. Text is invisible; only the
 * colored name labels at each cursor position are visible.
 */
export function CursorOverlay({ text, cursors }: CursorOverlayProps) {
  const segments = useMemo(() => {
    if (cursors.length === 0) return null;

    const sorted = [...cursors].sort((a, b) => a.position - b.position);
    const parts: React.ReactNode[] = [];
    let lastIndex = 0;

    sorted.forEach((cursor, i) => {
      const pos = Math.min(cursor.position, text.length);

      if (pos > lastIndex) {
        parts.push(
          <span key={`text-${i}`}>{text.slice(lastIndex, pos)}</span>
        );
      }

      const label = cursor.client_id.replace(/^web_/, "").slice(0, 6);
      parts.push(
        <span
          key={`cursor-${cursor.client_id}`}
          className="relative inline-block w-0"
          style={{ height: 0 }}
        >
          <span
            className="absolute left-0 text-[10px] font-sans font-medium leading-none px-1 py-0.5 rounded whitespace-nowrap"
            style={{
              top: "-14px",
              backgroundColor: cursor.color,
              color: "white",
            }}
          >
            {label}
          </span>
        </span>
      );

      lastIndex = pos;
    });

    if (lastIndex < text.length) {
      parts.push(<span key="text-end">{text.slice(lastIndex)}</span>);
    }

    parts.push(<br key="trailing" />);
    return parts;
  }, [text, cursors]);

  if (!segments) return null;

  return (
    <div
      className="absolute inset-0 p-4 font-mono text-sm leading-relaxed whitespace-pre-wrap break-words text-transparent pointer-events-none overflow-hidden"
      aria-hidden
    >
      {segments}
    </div>
  );
}
