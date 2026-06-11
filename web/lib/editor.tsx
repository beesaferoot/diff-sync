"use client";

import {
  useRef,
  useEffect,
  useImperativeHandle,
  forwardRef,
  type Ref,
} from "react";
import { EditorState, Annotation, Compartment, type ChangeSpec } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { markdown } from "@codemirror/lang-markdown";
import { oneDark } from "@codemirror/theme-one-dark";
import { byteToCharOffset, type Edit, type EditList } from "./diff";
import {
  remoteCursorsExtension,
  type RemoteCursorState,
} from "./remote-cursors";

const remoteAnnotation = Annotation.define<boolean>();

function editToChangeSpec(edit: Edit, doc: string): ChangeSpec {
  if ("Insert" in edit) {
    const charPos = byteToCharOffset(doc, edit.Insert.pos);
    return { from: charPos, insert: edit.Insert.text };
  }
  if ("Delete" in edit) {
    const charFrom = byteToCharOffset(doc, edit.Delete.pos);
    const charTo = byteToCharOffset(doc, edit.Delete.pos + edit.Delete.len);
    return { from: charFrom, to: charTo };
  }
  // Replace
  const charFrom = byteToCharOffset(doc, edit.Replace.pos);
  const charTo = byteToCharOffset(
    doc,
    edit.Replace.pos + edit.Replace.old_len
  );
  return { from: charFrom, to: charTo, insert: edit.Replace.new_text };
}

export interface EditorHandle {
  applyRemoteEdits: (edits: EditList) => void;
  getCursorPosition: () => number;
  updateRemoteCursors: (cursors: RemoteCursorState[]) => void;
}

interface EditorProps {
  initialContent: string;
  onLocalChange: (content: string) => void;
  onCursorChange?: (position: number) => void;
  disabled?: boolean;
  placeholder?: string;
}

export const Editor = forwardRef(function Editor(
  {
    initialContent,
    onLocalChange,
    onCursorChange,
    disabled,
    placeholder,
  }: EditorProps,
  ref: Ref<EditorHandle>
) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const editableCompartment = useRef(new Compartment());
  const onLocalChangeRef = useRef(onLocalChange);
  const onCursorChangeRef = useRef(onCursorChange);
  onLocalChangeRef.current = onLocalChange;
  onCursorChangeRef.current = onCursorChange;

  useImperativeHandle(ref, () => ({
    applyRemoteEdits(editList: EditList) {
      const view = viewRef.current;
      if (!view || editList.edits.length === 0) return;

      const doc = view.state.doc.toString();
      const changes: ChangeSpec[] = editList.edits.map((edit) =>
        editToChangeSpec(edit, doc)
      );

      view.dispatch({
        changes,
        annotations: remoteAnnotation.of(true),
      });
    },
    getCursorPosition() {
      const view = viewRef.current;
      if (!view) return 0;
      return view.state.selection.main.head;
    },
    updateRemoteCursors(cursors: RemoteCursorState[]) {
      const view = viewRef.current;
      if (!view) return;
      view.dispatch({
        effects: remoteCursorsExtension.update.of(cursors),
      });
    },
  }));

  useEffect(() => {
    if (!containerRef.current) return;

    const isDark = window.matchMedia("(prefers-color-scheme: dark)").matches;

    const state = EditorState.create({
      doc: initialContent,
      extensions: [
        markdown(),
        EditorView.lineWrapping,
        ...(isDark ? [oneDark] : []),
        ...(placeholder
          ? [
              EditorView.contentAttributes.of({
                "aria-placeholder": placeholder,
              }),
            ]
          : []),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            const isRemote = update.transactions.some((tr) =>
              tr.annotation(remoteAnnotation)
            );
            if (!isRemote) {
              onLocalChangeRef.current(update.state.doc.toString());
            }
          }
          if (update.selectionSet) {
            onCursorChangeRef.current?.(update.state.selection.main.head);
          }
        }),
        editableCompartment.current.of(EditorView.editable.of(!disabled)),
        remoteCursorsExtension.extension,
        EditorView.theme({
          "&": {
            height: "24rem",
            fontSize: "0.875rem",
            border: "1px solid var(--border-color, #e4e4e7)",
            borderRadius: "0.5rem",
            overflow: "hidden",
          },
          ".cm-scroller": {
            fontFamily: "ui-monospace, monospace",
            lineHeight: "1.625",
            padding: "1rem",
            overflow: "auto",
          },
          ".cm-focused": {
            outline: "2px solid #3b82f6",
            outlineOffset: "-1px",
            borderColor: "transparent",
          },
          "&.cm-editor": {
            backgroundColor: "var(--editor-bg, white)",
          },
        }),
      ],
    });

    const view = new EditorView({ state, parent: containerRef.current });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // Only run on mount — content updates come through applyRemoteEdits
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: editableCompartment.current.reconfigure(
        EditorView.editable.of(!disabled)
      ),
    });
  }, [disabled]);

  return (
    <div
      ref={containerRef}
      className="[--border-color:theme(colors.zinc.200)] dark:[--border-color:theme(colors.zinc.800)] [--editor-bg:white] dark:[--editor-bg:theme(colors.zinc.900)]"
    />
  );
});
