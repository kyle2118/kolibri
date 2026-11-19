//! A deliberately simple load tester: N workers send the same request
//! (exactly as typed) either a fixed number of times or for a fixed duration.

use eframe::egui;
use crate::http::{new_agent, send_raw, Prepared};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Count,
    Duration,
}

#[derive(Clone)]
pub struct Config {
    pub mode: Mode,
    pub total: u64,
    pub seconds: u64,
    pub concurrency: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self { mode: Mode::Count, total: 100, seconds: 10, concurrency: 10 }
    }
}

#[derive(Default)]
struct Stats {
    latencies_us: Vec<u64>,
    statuses: BTreeMap<u16, u64>,
    errors: BTreeMap<String, u64>,
    bytes: u64,
}

struct Shared {
    stop: AtomicBool,
    issued: AtomicU64,
    live_workers: AtomicUsize,
    stats: Mutex<Stats>,
}

pub struct Run {
    pub config: Config,
    started: Instant,
    ended: Mutex<Option<Instant>>,
    shared: Arc<Shared>,
}

#[derive(Default, Clone)]
pub struct Summary {
    pub completed: u64,
    pub failed: u64,
    pub elapsed: Duration,
    pub rps: f64,
    pub bytes: u64,
    pub min: Duration,
    pub avg: Duration,
    pub p50: Duration,
    pub p90: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub max: Duration,
    pub statuses: Vec<(u16, u64)>,
    pub errors: Vec<(String, u64)>,
    pub running: bool,
    pub progress: f32,
}

impl Run {
    pub fn start(prepared: Prepared, config: Config, ctx: egui::Context) -> Self {
        let concurrency = config.concurrency.clamp(1, 1000);
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            issued: AtomicU64::new(0),
            live_workers: AtomicUsize::new(concurrency),
            stats: Mutex::new(Stats::default()),
        });
        let agent = new_agent(concurrency);
        let prepared = Arc::new(prepared);
        let started = Instant::now();
        let deadline = started + Duration::from_secs(config.seconds.max(1));
        for _ in 0..concurrency {
            let shared = shared.clone();
            let agent = agent.clone();
            let prepared = prepared.clone();
            let cfg = config.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                loop {
                    if shared.stop.load(Ordering::Relaxed) {
                        break;
                    }
                    match cfg.mode {
                        Mode::Count => {
                            if shared.issued.fetch_add(1, Ordering::Relaxed) >= cfg.total {
                                break;
                            }
                        }
                        Mode::Duration => {
                            if Instant::now() >= deadline {
                                break;
                            }
                            shared.issued.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    let t = Instant::now();
                    let r = send_raw(&agent, &prepared);
                    let us = t.elapsed().as_micros() as u64;
                    let mut s = shared.stats.lock().unwrap();
                    match r {
                        Ok((status, n)) => {
                            s.latencies_us.push(us);
                            *s.statuses.entry(status).or_default() += 1;
                            s.bytes += n as u64;
                        }
                        Err(e) => *s.errors.entry(e).or_default() += 1,
                    }
                }
                shared.live_workers.fetch_sub(1, Ordering::AcqRel);
                ctx.request_repaint();
            });
        }
        Self { config, started, ended: Mutex::new(None), shared }
    }

    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }

    pub fn is_running(&self) -> bool {
        self.shared.live_workers.load(Ordering::Acquire) > 0
    }

    pub fn summary(&self) -> Summary {
        let running = self.is_running();
        let elapsed = {
            let mut ended = self.ended.lock().unwrap();
            if !running && ended.is_none() {
                *ended = Some(Instant::now());
            }
            ended.unwrap_or_else(Instant::now) - self.started
        };
        let s = self.shared.stats.lock().unwrap();
        let mut lat = s.latencies_us.clone();
        let failed: u64 = s.errors.values().sum();
        let statuses: Vec<_> = s.statuses.iter().map(|(k, v)| (*k, *v)).collect();
        let errors: Vec<_> = s.errors.iter().map(|(k, v)| (k.clone(), *v)).collect();
        let bytes = s.bytes;
        drop(s);

        lat.sort_unstable();
        let completed = lat.len() as u64;
        let us = Duration::from_micros;
        let pct = |p: f64| -> Duration {
            if lat.is_empty() {
                return Duration::ZERO;
            }
            let i = ((p / 100.0) * (lat.len() as f64 - 1.0)).round() as usize;
            us(lat[i.min(lat.len() - 1)])
        };
        let avg = if lat.is_empty() { Duration::ZERO } else { us(lat.iter().sum::<u64>() / lat.len() as u64) };
        let done = completed + failed;
        let progress = match self.config.mode {
            Mode::Count => done as f32 / self.config.total.max(1) as f32,
            Mode::Duration => elapsed.as_secs_f32() / self.config.seconds.max(1) as f32,
        };
        Summary {
            completed,
            failed,
            elapsed,
            rps: if elapsed.as_secs_f64() > 0.0 { done as f64 / elapsed.as_secs_f64() } else { 0.0 },
            bytes,
            min: lat.first().map(|v| us(*v)).unwrap_or_default(),
            avg,
            p50: pct(50.0),
            p90: pct(90.0),
            p95: pct(95.0),
            p99: pct(99.0),
            max: lat.last().map(|v| us(*v)).unwrap_or_default(),
            statuses,
            errors,
            running,
            progress: if running { progress.min(1.0) } else { 1.0 },
        }
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        self.stop();
    }
}
