//! Performance profiling utilities
//!
//! This module provides profiling tools that are conditionally compiled
//! based on the `profiling` feature flag.
//!
//! Enable profiling with: `cargo run --features profiling`

use std::time::Instant;

/// Profiling statistics for performance measurement
#[derive(Default, Clone)]
pub struct ProfilingStats {
    frame_times: Vec<f64>,
    transform_times: Vec<f64>,
    draw_times: Vec<f64>,
    frame_count: u64,
    #[allow(dead_code)]
    max_samples: usize,
}

impl ProfilingStats {
    /// Create a new profiling stats tracker
    pub fn new(max_samples: usize) -> Self {
        Self {
            frame_times: Vec::with_capacity(max_samples),
            transform_times: Vec::with_capacity(max_samples),
            draw_times: Vec::with_capacity(max_samples),
            frame_count: 0,
            max_samples,
        }
    }

    /// Record a frame's timing data
    #[cfg(feature = "profiling")]
    pub fn record_frame(&mut self, frame_ms: f64, transform_ms: f64, draw_ms: f64) {
        self.frame_count += 1;

        if self.frame_times.len() >= self.max_samples {
            self.frame_times.remove(0);
            self.transform_times.remove(0);
            self.draw_times.remove(0);
        }

        self.frame_times.push(frame_ms);
        self.transform_times.push(transform_ms);
        self.draw_times.push(draw_ms);
    }

    /// Record a frame (no-op when profiling disabled)
    #[cfg(not(feature = "profiling"))]
    #[inline]
    pub fn record_frame(&mut self, _frame_ms: f64, _transform_ms: f64, _draw_ms: f64) {
        // No-op when profiling is disabled
    }

    /// Get average frame time
    pub fn avg_frame_time(&self) -> f64 {
        if self.frame_times.is_empty() {
            return 0.0;
        }
        self.frame_times.iter().sum::<f64>() / self.frame_times.len() as f64
    }

    /// Get average transform time
    pub fn avg_transform_time(&self) -> f64 {
        if self.transform_times.is_empty() {
            return 0.0;
        }
        self.transform_times.iter().sum::<f64>() / self.transform_times.len() as f64
    }

    /// Get average draw time
    pub fn avg_draw_time(&self) -> f64 {
        if self.draw_times.is_empty() {
            return 0.0;
        }
        self.draw_times.iter().sum::<f64>() / self.draw_times.len() as f64
    }

    /// Get current FPS
    pub fn fps(&self) -> f64 {
        let avg = self.avg_frame_time();
        if avg > 0.001 {
            1000.0 / avg
        } else {
            0.0
        }
    }

    /// Get frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Print stats to stderr (only when profiling enabled)
    #[cfg(feature = "profiling")]
    pub fn print_stats(&self) {
        // Print every 30 frames
        if self.frame_count % 30 == 0 && self.frame_count > 0 {
            eprintln!(
                "[Frame {:>5}] Avg: {:>6.2}ms total | {:>6.3}ms transform | {:>6.2}ms draw | {:>5.1} FPS",
                self.frame_count,
                self.avg_frame_time(),
                self.avg_transform_time(),
                self.avg_draw_time(),
                self.fps()
            );
        }
    }

    /// Print stats (no-op when profiling disabled)
    #[cfg(not(feature = "profiling"))]
    #[inline]
    pub fn print_stats(&self) {
        // No-op when profiling is disabled
    }
}

/// A scoped timer that records elapsed time when dropped
pub struct ScopedTimer {
    start: Instant,
    #[cfg(feature = "profiling")]
    name: &'static str,
}

impl ScopedTimer {
    /// Start a new scoped timer
    #[cfg(feature = "profiling")]
    pub fn new(name: &'static str) -> Self {
        Self {
            start: Instant::now(),
            name,
        }
    }

    /// Start a new scoped timer (no-op version)
    #[cfg(not(feature = "profiling"))]
    #[inline]
    pub fn new(_name: &'static str) -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }
}

#[cfg(feature = "profiling")]
impl Drop for ScopedTimer {
    fn drop(&mut self) {
        eprintln!("[Timer] {}: {:.3}ms", self.name, self.elapsed_ms());
    }
}

/// Macro for conditional profiling output
#[macro_export]
macro_rules! profile_log {
    ($($arg:tt)*) => {
        #[cfg(feature = "profiling")]
        eprintln!($($arg)*);
    };
}

/// Macro for timing a block of code
#[macro_export]
macro_rules! profile_scope {
    ($name:expr) => {
        #[cfg(feature = "profiling")]
        let _timer = $crate::profiling::ScopedTimer::new($name);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profiling_stats() {
        let mut stats = ProfilingStats::new(10);

        for i in 0..5 {
            stats.record_frame(16.0 + i as f64, 0.1, 0.5);
        }

        #[cfg(feature = "profiling")]
        {
            assert_eq!(stats.frame_count(), 5);
            assert!(stats.avg_frame_time() > 16.0);
        }
    }

    #[test]
    fn test_scoped_timer() {
        let timer = ScopedTimer::new("test");
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(timer.elapsed_ms() >= 10.0);
    }
}
