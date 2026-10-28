use serde::Serialize;
use std::time::Instant;

/// Bytes per second, summed over the given volumes.
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Rates {
    pub read: f64,
    pub write: f64,
}

/// Live disk throughput. Call `poll` on an interval; the first call primes
/// the counters and returns zero rates. `None` means the platform does not
/// expose counters (e.g. Android's SELinux blocks /proc/diskstats).
pub struct Throughput {
    inner: platform::Counters,
}

impl Throughput {
    pub fn new() -> Self {
        Throughput { inner: platform::Counters::new() }
    }

    /// `knames` are `Volume::kname` values; Windows reports the total over
    /// all physical disks and ignores them.
    pub fn poll(&mut self, knames: &[String]) -> Option<Rates> {
        self.inner.poll(knames)
    }
}

impl Default for Throughput {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::collections::HashMap;

    type Sample = HashMap<String, (u64, u64)>;

    pub struct Counters {
        previous: Option<(Sample, Instant)>,
    }

    impl Counters {
        pub fn new() -> Self {
            Counters { previous: None }
        }

        pub fn poll(&mut self, knames: &[String]) -> Option<Rates> {
            let text = std::fs::read_to_string("/proc/diskstats").ok()?;
            let now = Instant::now();
            let current = parse(&text);
            let rates = match &self.previous {
                Some((before, at)) => rates(before, &current, now.duration_since(*at).as_secs_f64(), knames),
                None => Rates::default(),
            };
            self.previous = Some((current, now));
            Some(rates)
        }
    }

    /// `/proc/diskstats`: field 2 is the kernel name, 5 and 9 are sectors
    /// read and written (always 512-byte units).
    pub(super) fn parse(text: &str) -> Sample {
        text.lines()
            .filter_map(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() < 10 {
                    return None;
                }
                let sectors = |i: usize| fields[i].parse::<u64>().unwrap_or(0) * 512;
                Some((fields[2].to_string(), (sectors(5), sectors(9))))
            })
            .collect()
    }

    pub(super) fn rates(before: &Sample, after: &Sample, seconds: f64, knames: &[String]) -> Rates {
        let mut total = Rates::default();
        if seconds <= 0.0 {
            return total;
        }
        for kname in knames {
            if let (Some(b), Some(a)) = (before.get(kname), after.get(kname)) {
                total.read += a.0.saturating_sub(b.0) as f64 / seconds;
                total.write += a.1.saturating_sub(b.1) as f64 / seconds;
            }
        }
        total
    }
}

#[cfg(windows)]
mod platform {
    //! PDH rate counters: Windows computes bytes/sec between two collections.

    use super::*;
    use windows::Win32::System::Performance::*;
    use windows::core::w;

    pub struct Counters {
        query: PDH_HQUERY,
        read: PDH_HCOUNTER,
        write: PDH_HCOUNTER,
        ok: bool,
    }

    // PDH handles are process-wide and safe to use from any thread; the app
    // keeps this behind a Mutex.
    unsafe impl Send for Counters {}

    impl Counters {
        pub fn new() -> Self {
            let mut counters = Counters { query: PDH_HQUERY::default(), read: PDH_HCOUNTER::default(), write: PDH_HCOUNTER::default(), ok: false };
            unsafe {
                counters.ok = PdhOpenQueryW(None, 0, &mut counters.query) == 0
                    && PdhAddEnglishCounterW(counters.query, w!(r"\PhysicalDisk(_Total)\Disk Read Bytes/sec"), 0, &mut counters.read) == 0
                    && PdhAddEnglishCounterW(counters.query, w!(r"\PhysicalDisk(_Total)\Disk Write Bytes/sec"), 0, &mut counters.write) == 0
                    && PdhCollectQueryData(counters.query) == 0;
            }
            counters
        }

        pub fn poll(&mut self, _knames: &[String]) -> Option<Rates> {
            if !self.ok || unsafe { PdhCollectQueryData(self.query) } != 0 {
                return None;
            }
            let value = |counter: PDH_HCOUNTER| unsafe {
                let mut out = PDH_FMT_COUNTERVALUE::default();
                if PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, None, &mut out) == 0 { out.Anonymous.doubleValue.max(0.0) } else { 0.0 }
            };
            Some(Rates { read: value(self.read), write: value(self.write) })
        }
    }

    impl Drop for Counters {
        fn drop(&mut self) {
            if !self.query.is_invalid() {
                unsafe { PdhCloseQuery(self.query) };
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::platform::{parse, rates};

    #[test]
    fn rates_between_samples() {
        let line = |read: u64, written: u64| format!("   8 0 sda 1 0 {read} 0 1 0 {written} 0 0 0 0");
        let before = parse(&line(0, 0));
        let after = parse(&line(2048, 4096));
        let r = rates(&before, &after, 2.0, &["sda".into()]);
        assert_eq!(r.read, 2048.0 * 512.0 / 2.0);
        assert_eq!(r.write, 4096.0 * 512.0 / 2.0);
        assert_eq!(rates(&before, &after, 2.0, &["sdb".into()]).read, 0.0);
    }
}
