use std::collections::VecDeque;

pub(crate) const LOG_CAPACITY: usize = 256;
pub(crate) const LOG_LINE_BYTES: usize = 2048;

#[derive(Debug, Default)]
pub(crate) struct DiagnosticLog {
    lines: VecDeque<String>,
}

impl DiagnosticLog {
    pub(crate) fn push(&mut self, mut line: String) {
        if line.len() > LOG_LINE_BYTES {
            let mut end = LOG_LINE_BYTES - "...".len();
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            line.truncate(end);
            line.push_str("...");
        }
        if line.capacity() > LOG_LINE_BYTES {
            // Truncation alone retains the page-controlled allocation's capacity.
            line = line.into_boxed_str().into_string();
        }
        if self.lines.len() == LOG_CAPACITY {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &String> + ExactSizeIterator {
        self.lines.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_log_bounds_history_and_preserves_recent_order() {
        let mut log = DiagnosticLog::default();
        for index in 0..100_000 {
            log.push(format!("event {index}"));
        }
        let lines: Vec<_> = log.iter().map(String::as_str).collect();
        assert_eq!(lines.len(), 256);
        assert_eq!(lines[0], "event 99744");
        assert_eq!(lines[255], "event 99999");
    }

    #[test]
    fn diagnostic_log_releases_oversized_utf8_storage() {
        let mut log = DiagnosticLog::default();
        log.push("short diagnostic".to_owned());
        log.push("界".repeat(1_000_000));
        assert_eq!(log.iter().next().unwrap(), "short diagnostic");
        let line = log.iter().next_back().unwrap();
        assert!(line.ends_with("..."));
        assert_eq!(line.len(), 2046);
        assert!(
            line.capacity() <= 2048,
            "retained {} bytes",
            line.capacity()
        );
        let mut spare = String::with_capacity(1_000_000);
        spare.push_str("small");
        log.push(spare);
        assert_eq!(log.iter().next_back().unwrap().capacity(), 5);
    }
}
