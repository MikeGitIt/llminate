//! Progress bar module using indicatif for terminal progress display
//!
//! This module provides progress bar support matching JavaScript's terminalProgressBarEnabled setting.
//! It supports both determinate (percentage-based) and indeterminate (spinner) progress bars.

use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::borrow::Cow;
use std::time::Duration;

/// Progress bar state matching JavaScript implementation
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProgressDisplayState {
    /// Indeterminate progress (spinner) - no known completion percentage
    Indeterminate,
    /// Determinate progress - percentage known (0-100)
    Determinate,
    /// Paused state
    Paused,
    /// Error state
    Error,
}

/// Progress bar wrapper for terminal operations
pub struct TerminalProgress {
    bar: ProgressBar,
    state: ProgressDisplayState,
    enabled: bool,
}

impl TerminalProgress {
    /// Create a new indeterminate progress bar (spinner)
    pub fn new_spinner(message: impl Into<Cow<'static, str>>) -> Self {
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg}")
                .expect("valid template")
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
        );
        bar.set_message(message);
        bar.enable_steady_tick(Duration::from_millis(80));

        Self {
            bar,
            state: ProgressDisplayState::Indeterminate,
            enabled: true,
        }
    }

    /// Create a new determinate progress bar with known length
    pub fn new_progress(length: u64, message: impl Into<Cow<'static, str>>) -> Self {
        let bar = ProgressBar::new(length);
        bar.set_style(
            ProgressStyle::with_template(
                "{msg} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%)"
            )
            .expect("valid template")
            .progress_chars("█▓▒░")
        );
        bar.set_message(message);

        Self {
            bar,
            state: ProgressDisplayState::Determinate,
            enabled: true,
        }
    }

    /// Create a new progress bar with percentage display (0-100)
    pub fn new_percentage(message: impl Into<Cow<'static, str>>) -> Self {
        let bar = ProgressBar::new(100);
        bar.set_style(
            ProgressStyle::with_template(
                "{msg} [{bar:40.cyan/blue}] {percent}%"
            )
            .expect("valid template")
            .progress_chars("█▓▒░")
        );
        bar.set_message(message);

        Self {
            bar,
            state: ProgressDisplayState::Determinate,
            enabled: true,
        }
    }

    /// Create a progress bar that is disabled (does nothing)
    pub fn disabled() -> Self {
        let bar = ProgressBar::hidden();
        Self {
            bar,
            state: ProgressDisplayState::Indeterminate,
            enabled: false,
        }
    }

    /// Check if progress bar is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set the message displayed on the progress bar
    pub fn set_message(&self, message: impl Into<Cow<'static, str>>) {
        if self.enabled {
            self.bar.set_message(message);
        }
    }

    /// Set the current position (for determinate progress)
    pub fn set_position(&self, pos: u64) {
        if self.enabled {
            self.bar.set_position(pos);
        }
    }

    /// Set percentage (0-100) for percentage-based progress bars
    pub fn set_percentage(&self, percent: u32) {
        if self.enabled {
            self.bar.set_position(percent.min(100) as u64);
        }
    }

    /// Increment position by delta
    pub fn inc(&self, delta: u64) {
        if self.enabled {
            self.bar.inc(delta);
        }
    }

    /// Mark the progress bar as finished
    pub fn finish(&self) {
        if self.enabled {
            self.bar.finish();
        }
    }

    /// Mark the progress bar as finished with a message
    pub fn finish_with_message(&self, message: impl Into<Cow<'static, str>>) {
        if self.enabled {
            self.bar.finish_with_message(message);
        }
    }

    /// Mark the progress bar as finished and clear it from display
    pub fn finish_and_clear(&self) {
        if self.enabled {
            self.bar.finish_and_clear();
        }
    }

    /// Abandon the progress bar (mark as incomplete but stop updating)
    pub fn abandon(&self) {
        if self.enabled {
            self.bar.abandon();
        }
    }

    /// Abandon with message
    pub fn abandon_with_message(&self, message: impl Into<Cow<'static, str>>) {
        if self.enabled {
            self.bar.abandon_with_message(message);
        }
    }

    /// Suspend the progress bar to allow normal output
    pub fn suspend<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        if self.enabled {
            self.bar.suspend(f)
        } else {
            f()
        }
    }

    /// Print a message while keeping the progress bar visible
    pub fn println(&self, message: impl AsRef<str>) {
        if self.enabled {
            self.bar.println(message);
        } else {
            println!("{}", message.as_ref());
        }
    }

    /// Get the current position
    pub fn position(&self) -> u64 {
        self.bar.position()
    }

    /// Get the length
    pub fn length(&self) -> Option<u64> {
        self.bar.length()
    }

    /// Set the length (for determinate progress)
    pub fn set_length(&self, length: u64) {
        if self.enabled {
            self.bar.set_length(length);
        }
    }

    /// Check if progress is finished
    pub fn is_finished(&self) -> bool {
        self.bar.is_finished()
    }

    /// Reset the progress bar
    pub fn reset(&self) {
        if self.enabled {
            self.bar.reset();
        }
    }

    /// Set a custom style
    pub fn set_style(&self, style: ProgressStyle) {
        if self.enabled {
            self.bar.set_style(style);
        }
    }

    /// Convert to indeterminate (spinner) style
    pub fn set_indeterminate(&mut self) {
        if self.enabled && self.state != ProgressDisplayState::Indeterminate {
            self.bar.set_style(
                ProgressStyle::with_template("{spinner:.cyan} {msg}")
                    .expect("valid template")
                    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            );
            self.bar.enable_steady_tick(Duration::from_millis(80));
            self.state = ProgressDisplayState::Indeterminate;
        }
    }

    /// Convert to determinate (bar) style
    pub fn set_determinate(&mut self, length: u64) {
        if self.enabled {
            self.bar.set_length(length);
            self.bar.set_style(
                ProgressStyle::with_template(
                    "{msg} [{bar:40.cyan/blue}] {pos}/{len} ({percent}%)"
                )
                .expect("valid template")
                .progress_chars("█▓▒░")
            );
            self.bar.disable_steady_tick();
            self.state = ProgressDisplayState::Determinate;
        }
    }
}

/// Multi-progress bar manager for multiple concurrent operations
pub struct MultiProgressManager {
    multi: MultiProgress,
    enabled: bool,
}

impl MultiProgressManager {
    /// Create a new multi-progress manager
    pub fn new() -> Self {
        Self {
            multi: MultiProgress::new(),
            enabled: true,
        }
    }

    /// Create a disabled manager
    pub fn disabled() -> Self {
        Self {
            multi: MultiProgress::new(),
            enabled: false,
        }
    }

    /// Add a new spinner to the multi-progress display
    pub fn add_spinner(&self, message: impl Into<Cow<'static, str>>) -> TerminalProgress {
        if self.enabled {
            let bar = ProgressBar::new_spinner();
            bar.set_style(
                ProgressStyle::with_template("{spinner:.cyan} {msg}")
                    .expect("valid template")
                    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            );
            bar.set_message(message);
            bar.enable_steady_tick(Duration::from_millis(80));

            let bar = self.multi.add(bar);
            TerminalProgress {
                bar,
                state: ProgressDisplayState::Indeterminate,
                enabled: true,
            }
        } else {
            TerminalProgress::disabled()
        }
    }

    /// Add a new progress bar with known length
    pub fn add_progress(&self, length: u64, message: impl Into<Cow<'static, str>>) -> TerminalProgress {
        if self.enabled {
            let bar = ProgressBar::new(length);
            bar.set_style(
                ProgressStyle::with_template(
                    "{msg} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%)"
                )
                .expect("valid template")
                .progress_chars("█▓▒░")
            );
            bar.set_message(message);

            let bar = self.multi.add(bar);
            TerminalProgress {
                bar,
                state: ProgressDisplayState::Determinate,
                enabled: true,
            }
        } else {
            TerminalProgress::disabled()
        }
    }

    /// Add a percentage-based progress bar
    pub fn add_percentage(&self, message: impl Into<Cow<'static, str>>) -> TerminalProgress {
        if self.enabled {
            let bar = ProgressBar::new(100);
            bar.set_style(
                ProgressStyle::with_template(
                    "{msg} [{bar:40.cyan/blue}] {percent}%"
                )
                .expect("valid template")
                .progress_chars("█▓▒░")
            );
            bar.set_message(message);

            let bar = self.multi.add(bar);
            TerminalProgress {
                bar,
                state: ProgressDisplayState::Determinate,
                enabled: true,
            }
        } else {
            TerminalProgress::disabled()
        }
    }

    /// Clear all progress bars
    pub fn clear(&self) {
        if self.enabled {
            self.multi.clear().ok();
        }
    }

    /// Suspend all progress bars to allow normal output
    pub fn suspend<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        if self.enabled {
            self.multi.suspend(f)
        } else {
            f()
        }
    }
}

impl Default for MultiProgressManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if terminal progress bars are enabled from settings
pub fn terminal_progress_bar_enabled() -> bool {
    // Check settings file for terminalProgressBarEnabled
    if let Ok(config) = crate::config::load_config(crate::config::ConfigScope::User) {
        config.terminal_progress_bar_enabled.unwrap_or(true)
    } else {
        true // Default to enabled
    }
}

/// Create a progress bar respecting the terminalProgressBarEnabled setting
pub fn create_progress_spinner(message: impl Into<Cow<'static, str>>) -> TerminalProgress {
    if terminal_progress_bar_enabled() {
        TerminalProgress::new_spinner(message)
    } else {
        TerminalProgress::disabled()
    }
}

/// Create a determinate progress bar respecting settings
pub fn create_progress_bar(length: u64, message: impl Into<Cow<'static, str>>) -> TerminalProgress {
    if terminal_progress_bar_enabled() {
        TerminalProgress::new_progress(length, message)
    } else {
        TerminalProgress::disabled()
    }
}

/// Create a percentage progress bar respecting settings
pub fn create_percentage_bar(message: impl Into<Cow<'static, str>>) -> TerminalProgress {
    if terminal_progress_bar_enabled() {
        TerminalProgress::new_percentage(message)
    } else {
        TerminalProgress::disabled()
    }
}

/// Create a multi-progress manager respecting settings
pub fn create_multi_progress() -> MultiProgressManager {
    if terminal_progress_bar_enabled() {
        MultiProgressManager::new()
    } else {
        MultiProgressManager::disabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_creation() {
        let progress = TerminalProgress::new_spinner("Loading...");
        assert!(progress.is_enabled());
        progress.finish_and_clear();
    }

    #[test]
    fn test_progress_bar_creation() {
        let progress = TerminalProgress::new_progress(100, "Processing");
        assert!(progress.is_enabled());
        assert_eq!(progress.position(), 0);
        progress.set_position(50);
        assert_eq!(progress.position(), 50);
        progress.finish_and_clear();
    }

    #[test]
    fn test_percentage_bar() {
        let progress = TerminalProgress::new_percentage("Downloading");
        assert!(progress.is_enabled());
        progress.set_percentage(75);
        assert_eq!(progress.position(), 75);
        progress.finish_and_clear();
    }

    #[test]
    fn test_disabled_progress() {
        let progress = TerminalProgress::disabled();
        assert!(!progress.is_enabled());
        // These should not panic on disabled progress
        progress.set_message("test");
        progress.set_position(50);
        progress.finish();
    }

    #[test]
    fn test_multi_progress() {
        let multi = MultiProgressManager::new();
        let spinner = multi.add_spinner("Task 1");
        let bar = multi.add_progress(100, "Task 2");

        spinner.set_message("Task 1 running");
        bar.set_position(50);

        spinner.finish();
        bar.finish();
        multi.clear();
    }
}
