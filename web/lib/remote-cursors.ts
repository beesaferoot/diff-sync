import {
  StateEffect,
  StateField,
  type Extension,
} from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  WidgetType,
} from "@codemirror/view";

export interface RemoteCursorState {
  clientId: string;
  position: number; // character offset (already converted from bytes)
  color: string;
}

const updateCursors = StateEffect.define<RemoteCursorState[]>();

class CursorWidget extends WidgetType {
  constructor(
    readonly color: string,
    readonly label: string
  ) {
    super();
  }

  toDOM(): HTMLElement {
    const wrapper = document.createElement("span");
    wrapper.className = "cm-remote-cursor";
    wrapper.style.borderLeft = `2px solid ${this.color}`;
    wrapper.style.marginLeft = "-1px";
    wrapper.style.position = "relative";

    const tag = document.createElement("span");
    tag.className = "cm-remote-cursor-label";
    tag.textContent = this.label;
    tag.style.position = "absolute";
    tag.style.top = "-1.4em";
    tag.style.left = "-1px";
    tag.style.fontSize = "10px";
    tag.style.lineHeight = "1";
    tag.style.padding = "1px 4px";
    tag.style.borderRadius = "3px";
    tag.style.backgroundColor = this.color;
    tag.style.color = "white";
    tag.style.whiteSpace = "nowrap";
    tag.style.pointerEvents = "none";
    tag.style.fontFamily = "system-ui, sans-serif";
    tag.style.fontWeight = "500";

    wrapper.appendChild(tag);
    return wrapper;
  }

  eq(other: CursorWidget): boolean {
    return this.color === other.color && this.label === other.label;
  }
}

function buildDecorations(
  cursors: RemoteCursorState[],
  docLength: number
): DecorationSet {
  if (cursors.length === 0) return Decoration.none;

  const decorations = cursors
    .map((c) => {
      const pos = Math.min(c.position, docLength);
      const label = c.clientId.replace(/^web_/, "").slice(0, 6);
      return Decoration.widget({
        widget: new CursorWidget(c.color, label),
        side: 1,
      }).range(pos);
    })
    .sort((a, b) => a.from - b.from);

  return Decoration.set(decorations);
}

const cursorsField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(value, tr) {
    // Map existing decoration positions through any document changes
    if (tr.docChanged) {
      value = value.map(tr.changes);
    }
    // Replace with fresh positions when a new update arrives from the server
    for (const effect of tr.effects) {
      if (effect.is(updateCursors)) {
        return buildDecorations(effect.value, tr.state.doc.length);
      }
    }
    return value;
  },
  provide: (field) => EditorView.decorations.from(field),
});

export const remoteCursorsExtension = {
  extension: cursorsField as Extension,
  update: updateCursors,
};
