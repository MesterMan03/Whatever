import { _send, nextReqId } from "../shared.ts";

export type AudioMetadata = {
  duration_ms: number | null;
  sample_rate: number;
  channels: number;
};

export type CloseStrategy = "Auto" | "Manual";

type Cb<T> = { resolve: (v: T) => void; reject: (e: Error) => void };

/** Maps request_id → pending AudioLoad promise resolver. */
export const _audioLoadCallbacks = new Map<string, Cb<AudioHandle>>();
/** Maps request_id → pending AudioState promise resolver. */
export const _audioStateCallbacks = new Map<string, Cb<_AudioStatePayload>>();
/** Maps audio_id → live AudioHandle. */
export const _audioHandles = new Map<string, AudioHandle>();

export type _AudioStatePayload = {
  position_ms: number;
  volume: number;
  speed: number;
  is_playing: boolean;
  is_looping: boolean;
};

export class AudioHandle {
  readonly audio_id: string;
  private _isStopped = false;
  private _metadata: AudioMetadata | null = null;
  private _closeHandlers: Set<() => void> = new Set();

  constructor(audio_id: string, metadata: AudioMetadata) {
    this.audio_id = audio_id;
    this._metadata = metadata;
    _audioHandles.set(audio_id, this);
  }

  isStopped(): boolean {
    return this._isStopped;
  }

  on(event: "close", fn: () => void): void {
    this._closeHandlers.add(fn);
  }

  _handleClose(): void {
    if (this._isStopped) return;
    this._isStopped = true;
    _audioHandles.delete(this.audio_id);
    for (const fn of this._closeHandlers) fn();
  }

  stop(): void {
    if (this._isStopped) return;
    this._isStopped = true;
    _audioHandles.delete(this.audio_id);
    _send({ type: "AudioStop", audio_id: this.audio_id });
    for (const fn of this._closeHandlers) fn();
  }

  loop(enabled: boolean): void {
    if (this._isStopped) throw new Error("AudioHandle is stopped");
    _send({ type: "AudioSetLoop", audio_id: this.audio_id, loop_: enabled });
  }

  async play(opts?: { volume?: number; speed?: number }): Promise<number> {
    if (this._isStopped) throw new Error("AudioHandle is stopped");
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _audioStateCallbacks.set(request_id, {
        resolve: (s) => resolve(s.position_ms),
        reject,
      });
      _send({
        type: "AudioPlay",
        request_id,
        audio_id: this.audio_id,
        volume: opts?.volume,
        speed: opts?.speed,
      });
    });
  }

  async pause(): Promise<number> {
    if (this._isStopped) throw new Error("AudioHandle is stopped");
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _audioStateCallbacks.set(request_id, {
        resolve: (s) => resolve(s.position_ms),
        reject,
      });
      _send({ type: "AudioPause", request_id, audio_id: this.audio_id });
    });
  }

  async position(): Promise<number> {
    return (await this._query()).position_ms;
  }

  async isPlaying(): Promise<boolean> {
    return (await this._query()).is_playing;
  }

  async volume(): Promise<number> {
    return (await this._query()).volume;
  }

  async speed(): Promise<number> {
    return (await this._query()).speed;
  }

  async isLooping(): Promise<boolean> {
    return (await this._query()).is_looping;
  }

  async metadata(): Promise<AudioMetadata> {
    if (this._metadata) return this._metadata;
    const state = await this._query();
    // metadata fields are static — we can cache once received
    // (already set in constructor from AudioLoaded, but fallback path)
    return this._metadata!;
  }

  /** Seek to an absolute position in ms. Returns the previous position. */
  async seekTo(ms: number): Promise<number> {
    if (this._isStopped) throw new Error("AudioHandle is stopped");
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _audioStateCallbacks.set(request_id, {
        resolve: (s) => resolve(s.position_ms),
        reject,
      });
      _send({ type: "AudioSeekTo", request_id, audio_id: this.audio_id, position_ms: ms });
    });
  }

  /** Seek forward (positive) or backward (negative) by ms. Returns the new position. */
  async seek(offsetMs: number): Promise<number> {
    if (this._isStopped) throw new Error("AudioHandle is stopped");
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _audioStateCallbacks.set(request_id, {
        resolve: (s) => resolve(s.position_ms),
        reject,
      });
      _send({ type: "AudioSeek", request_id, audio_id: this.audio_id, offset_ms: offsetMs });
    });
  }

  private _query(): Promise<_AudioStatePayload> {
    if (this._isStopped) return Promise.reject(new Error("AudioHandle is stopped"));
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _audioStateCallbacks.set(request_id, { resolve, reject });
      _send({ type: "AudioQuery", request_id, audio_id: this.audio_id });
    });
  }
}

export const Audio = {
  /**
   * Fire-and-forget playback. The engine loads and plays the audio automatically,
   * freeing the handle when playback ends. No AudioHandle is returned.
   */
  play(opts: { path: string; seek?: number; speed?: number; volume?: number }): void {
    const audio_id = nextReqId();
    _send({
      type: "AudioLoad",
      request_id: audio_id,
      audio_id,
      path: opts.path,
      play: true,
      volume: opts.volume ?? 1.0,
      speed: opts.speed ?? 1.0,
      loop_: false,
      close_strategy: "Auto",
    });
    // No callback registered — AudioLoaded response is silently ignored.
  },

  /**
   * Load an audio file and return a controllable handle.
   * @param opts.play - Whether to start playback immediately (default: false).
   * @param opts.closeStrategy - "Auto" frees the handle when playback ends (default); "Manual" requires stop().
   */
  load(opts: {
    path: string;
    play?: boolean;
    closeStrategy?: CloseStrategy;
    volume?: number;
    speed?: number;
    loop?: boolean;
  }): Promise<AudioHandle> {
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      const audio_id = request_id;
      _audioLoadCallbacks.set(request_id, { resolve, reject });
      _send({
        type: "AudioLoad",
        request_id,
        audio_id,
        path: opts.path,
        play: opts.play ?? false,
        volume: opts.volume ?? 1.0,
        speed: opts.speed ?? 1.0,
        loop_: opts.loop ?? false,
        close_strategy: opts.closeStrategy ?? "Auto",
      });
    });
  },
};
