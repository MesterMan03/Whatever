// Small explicit platform services. No filesystem, processes, sockets or dynamic package loading.
(() => {
  class TextEncoder {
    encode(text = "") {
      const bytes = [];
      for (const character of String(text)) {
        let point = character.codePointAt(0);
        if (point >= 0xd800 && point <= 0xdfff) point = 0xfffd;
        if (point < 0x80) bytes.push(point);
        else if (point < 0x800) bytes.push(0xc0 | (point >> 6), 0x80 | (point & 63));
        else if (point < 0x10000)
          bytes.push(0xe0 | (point >> 12), 0x80 | ((point >> 6) & 63), 0x80 | (point & 63));
        else
          bytes.push(
            0xf0 | (point >> 18),
            0x80 | ((point >> 12) & 63),
            0x80 | ((point >> 6) & 63),
            0x80 | (point & 63),
          );
      }
      return new Uint8Array(bytes);
    }
  }
  class TextDecoder {
    constructor(_encoding = "utf-8", options = {}) {
      this.fatal = !!options.fatal;
    }
    decode(input = new Uint8Array()) {
      const bytes = input instanceof ArrayBuffer ? new Uint8Array(input) : input;
      let output = "";
      for (let i = 0; i < bytes.length; ) {
        const first = bytes[i++];
        if (first < 128) {
          output += String.fromCharCode(first);
          continue;
        }
        const count =
          first >= 0xc2 && first <= 0xdf
            ? 1
            : first >= 0xe0 && first <= 0xef
              ? 2
              : first >= 0xf0 && first <= 0xf4
                ? 3
                : -1;
        let point = count === 1 ? first & 31 : count === 2 ? first & 15 : first & 7,
          valid = count >= 0 && i + count <= bytes.length;
        for (let n = 0; valid && n < count; n++) {
          const b = bytes[i + n];
          if ((b & 192) !== 128) valid = false;
          else point = (point << 6) | (b & 63);
        }
        valid =
          valid &&
          point >= (count === 1 ? 128 : count === 2 ? 2048 : 65536) &&
          point <= 0x10ffff &&
          !(point >= 0xd800 && point <= 0xdfff);
        if (!valid) {
          if (this.fatal) throw Error("Invalid UTF-8");
          output += "\ufffd";
          continue;
        }
        i += count;
        output += String.fromCodePoint(point);
      }
      return output;
    }
  }
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  class Buffer extends Uint8Array {
    static from(value, encoding = "utf8") {
      if (typeof value !== "string") return new Buffer(value);
      if (encoding !== "base64") return new Buffer(new TextEncoder().encode(value));
      const bytes = [];
      let bits = 0,
        count = 0;
      for (const c of value) {
        const n = alphabet.indexOf(c);
        if (n < 0) continue;
        bits = (bits << 6) | n;
        count += 6;
        if (count >= 8) {
          count -= 8;
          bytes.push((bits >> count) & 255);
        }
      }
      return new Buffer(bytes);
    }
    static byteLength(value) {
      return new TextEncoder().encode(value).length;
    }
    toString(encoding = "utf8") {
      if (encoding !== "base64") return new TextDecoder().decode(this);
      let result = "";
      for (let i = 0; i < this.length; i += 3) {
        const n = (this[i] << 16) | ((this[i + 1] ?? 0) << 8) | (this[i + 2] ?? 0);
        result +=
          alphabet[(n >> 18) & 63] +
          alphabet[(n >> 12) & 63] +
          (i + 1 < this.length ? alphabet[(n >> 6) & 63] : "=") +
          (i + 2 < this.length ? alphabet[n & 63] : "=");
      }
      return result;
    }
  }
  globalThis.TextEncoder = TextEncoder;
  globalThis.TextDecoder = TextDecoder;
  globalThis.Buffer = Buffer;
  globalThis.queueMicrotask = (callback) => Promise.resolve().then(callback);
  const timers = new Map();
  let serial = 0,
    now = 0;
  globalThis.setTimeout = (callback, delay = 0, ...args) => {
    const id = ++serial;
    timers.set(id, { callback, args, at: now + Math.max(0, delay) });
    return id;
  };
  globalThis.clearTimeout = (id) => timers.delete(id);
  globalThis.setInterval = (callback, delay = 0, ...args) => {
    const id = ++serial;
    timers.set(id, { callback, args, at: now + Math.max(1, delay), interval: Math.max(1, delay) });
    return id;
  };
  globalThis.clearInterval = globalThis.clearTimeout;
  globalThis.__advanceTimers = (elapsed) => {
    now += elapsed;
    for (const [id, timer] of [...timers])
      if (timer.at <= now) {
        if (timer.interval) timer.at = now + timer.interval;
        else timers.delete(id);
        timer.callback(...timer.args);
      }
  };
})();
