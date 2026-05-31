use std::collections::HashMap;
use std::io::Cursor;
use std::time::Duration;

use anyhow::anyhow;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseStrategy {
    Auto,
    Manual,
}

#[derive(Debug, Clone)]
pub struct AudioMetadata {
    pub duration_ms: Option<u64>,
    pub sample_rate: u32,
    pub channels: u16,
}

pub struct AudioState {
    pub position_ms: u64,
    pub volume: f32,
    pub speed: f32,
    pub is_playing: bool,
    pub is_looping: bool,
}

struct AudioHandle {
    sink: Sink,
    metadata: AudioMetadata,
    data: Vec<u8>,
    close_strategy: CloseStrategy,
    mod_id: String,
    volume: f32,
    speed: f32,
    is_looping: bool,
}

pub struct LoadOpts {
    pub play: bool,
    pub volume: f32,
    pub speed: f32,
    pub loop_: bool,
    pub close_strategy: CloseStrategy,
}

pub struct AudioManager {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    handles: HashMap<String, AudioHandle>,
}

impl AudioManager {
    pub fn new() -> anyhow::Result<Self> {
        let (_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| anyhow!("audio output init failed: {e}"))?;
        Ok(AudioManager {
            _stream,
            stream_handle,
            handles: HashMap::new(),
        })
    }

    pub fn load(
        &mut self,
        audio_id: String,
        mod_id: String,
        data: Vec<u8>,
        opts: LoadOpts,
    ) -> anyhow::Result<AudioMetadata> {
        let source = Decoder::new(Cursor::new(data.clone()))
            .map_err(|e| anyhow!("decode error: {e}"))?;

        let channels = source.channels();
        let sample_rate = source.sample_rate();
        let duration_ms = source.total_duration().map(|d| d.as_millis() as u64);
        let metadata = AudioMetadata { duration_ms, sample_rate, channels };

        let sink = Sink::try_new(&self.stream_handle)
            .map_err(|e| anyhow!("sink create failed: {e}"))?;
        sink.set_volume(opts.volume);
        sink.set_speed(opts.speed);

        if opts.loop_ {
            let src = Decoder::new(Cursor::new(data.clone()))
                .map_err(|e| anyhow!("decode error: {e}"))?;
            sink.append(src.repeat_infinite());
        } else {
            let src = Decoder::new(Cursor::new(data.clone()))
                .map_err(|e| anyhow!("decode error: {e}"))?;
            sink.append(src);
        }

        if !opts.play {
            sink.pause();
        }

        self.handles.insert(
            audio_id,
            AudioHandle {
                sink,
                metadata: metadata.clone(),
                data,
                close_strategy: opts.close_strategy,
                mod_id,
                volume: opts.volume,
                speed: opts.speed,
                is_looping: opts.loop_,
            },
        );

        Ok(metadata)
    }

    pub fn play(
        &mut self,
        audio_id: &str,
        volume: Option<f32>,
        speed: Option<f32>,
    ) -> anyhow::Result<u64> {
        let handle = self.handles.get_mut(audio_id).ok_or_else(|| anyhow!("handle not found"))?;
        if let Some(v) = volume {
            handle.sink.set_volume(v);
            handle.volume = v;
        }
        if let Some(s) = speed {
            handle.sink.set_speed(s);
            handle.speed = s;
        }
        handle.sink.play();
        Ok(handle.sink.get_pos().as_millis() as u64)
    }

    pub fn pause(&mut self, audio_id: &str) -> anyhow::Result<u64> {
        let handle = self.handles.get_mut(audio_id).ok_or_else(|| anyhow!("handle not found"))?;
        handle.sink.pause();
        Ok(handle.sink.get_pos().as_millis() as u64)
    }

    pub fn stop(&mut self, audio_id: &str) {
        if let Some(handle) = self.handles.remove(audio_id) {
            handle.sink.stop();
        }
    }

    pub fn seek_to(&mut self, audio_id: &str, position_ms: u64) -> anyhow::Result<u64> {
        let handle = self.handles.get_mut(audio_id).ok_or_else(|| anyhow!("handle not found"))?;
        let prev = handle.sink.get_pos().as_millis() as u64;
        handle.sink.try_seek(Duration::from_millis(position_ms))
            .map_err(|e| anyhow!("seek failed: {e}"))?;
        Ok(prev)
    }

    pub fn seek(&mut self, audio_id: &str, offset_ms: i64) -> anyhow::Result<u64> {
        let handle = self.handles.get_mut(audio_id).ok_or_else(|| anyhow!("handle not found"))?;
        let current = handle.sink.get_pos().as_millis() as i64;
        let new_pos = (current + offset_ms).max(0);

        let clamped = if let Some(dur_ms) = handle.metadata.duration_ms {
            if new_pos >= dur_ms as i64 {
                let last = dur_ms.saturating_sub(1);
                handle.sink.try_seek(Duration::from_millis(last))
                    .map_err(|e| anyhow!("seek failed: {e}"))?;
                handle.sink.pause();
                last
            } else {
                handle.sink.try_seek(Duration::from_millis(new_pos as u64))
                    .map_err(|e| anyhow!("seek failed: {e}"))?;
                new_pos as u64
            }
        } else {
            handle.sink.try_seek(Duration::from_millis(new_pos as u64))
                .map_err(|e| anyhow!("seek failed: {e}"))?;
            new_pos as u64
        };

        Ok(clamped)
    }

    pub fn set_loop(&mut self, audio_id: &str, loop_: bool) -> anyhow::Result<()> {
        let handle = self.handles.get_mut(audio_id).ok_or_else(|| anyhow!("handle not found"))?;
        if handle.is_looping == loop_ {
            return Ok(());
        }
        let was_playing = !handle.sink.is_paused();
        handle.sink.clear();

        if loop_ {
            let src = Decoder::new(Cursor::new(handle.data.clone()))
                .map_err(|e| anyhow!("decode error on loop change: {e}"))?;
            handle.sink.append(src.repeat_infinite());
        } else {
            let src = Decoder::new(Cursor::new(handle.data.clone()))
                .map_err(|e| anyhow!("decode error on loop change: {e}"))?;
            handle.sink.append(src);
        }

        if was_playing {
            handle.sink.play();
        } else {
            handle.sink.pause();
        }

        handle.is_looping = loop_;
        Ok(())
    }

    pub fn query(&self, audio_id: &str) -> anyhow::Result<AudioState> {
        let handle = self.handles.get(audio_id).ok_or_else(|| anyhow!("handle not found"))?;
        Ok(AudioState {
            position_ms: handle.sink.get_pos().as_millis() as u64,
            volume: handle.volume,
            speed: handle.speed,
            is_playing: !handle.sink.is_paused() && !handle.sink.empty(),
            is_looping: handle.is_looping,
        })
    }

    /// Returns `(mod_id, audio_id)` pairs for auto-close handles that have finished playing.
    pub fn poll_finished(&mut self) -> Vec<(String, String)> {
        let finished: Vec<String> = self
            .handles
            .iter()
            .filter(|(_, h)| h.sink.empty() && h.close_strategy == CloseStrategy::Auto)
            .map(|(id, _)| id.clone())
            .collect();

        finished
            .into_iter()
            .map(|id| {
                let h = self.handles.remove(&id).unwrap();
                (h.mod_id, id)
            })
            .collect()
    }
}
