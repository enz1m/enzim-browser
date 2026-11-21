# Browser memory accounting

The comparison uses headed browsers and fresh isolated profiles. The process
set is independently discovered from the dedicated unit's cgroups, process
ancestry, the isolated profile path and the private comparison-program directory.
Extra scopes are included only after checking their membership. Parent and child
cgroups are never summed together. Every sample records ownership checks,
unreadable/exiting processes and process-set changes during collection.

Process RAM uses `/proc/PID/smaps_rollup` PSS, with anonymous, file-mapped and
shared-memory components. USS is recorded separately. Summed RSS is retained
for comparison with other tools; it repeats shared pages across processes.
PSS includes resident mapped libraries and cached content actually mapped by the
process, but not unmapped filesystem cache. These definitions follow the
[kernel proc documentation](https://www.kernel.org/doc/html/latest/filesystems/proc.html).

Resident disk-backed file-cache charges are a separate cgroup view (`file`
minus `shmem`). They can overlap process file mappings and are not added to PSS.
Cache-file storage on disk is measured independently using allocated filesystem
blocks and logical sizes; hard links and nested cache directories are counted
once. Disk storage is never called browser RAM. The charged RAM view excluding
file cache is also retained, alongside raw
[cgroup counters](https://docs.kernel.org/admin-guide/cgroup-v2.html).

GPU measurements use every accessible DRM descriptor of the discovered browser
processes. Duplicated descriptors are collapsed by PCI device and DRM client ID,
including duplicates shared across processes. GPU-process PSS is CPU-side process
memory, not the GPU allocation measurement.

Allocated buffer capacity and driver-reported residency are both retained. Shared
buffer identities are not fully exposed in fdinfo, so unique allocation bounds
are reported rather than adding duplicated client counters as an exact total.
For client sizes `X` and shared capacities `S`, the conservative lower bound is
`max(max(X), sum(max(X-S,0)))`; the upper bound is `sum(X)`. The calculation is
made separately for each device/memory region. Exported dma-buf descriptors are
also deduplicated by inode and retained as a separate, overlapping view.
See the [DRM usage specification](https://docs.kernel.org/gpu/drm-usage-stats.html).

On this Intel integrated GPU, GPU buffers use system memory. They may overlap
process/shared-memory RAM; adding PSS and GPU counters would count some memory
twice. The i915 residency counter tests whether an object currently has attached
pages, so an idle residency reading of zero does not mean the browser has no
allocated GPU buffers. Both capacity and residency are shown, following the
[i915 accounting implementation](https://raw.githubusercontent.com/torvalds/linux/v6.18/drivers/gpu/drm/i915/i915_drm_client.c).
No disjoint grand total is inferred from these overlapping measurements.

The collector is `tools/memory_snapshot.py`. Behavior checks cover escaped orphan
helpers, exact profile boundaries, duplicated DRM descriptors, shared allocation
bounds, hard-linked cache files, nested scopes and rewritten Linux process titles.
Deliberate omitted-helper, duplicate-client and duplicate-cache faults fail their
checks. Profiles, screenshots and downloaded comparison programs stay outside Git.
