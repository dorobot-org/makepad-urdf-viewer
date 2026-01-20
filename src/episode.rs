//! Episode Data Loader
//!
//! Loads robot episode data from LeRobot-format parquet files.
//! Converts joint angles from degrees (dataset format) to radians (URDF format).

use std::path::Path;
use std::fs::File;
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::{RowAccessor, ListAccessor};

/// A single frame of episode data containing joint positions
#[derive(Clone, Debug)]
pub struct EpisodeFrame {
    /// Joint angles in radians (converted from degrees in the dataset)
    pub joint_angles: Vec<f32>,
    /// Frame index within the episode
    pub frame_index: u64,
    /// Timestamp if available
    pub timestamp: Option<f64>,
}

/// Episode data loaded from a parquet file
#[derive(Clone, Debug)]
pub struct Episode {
    /// All frames in the episode
    pub frames: Vec<EpisodeFrame>,
    /// Episode index (from filename)
    pub episode_index: u32,
    /// Dataset FPS (default 30)
    pub fps: f64,
}

impl Episode {
    /// Load an episode from a parquet file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, EpisodeError> {
        let path = path.as_ref();
        let file = File::open(path)
            .map_err(|e| EpisodeError::IoError(e.to_string()))?;

        let reader = SerializedFileReader::new(file)
            .map_err(|e| EpisodeError::ParquetError(e.to_string()))?;

        let mut frames = Vec::new();

        // Iterate through row groups and rows
        let iter = reader.get_row_iter(None)
            .map_err(|e| EpisodeError::ParquetError(e.to_string()))?;

        for row_result in iter {
            let row = row_result
                .map_err(|e| EpisodeError::ParquetError(e.to_string()))?;

            // Extract observation.state - expecting a list of 6 floats
            let state_list = row.get_list(1) // observation.state is typically column 1
                .map_err(|e| EpisodeError::ParseError(format!("Failed to get observation.state: {}", e)))?;

            let mut joint_angles_degrees = Vec::new();
            for i in 0..state_list.len() {
                let val = state_list.get_float(i)
                    .or_else(|_| state_list.get_double(i).map(|d| d as f32))
                    .map_err(|e| EpisodeError::ParseError(format!("Failed to parse state[{}]: {}", i, e)))?;
                joint_angles_degrees.push(val);
            }

            // Convert degrees to radians
            // Negate joints 2 and 3 (indices 1,2) to match URDF direction conventions
            let joint_angles: Vec<f32> = joint_angles_degrees
                .iter()
                .enumerate()
                .map(|(idx, deg)| {
                    let rad = deg * std::f32::consts::PI / 180.0;
                    if idx == 1 || idx == 2 {
                        -rad
                    } else {
                        rad
                    }
                })
                .collect();

            // Get frame index (column 3 in schema)
            let frame_index = row.get_long(3) // frame_index column
                .unwrap_or(frames.len() as i64) as u64;

            // Get timestamp if available (column 2 - it's a float, not double)
            let timestamp = row.get_float(2).map(|f| f as f64).ok();

            frames.push(EpisodeFrame {
                joint_angles,
                frame_index,
                timestamp,
            });
        }

        // Extract episode index from filename (e.g., "episode_000000.parquet" -> 0)
        let episode_index = path.file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("episode_"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        Ok(Episode {
            frames,
            episode_index,
            fps: 30.0, // Default LeRobot FPS
        })
    }

    /// Get frame at a specific index
    pub fn get_frame(&self, index: usize) -> Option<&EpisodeFrame> {
        self.frames.get(index)
    }

    /// Get total number of frames
    pub fn num_frames(&self) -> usize {
        self.frames.len()
    }

    /// Get duration in seconds
    pub fn duration(&self) -> f64 {
        self.frames.len() as f64 / self.fps
    }

    /// Get frame at a specific time in seconds
    pub fn get_frame_at_time(&self, time_secs: f64) -> Option<&EpisodeFrame> {
        let frame_idx = (time_secs * self.fps) as usize;
        self.get_frame(frame_idx)
    }
}

/// Error type for episode loading
#[derive(Debug)]
pub enum EpisodeError {
    IoError(String),
    ParquetError(String),
    ParseError(String),
}

impl std::fmt::Display for EpisodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EpisodeError::IoError(msg) => write!(f, "IO error: {}", msg),
            EpisodeError::ParquetError(msg) => write!(f, "Parquet error: {}", msg),
            EpisodeError::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for EpisodeError {}

/// Episode player for animating through episode frames
pub struct EpisodePlayer {
    episode: Episode,
    current_frame: usize,
    playing: bool,
    loop_playback: bool,
    playback_speed: f64,
}

impl EpisodePlayer {
    pub fn new(episode: Episode) -> Self {
        EpisodePlayer {
            episode,
            current_frame: 0,
            playing: false,
            loop_playback: true,
            playback_speed: 1.0,
        }
    }

    /// Start/resume playback
    pub fn play(&mut self) {
        self.playing = true;
    }

    /// Pause playback
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Toggle play/pause
    pub fn toggle(&mut self) -> bool {
        self.playing = !self.playing;
        self.playing
    }

    /// Reset to beginning
    pub fn reset(&mut self) {
        self.current_frame = 0;
    }

    /// Advance by one frame
    pub fn advance(&mut self) -> Option<&EpisodeFrame> {
        if !self.playing {
            return self.current_frame();
        }

        self.current_frame += 1;

        if self.current_frame >= self.episode.num_frames() {
            if self.loop_playback {
                self.current_frame = 0;
            } else {
                self.current_frame = self.episode.num_frames() - 1;
                self.playing = false;
            }
        }

        self.current_frame()
    }

    /// Get current frame
    pub fn current_frame(&self) -> Option<&EpisodeFrame> {
        self.episode.get_frame(self.current_frame)
    }

    /// Get current frame index
    pub fn current_frame_index(&self) -> usize {
        self.current_frame
    }

    /// Seek to specific frame
    pub fn seek(&mut self, frame: usize) {
        self.current_frame = frame.min(self.episode.num_frames().saturating_sub(1));
    }

    /// Is playing?
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Set loop mode
    pub fn set_loop(&mut self, loop_playback: bool) {
        self.loop_playback = loop_playback;
    }

    /// Get episode reference
    pub fn episode(&self) -> &Episode {
        &self.episode
    }

    /// Get playback speed
    pub fn playback_speed(&self) -> f64 {
        self.playback_speed
    }

    /// Set playback speed
    pub fn set_playback_speed(&mut self, speed: f64) {
        self.playback_speed = speed.max(0.1).min(10.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_degrees_to_radians() {
        let deg = 180.0f32;
        let rad = deg * std::f32::consts::PI / 180.0;
        assert!((rad - std::f32::consts::PI).abs() < 0.001);
    }
}
