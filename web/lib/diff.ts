/**
 * Diff and patch engine matching the Rust implementation in src/diff.rs.
 *
 * All positions (pos, len, old_len) are byte offsets — Rust strings are UTF-8
 * byte arrays. We use TextEncoder/TextDecoder to produce compatible offsets
 * so edits can cross the language boundary.
 */

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export type Edit =
  | { Insert: { pos: number; text: string } }
  | { Delete: { pos: number; len: number } }
  | { Replace: { pos: number; old_len: number; new_text: string } };

export interface EditList {
  edits: Edit[];
  checksum: string;
}

/** Simple checksum matching the Rust implementation: byte_len XOR char_code_sum. */
export function checksum(text: string): string {
  let charSum = 0;
  for (const ch of text) {
    charSum += ch.codePointAt(0)!;
  }
  const result = encoder.encode(text).length ^ charSum;
  return (result >>> 0).toString(16);
}

/** Compute the minimal edit list to transform `from` into `to`. */
export function diff(from: string, to: string): EditList {
  if (from === to) {
    return { edits: [], checksum: checksum(from) };
  }

  if (from.length === 0) {
    return {
      edits: [{ Insert: { pos: 0, text: to } }],
      checksum: checksum(from),
    };
  }

  if (to.length === 0) {
    return {
      edits: [{ Delete: { pos: 0, len: encoder.encode(from).length } }],
      checksum: checksum(from),
    };
  }

  const fromChars = Array.from(from);
  const toChars = Array.from(to);

  let commonStart = 0;
  while (
    commonStart < fromChars.length &&
    commonStart < toChars.length &&
    fromChars[commonStart] === toChars[commonStart]
  ) {
    commonStart++;
  }

  let commonEnd = 0;
  while (
    commonEnd < fromChars.length - commonStart &&
    commonEnd < toChars.length - commonStart &&
    fromChars[fromChars.length - 1 - commonEnd] ===
      toChars[toChars.length - 1 - commonEnd]
  ) {
    commonEnd++;
  }

  const prefixBytes = encoder.encode(
    fromChars.slice(0, commonStart).join("")
  ).length;

  const suffixBytes =
    commonEnd > 0
      ? encoder.encode(
          fromChars.slice(fromChars.length - commonEnd).join("")
        ).length
      : 0;

  const fromMiddleBytes =
    encoder.encode(from).length - prefixBytes - suffixBytes;
  const toMiddle = toChars
    .slice(commonStart, toChars.length - commonEnd || undefined)
    .join("");

  const edits: Edit[] = [];

  if (fromMiddleBytes > 0 || toMiddle.length > 0) {
    if (fromMiddleBytes === 0) {
      edits.push({ Insert: { pos: prefixBytes, text: toMiddle } });
    } else if (toMiddle.length === 0) {
      edits.push({ Delete: { pos: prefixBytes, len: fromMiddleBytes } });
    } else {
      edits.push({
        Replace: {
          pos: prefixBytes,
          old_len: fromMiddleBytes,
          new_text: toMiddle,
        },
      });
    }
  }

  return { edits, checksum: checksum(from) };
}

/**
 * Apply edits to `text`, returning the transformed result.
 * Edits are applied in reverse order to avoid cascading position shifts.
 * Positions are clamped to byte-array bounds for fuzzy-patch tolerance.
 */
export function patch(text: string, editList: EditList): string {
  if (editList.edits.length === 0) {
    return text;
  }

  let bytes = encoder.encode(text);

  for (let i = editList.edits.length - 1; i >= 0; i--) {
    const edit = editList.edits[i];

    if ("Insert" in edit) {
      const pos = Math.min(edit.Insert.pos, bytes.length);
      const insertBytes = encoder.encode(edit.Insert.text);
      const result = new Uint8Array(bytes.length + insertBytes.length);
      result.set(bytes.slice(0, pos));
      result.set(insertBytes, pos);
      result.set(bytes.slice(pos), pos + insertBytes.length);
      bytes = result;
    } else if ("Delete" in edit) {
      const start = Math.min(edit.Delete.pos, bytes.length);
      const end = Math.min(start + edit.Delete.len, bytes.length);
      if (start < end) {
        const result = new Uint8Array(bytes.length - (end - start));
        result.set(bytes.slice(0, start));
        result.set(bytes.slice(end), start);
        bytes = result;
      }
    } else if ("Replace" in edit) {
      const start = Math.min(edit.Replace.pos, bytes.length);
      const end = Math.min(start + edit.Replace.old_len, bytes.length);
      const newTextBytes = encoder.encode(edit.Replace.new_text);
      const result = new Uint8Array(
        bytes.length - (end - start) + newTextBytes.length
      );
      result.set(bytes.slice(0, start));
      result.set(newTextBytes, start);
      result.set(bytes.slice(end), start + newTextBytes.length);
      bytes = result;
    }
  }

  return decoder.decode(bytes);
}
