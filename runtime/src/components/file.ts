import { _fileCallbacks, _send, nextReqId } from "../shared.ts";

/** Sandboxed per-mod file I/O. Paths must not contain `..`. */
export const File = {
  /** Write a UTF-8 string to a sandboxed file for this mod. */
  write(path: string, data: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _fileCallbacks.set(request_id, { resolve: () => resolve(), reject });
      _send({ type: "FileWrite", request_id, path, data_base64: Buffer.from(data, "utf8").toString("base64") });
    });
  },

  /** Read a sandboxed file for this mod and return its contents as a UTF-8 string. */
  read(path: string): Promise<string> {
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _fileCallbacks.set(request_id, {
        resolve: (b64) => resolve(Buffer.from(b64!, "base64").toString("utf8")),
        reject,
      });
      _send({ type: "FileRead", request_id, path });
    });
  },

  /** Delete a sandboxed file for this mod. */
  delete(path: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _fileCallbacks.set(request_id, { resolve: () => resolve(), reject });
      _send({ type: "FileDelete", request_id, path });
    });
  },
};
