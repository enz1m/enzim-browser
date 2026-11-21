use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const VARIABLES: [&str; 11] = [
    "MALLOC_ARENA_MAX",
    "MALLOC_MMAP_THRESHOLD_",
    "GLIBC_TUNABLES",
    "WEBKIT_DEFER_COMPLETE_IMAGE_FRAMES",
    "WEBKIT_RELEASE_SVG_RESOURCE_CACHES",
    "WEBKIT_TIGHT_BACKGROUND_HEAPS",
    "JSC_smallHeapGrowthFactor",
    "JSC_largeHeapSize",
    "WPE_READY_SCRIPT_PAUSE",
    "WPE_PREPARE_FD_TABLE",
    "ENZIM_PREPARE_FD_TABLE",
];

fn report_child() -> bool {
    if std::env::var_os("ENZIM_STARTUP_TEST_CHILD").is_none() {
        return false;
    }
    super::configure_startup();
    let values: serde_json::Map<String, Value> = VARIABLES
        .iter()
        .map(|key| ((*key).to_owned(), json!(std::env::var(key).ok())))
        .collect();
    writeln!(
        std::io::stdout().lock(),
        "\nSTARTUP_MEMORY {}",
        json!({
            "pid": std::process::id(),
            "args": std::env::args().skip(1).collect::<Vec<_>>(),
            "environment": values,
        })
    )
    .expect("write startup child report");
    true
}

fn child_report(test: &str, overrides: &[(&str, &str)]) -> Value {
    let arguments = ["--exact", test, "--nocapture", "--test-threads=1"];
    let mut command = Command::new(std::env::current_exe().expect("test executable"));
    command
        .args(arguments)
        .env("ENZIM_STARTUP_TEST_CHILD", "1")
        .env_remove("ENZIM_BENCH_SYSTEM_ALLOCATOR")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for variable in VARIABLES {
        command.env_remove(variable);
    }
    command.envs(overrides.iter().copied());
    let mut child = command.spawn().expect("start isolated allocator child");
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(10);
    let completed = loop {
        if child.try_wait().expect("startup child status").is_some() {
            break true;
        }
        if Instant::now() >= deadline {
            child.kill().expect("stop stalled startup child");
            child.wait().expect("reap stalled startup child");
            break false;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(completed, "startup child did not finish");
    let output = child.wait_with_output().expect("startup child output");
    assert!(
        output.status.success(),
        "startup child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("UTF-8 child output");
    let line = text
        .lines()
        .find_map(|line| line.strip_prefix("STARTUP_MEMORY "))
        .expect("startup report");
    let report: Value = serde_json::from_str(line).expect("startup report JSON");
    assert_eq!(report["pid"], pid, "re-exec must preserve process identity");
    assert_eq!(
        report["args"],
        json!(arguments),
        "arguments must survive re-exec"
    );
    report
}

#[test]
fn startup_memory_defaults_apply() {
    if report_child() {
        return;
    }
    let report = child_report("allocator::tests::startup_memory_defaults_apply", &[]);
    let values = &report["environment"];
    assert_eq!(values["MALLOC_ARENA_MAX"], "2");
    assert_eq!(values["MALLOC_MMAP_THRESHOLD_"], "131072");
    assert!(
        values["GLIBC_TUNABLES"].is_null(),
        "unrelated glibc options must remain unset"
    );
    #[cfg(feature = "wpe")]
    {
        assert_eq!(values["WEBKIT_DEFER_COMPLETE_IMAGE_FRAMES"], "1");
        assert_eq!(values["WEBKIT_RELEASE_SVG_RESOURCE_CACHES"], "1");
        assert_eq!(values["WEBKIT_TIGHT_BACKGROUND_HEAPS"], "1");
        assert_eq!(values["JSC_smallHeapGrowthFactor"], "1.75");
        assert_eq!(values["JSC_largeHeapSize"], "4194304");
        assert_eq!(values["WPE_READY_SCRIPT_PAUSE"], "1");
        assert_eq!(values["WPE_PREPARE_FD_TABLE"], "1");
        assert_eq!(values["ENZIM_PREPARE_FD_TABLE"], "1");
    }
}

#[test]
fn startup_memory_preserves_explicit_overrides() {
    if report_child() {
        return;
    }
    let overrides = [
        ("MALLOC_ARENA_MAX", "7"),
        ("MALLOC_MMAP_THRESHOLD_", "65536"),
        ("GLIBC_TUNABLES", "glibc.malloc.tcache_count=3"),
        ("WEBKIT_DEFER_COMPLETE_IMAGE_FRAMES", "0"),
        ("WEBKIT_RELEASE_SVG_RESOURCE_CACHES", "0"),
        ("WEBKIT_TIGHT_BACKGROUND_HEAPS", "0"),
        ("JSC_smallHeapGrowthFactor", "1.8"),
        ("JSC_largeHeapSize", "16777216"),
        ("WPE_READY_SCRIPT_PAUSE", "0"),
        ("WPE_PREPARE_FD_TABLE", "0"),
        ("ENZIM_PREPARE_FD_TABLE", "0"),
    ];
    let report = child_report(
        "allocator::tests::startup_memory_preserves_explicit_overrides",
        &overrides,
    );
    for (name, expected) in overrides {
        assert_eq!(report["environment"][name], expected, "override {name}");
    }
}
