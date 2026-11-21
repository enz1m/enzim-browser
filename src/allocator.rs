pub(crate) fn configure_startup() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        use std::os::unix::process::CommandExt;
        #[cfg(feature = "benchmark")]
        if std::env::var_os("ENZIM_BENCH_SYSTEM_ALLOCATOR").is_some_and(|value| value == "1") {
            return;
        }
        let defaults = [
            ("MALLOC_ARENA_MAX", "2"),
            ("MALLOC_MMAP_THRESHOLD_", "131072"),
            #[cfg(feature = "wpe")]
            ("WEBKIT_DEFER_COMPLETE_IMAGE_FRAMES", "1"),
            #[cfg(feature = "wpe")]
            ("WEBKIT_RELEASE_SVG_RESOURCE_CACHES", "1"),
            #[cfg(feature = "wpe")]
            ("WEBKIT_TIGHT_BACKGROUND_HEAPS", "1"),
            #[cfg(feature = "wpe")]
            ("WPE_READY_SCRIPT_PAUSE", "1"),
            #[cfg(feature = "wpe")]
            ("WPE_PREPARE_FD_TABLE", "1"),
            #[cfg(feature = "wpe")]
            ("ENZIM_PREPARE_FD_TABLE", "1"),
            #[cfg(feature = "wpe")]
            ("JSC_smallHeapGrowthFactor", "1.75"),
            #[cfg(feature = "wpe")]
            ("JSC_largeHeapSize", "4194304"),
        ];
        let missing: Vec<_> = defaults
            .into_iter()
            .filter(|(key, _)| std::env::var_os(key).is_none())
            .collect();
        if missing.is_empty() {
            return;
        }
        let executable = match std::env::current_exe() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("Allocator startup defaults unavailable: {error}");
                return;
            }
        };
        // Re-exec seeds glibc before arena creation without mutating a threaded process's environment.
        let error = std::process::Command::new(executable)
            .args(std::env::args_os().skip(1))
            .envs(missing)
            .exec();
        eprintln!("Allocator startup defaults unavailable: {error}");
    }
}

#[cfg(all(test, target_os = "linux", target_env = "gnu"))]
mod tests;
