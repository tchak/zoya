function encodeUTF8(s: string): Uint8Array {
  const bytes: number[] = [];
  for (let i = 0; i < s.length; i++) {
    let c = s.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
      const lo = s.charCodeAt(i + 1);
      if (lo >= 0xdc00 && lo <= 0xdfff) {
        c = ((c - 0xd800) << 10) + (lo - 0xdc00) + 0x10000;
        i++;
      }
    }
    if (c < 0x80) {
      bytes.push(c);
    } else if (c < 0x800) {
      bytes.push(0xc0 | (c >> 6), 0x80 | (c & 0x3f));
    } else if (c < 0x10000) {
      bytes.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f));
    } else {
      bytes.push(
        0xf0 | (c >> 18),
        0x80 | ((c >> 12) & 0x3f),
        0x80 | ((c >> 6) & 0x3f),
        0x80 | (c & 0x3f),
      );
    }
  }
  return new Uint8Array(bytes);
}

function decodeUTF8(b: Uint8Array): string {
  let s = '';
  let i = 0;
  while (i < b.length) {
    const byte = b[i];
    let cp: number;
    if (byte < 0x80) {
      cp = byte;
      i++;
    } else if ((byte & 0xe0) === 0xc0) {
      cp = ((byte & 0x1f) << 6) | (b[i + 1] & 0x3f);
      i += 2;
    } else if ((byte & 0xf0) === 0xe0) {
      cp = ((byte & 0x0f) << 12) | ((b[i + 1] & 0x3f) << 6) | (b[i + 2] & 0x3f);
      i += 3;
    } else {
      cp =
        ((byte & 0x07) << 18) |
        ((b[i + 1] & 0x3f) << 12) |
        ((b[i + 2] & 0x3f) << 6) |
        (b[i + 3] & 0x3f);
      i += 4;
    }
    if (cp < 0x10000) {
      s += String.fromCharCode(cp);
    } else {
      cp -= 0x10000;
      s += String.fromCharCode(0xd800 + (cp >> 10), 0xdc00 + (cp & 0x3ff));
    }
  }
  return s;
}

export const $$Bytes = {
  len(b: Uint8Array): number {
    return b.length;
  },
  get(b: Uint8Array, index: number): { $tag: string; $0?: number } {
    if (index < 0 || index >= b.length) return { $tag: 'None' };
    return { $tag: 'Some', $0: b[index] };
  },
  slice(b: Uint8Array, start: number, end: number): Uint8Array {
    return b.slice(start, end);
  },
  concat(a: Uint8Array, b: Uint8Array): Uint8Array {
    const result = new Uint8Array(a.length + b.length);
    result.set(a);
    result.set(b, a.length);
    return result;
  },
  to_list(b: Uint8Array): number[] {
    return Array.from(b);
  },
  from_list(list: number[]): Uint8Array {
    return new Uint8Array(list);
  },
  to_string(b: Uint8Array): string {
    return decodeUTF8(b);
  },
  from_string(s: string): Uint8Array {
    return encodeUTF8(s);
  },
};
