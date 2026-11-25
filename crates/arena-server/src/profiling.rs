//! Opt-in continuous CPU profiling (ADR 0054, Phase 3).
//!
//! Built only with the `profiling` feature (the Docker image has it) and
//! started only when `ARENA_PYROSCOPE_URL` names a Pyroscope server, such as
//! `http://lgtm:4040` in the `obs` Compose profile. It samples every thread
//! at `ARENA_PYROSCOPE_HZ` (default 100; Pyroscope estimates about 1% CPU,
//! not measured here) so tick p99 spikes
//! can be read as flame graphs. Without the variable nothing starts. The
//! agent reports through the `log` crate, which `logging::init` bridges into
//! the process logs.

/// Samples per second when `ARENA_PYROSCOPE_HZ` is unset.
pub const DEFAULT_HZ: u32 = 100;
/// The longest [`Profiler::stop`] waits for the last upload: the agent
/// stops at the end of its 10 s reporting cycle, then uploads.
pub const STOP_WAIT: std::time::Duration = std::time::Duration::from_secs(11);

/// The sampling rate named by `ARENA_PYROSCOPE_HZ`: 1 to 1000 per second.
pub fn sample_rate(value: Option<&str>) -> Result<u32, String> {
    match value.map(str::trim) {
        None | Some("") => Ok(DEFAULT_HZ),
        Some(text) => text
            .parse::<u32>()
            .ok()
            .filter(|hz| (1..=1000).contains(hz))
            .ok_or_else(|| format!("ARENA_PYROSCOPE_HZ must be 1 to 1000, not {text:?}")),
    }
}

/// A running profiler, or nothing. Stop it before exiting so the last
/// profile is sent.
pub struct Profiler {
    #[cfg(feature = "profiling")]
    agent: Option<pyroscope::PyroscopeAgent<pyroscope::pyroscope::PyroscopeAgentRunning>>,
}

impl Profiler {
    /// Whether profiles are being sent.
    pub fn running(&self) -> bool {
        #[cfg(feature = "profiling")]
        return self.agent.is_some();
        #[cfg(not(feature = "profiling"))]
        false
    }

    /// Sends the last profile and stops, waiting at most [`STOP_WAIT`]: an
    /// unreachable Pyroscope must not hold up the process past its stop
    /// grace period. Call it outside any async runtime.
    pub fn stop(self) {
        #[cfg(feature = "profiling")]
        if let Some(agent) = self.agent {
            let (done, finished) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                match agent.stop() {
                    Ok(ready) => ready.shutdown(),
                    Err(error) => {
                        tracing::warn!(reason = %error, "profiler did not stop cleanly");
                    }
                }
                let _ = done.send(());
            });
            if finished.recv_timeout(STOP_WAIT).is_err() {
                tracing::warn!("profiler did not stop in time; exiting without its last upload");
            }
        }
    }
}

/// Starts profiling `service` when `ARENA_PYROSCOPE_URL` is set. Call it
/// after logging is set up and before the async runtime starts.
pub fn start(service: &'static str) -> Profiler {
    let url = std::env::var("ARENA_PYROSCOPE_URL")
        .ok()
        .filter(|url| !url.trim().is_empty());
    let Some(url) = url else {
        return Profiler {
            #[cfg(feature = "profiling")]
            agent: None,
        };
    };
    let hz = match sample_rate(std::env::var("ARENA_PYROSCOPE_HZ").ok().as_deref()) {
        Ok(hz) => hz,
        Err(problem) => {
            tracing::warn!(reason = %problem, "using the default profiling rate");
            DEFAULT_HZ
        }
    };
    #[cfg(feature = "profiling")]
    {
        use pyroscope::backend::{BackendConfig, PprofConfig, pprof_backend};
        use pyroscope::pyroscope::PyroscopeAgentBuilder;
        let started = PyroscopeAgentBuilder::new(
            url.trim(),
            service,
            hz,
            "pyroscope-rs",
            "2.1.5",
            pprof_backend(PprofConfig { sample_rate: hz }, BackendConfig::default()),
        )
        .tags(vec![("service", service)])
        .build()
        .and_then(|agent| agent.start());
        match started {
            Ok(agent) => {
                tracing::info!(service, hz, "continuous CPU profiling on");
                Profiler { agent: Some(agent) }
            }
            Err(error) => {
                tracing::warn!(reason = %error, "continuous CPU profiling did not start");
                Profiler { agent: None }
            }
        }
    }
    #[cfg(not(feature = "profiling"))]
    {
        let _ = (url, hz);
        tracing::warn!(
            service,
            "ARENA_PYROSCOPE_URL is set, but this build has no profiling: build with --features profiling"
        );
        Profiler {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sampling_rate_defaults_to_100_and_is_bounded() {
        assert_eq!(sample_rate(None), Ok(100));
        assert_eq!(sample_rate(Some(" ")), Ok(100));
        assert_eq!(sample_rate(Some("250")), Ok(250));
        assert!(sample_rate(Some("0")).is_err());
        assert!(sample_rate(Some("1001")).is_err());
        assert!(sample_rate(Some("fast")).is_err());
    }
}
