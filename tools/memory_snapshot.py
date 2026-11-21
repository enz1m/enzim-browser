#!/usr/bin/env python3
"""Audit browser process ownership and collect separate RAM/cache/DRM views."""
import argparse
import collections
import json
import os
from pathlib import Path
import re
import time


def fields(text):
    return dict(line.split(':', 1) for line in text.splitlines() if ':' in line)


def byte_value(value):
    parts = value.split()
    factors = {'B': 1, 'bytes': 1, 'kB': 1024, 'KiB': 1024, 'MiB': 1048576, 'GiB': 1073741824}
    return int(parts[0]) * (factors[parts[1]] if len(parts) > 1 else 1)


def minimal_roots(paths):
    roots = []
    for path in sorted(set(paths), key=lambda p: (len(p.parts), str(p))):
        if not any(path.is_relative_to(root) for root in roots):
            roots.append(path)
    return roots


def members(root):
    pids = set()
    for path in root.rglob('cgroup.procs'):
        try:
            pids.update(map(int, path.read_text().split()))
        except FileNotFoundError:
            pass
    return pids


def process_role(cmd, default):
    match = re.search(rb'(?:^|\s)--type=([^\s\x00]+)', b' '.join(cmd))
    return os.fsdecode(match.group(1)) if match else default


def processes():
    result = {}
    for p in Path('/proc').glob('[0-9]*'):
        try:
            status = fields((p / 'status').read_text())
            cmd = (p / 'cmdline').read_bytes().split(b'\0')
            try:
                exe = (p / 'exe').resolve(strict=True)
            except (PermissionError, FileNotFoundError):
                exe = None
            result[int(p.name)] = {'ppid': int(status['PPid']), 'name': status['Name'].strip(),
                                  'exe': exe, 'cmd': cmd, 'role': process_role(cmd, status['Name'].strip()),
                                  'cgroup': next(x[3:] for x in (p / 'cgroup').read_text().splitlines() if x.startswith('0::'))}
        except (OSError, KeyError, StopIteration, ValueError):
            continue
    return result


def discover(table, scope_pids, profile, program_dirs):
    marker = re.compile(rb'(?:^|=)' + re.escape(os.fsencode(profile)) + rb'(?:/|$)')
    excluded = set()
    current = os.getpid()
    while current and current not in excluded:
        excluded.add(current)
        current = table.get(current, {}).get('ppid', 0)
    profile_pids = {pid for pid, p in table.items() if pid not in excluded and any(marker.search(arg) for arg in p['cmd'])}
    program_pids = {pid for pid, p in table.items() if p['exe'] and any(p['exe'].is_relative_to(d) for d in program_dirs)}
    program_pids -= excluded
    owned = (set(scope_pids) | profile_pids | program_pids) - excluded
    while True:
        more = {pid for pid, p in table.items() if pid not in excluded and p['ppid'] in owned}
        if more <= owned:
            break
        owned |= more
    return owned, profile_pids, program_pids


def drm_bounds(clients, counter="resident"):
    regions = collections.defaultdict(list)
    for client in clients:
        f = client['fields']
        for key, value in f.items():
            if key.startswith('drm-' + counter + '-'):
                region = key[len('drm-' + counter + '-'):]
                resident = byte_value(value)
                shared = byte_value(f.get('drm-shared-' + region, '0'))
                regions[(client['device'], region)].append((resident, shared))
    details = []
    for (device, region), rows in regions.items():
        residents = [r for r, _ in rows]
        # Shared buffer identities are not exported by fdinfo; report valid bounds.
        lower = max(max(residents, default=0), sum(max(0, r - s) for r, s in rows))
        details.append({'device': device, 'region': region, counter + '_lower_bytes': lower,
                        counter + '_upper_bytes': sum(residents), 'shared_client_sum_bytes': sum(s for _, s in rows)})
    return {'regions': details, counter + '_lower_bytes': sum(d[counter + '_lower_bytes'] for d in details),
            counter + '_upper_bytes': sum(d[counter + '_upper_bytes'] for d in details)}


def add_drm_client(clients, f, pid, fd):
    if 'drm-client-id' not in f or 'drm-pdev' not in f:
        return False
    key = (f['drm-pdev'], f['drm-client-id'])
    if key not in clients:
        clients[key] = {'device': key[0], 'client_id': key[1], 'fields': {k: v for k, v in f.items() if k.startswith('drm-')}, 'holders': []}
    clients[key]['holders'].append({'pid': pid, 'fd': int(fd)})
    return True


def gpu_info(pids):
    clients = {}
    exported = {}
    unreadable = []
    unidentifiable = []
    for pid in sorted(pids):
        folder = Path('/proc') / str(pid) / 'fdinfo'
        try:
            paths = list(folder.iterdir())
        except OSError:
            unreadable.append(pid)
            continue
        for path in paths:
            try:
                f = {k: v.strip() for k, v in fields(path.read_text()).items()}
            except (FileNotFoundError, ProcessLookupError):
                continue
            except PermissionError:
                unreadable.append(pid)
                continue
            if 'drm-driver' in f:
                if not add_drm_client(clients, f, pid, path.name):
                    unidentifiable.append({'pid': pid, 'fd': path.name})
            if 'exp_name' in f and 'ino' in f:
                inode = f['ino']
                if inode not in exported:
                    size_path = Path('/sys/kernel/dmabuf/buffers') / inode / 'size'
                    try:
                        size = int(size_path.read_text())
                    except OSError:
                        size = int(f.get('size', '0'))
                    exported[inode] = {'inode': inode, 'size_bytes': size, 'exporter': f['exp_name'], 'holders': []}
                exported[inode]['holders'].append({'pid': pid, 'fd': int(path.name)})
    rows = list(clients.values())
    return {'clients': rows, 'unique_client_count': len(rows),
            'descriptor_count': sum(len(c['holders']) for c in rows),
            'bounds': drm_bounds(rows), 'allocation_bounds': drm_bounds(rows, 'total'), 'exported_buffers': list(exported.values()),
            'exported_bytes_not_additive': sum(b['size_bytes'] for b in exported.values()),
            'unreadable_pids': sorted(set(unreadable)), 'unidentifiable_drm_descriptors': unidentifiable}


def cache_storage(profile):
    roots = [profile / 'cache']
    names = {'cache', 'cache2', 'code cache', 'gpucache', 'shadercache', 'grshadercache',
             'dawngraphitecache', 'dawnwebgpucache', 'graphitedawncache', 'cachestorage', 'startupcache'}
    for folder, dirs, _ in os.walk(profile, followlinks=False):
        for name in dirs:
            p = Path(folder) / name
            if name.lower() in names and not p.is_symlink():
                roots.append(p)
    roots = minimal_roots([r for r in roots if r.is_dir()])
    logical = allocated = 0
    seen = set()
    for root in roots:
        for p in root.rglob('*'):
            try:
                if p.is_symlink() or not p.is_file():
                    continue
                s = p.stat()
                key = (s.st_dev, s.st_ino)
                if key in seen:
                    continue
                seen.add(key)
                logical += s.st_size
                allocated += s.st_blocks * 512
            except FileNotFoundError:
                continue
    return {'roots': [str(p) for p in roots], 'logical_bytes': logical,
            'allocated_bytes': allocated, 'files': len(seen)}


def cgroup_roots(root, escaped, table, owned):
    roots = [root]
    contaminated = []
    for pid in escaped:
        if pid not in table:
            continue
        extra = Path('/sys/fs/cgroup') / table[pid]['cgroup'].lstrip('/')
        # Fresh forks can miss the first snapshot while inheriting the limited browser unit.
        if extra.is_relative_to(root):
            continue
        if members(extra) <= owned:
            roots.append(extra)
        else:
            contaminated.append(str(extra))
    return minimal_roots(roots), sorted(set(contaminated))


def collect(unit, profile, program_dirs=()):
    import subprocess
    started = time.time()
    cg = subprocess.check_output(['systemctl', '--user', 'show', unit, '-p', 'ControlGroup', '--value'], text=True).strip()
    if not cg or cg == '/':
        raise ValueError('A dedicated browser unit is required')
    root = Path('/sys/fs/cgroup') / cg.lstrip('/')
    initial_members = members(root)
    table = processes()
    owned, profile_pids, program_pids = discover(table, initial_members, profile, program_dirs)
    escaped = owned - initial_members
    roots, contaminated = cgroup_roots(root, escaped, table, owned)
    rows = []
    unreadable = []
    vanished = []
    for pid in sorted(owned):
        p = Path('/proc') / str(pid)
        try:
            f = fields((p / 'smaps_rollup').read_text())
            row = {'pid': pid, 'ppid': table.get(pid, {}).get('ppid'), 'name': table.get(pid, {}).get('name'),
                   'cgroup': table.get(pid, {}).get('cgroup'), 'role': table.get(pid, {}).get('role'),
                   'ram': {k: byte_value(f.get(k, '0')) for k in ['Rss', 'Pss', 'Pss_Anon', 'Pss_File', 'Pss_Shmem', 'Private_Clean', 'Private_Dirty', 'Private_Hugetlb', 'Shared_Hugetlb', 'AnonHugePages', 'LazyFree', 'Swap']}}
            row['uss_bytes'] = row['ram']['Private_Clean'] + row['ram']['Private_Dirty'] + row['ram']['Private_Hugetlb']
            rows.append(row)
        except (FileNotFoundError, ProcessLookupError):
            vanished.append(pid)
        except PermissionError:
            unreadable.append(pid)
    scopes = []
    for scope in roots:
        stat = dict((k, int(v)) for k, v in (line.split() for line in (scope / 'memory.stat').read_text().splitlines()))
        scopes.append({'path': str(scope), 'current_bytes': int((scope / 'memory.current').read_text()),
                       'swap_bytes': int((scope / 'memory.swap.current').read_text()), 'stat': stat,
                       'events': (scope / 'memory.events').read_text(),
                       'resident_nonshmem_file_bytes': max(0, stat['file'] - stat['shmem'])})
    gpu = gpu_info(owned)
    cache = cache_storage(profile)
    after = processes()
    after_owned, _, _ = discover(after, members(root), profile, program_dirs)
    return {'started': started, 'finished': time.time(), 'unit': unit, 'profile': str(profile),
            'processes': rows, 'process_count': len(rows), 'audit': {'scope_pids': sorted(initial_members),
             'profile_pids': sorted(profile_pids), 'program_pids': sorted(program_pids), 'escaped_pids': sorted(escaped),
             'unreadable_pss': unreadable, 'vanished': vanished, 'new_pids_during_sample': sorted(after_owned - owned),
             'contaminated_extra_cgroups': sorted(set(contaminated))},
            'ram_pss_bytes': sum(p['ram']['Pss'] for p in rows), 'ram_uss_bytes': sum(p['uss_bytes'] for p in rows),
            'rss_sum_bytes_not_unique': sum(p['ram']['Rss'] for p in rows),
            'pss_anon_bytes': sum(p['ram']['Pss_Anon'] for p in rows),
            'pss_file_bytes': sum(p['ram']['Pss_File'] for p in rows),
            'pss_shmem_bytes': sum(p['ram']['Pss_Shmem'] for p in rows),
            'cgroups': scopes, 'gpu': gpu, 'cache_storage': cache,
            'resident_nonshmem_file_cache_bytes_not_additive': sum(s['resident_nonshmem_file_bytes'] for s in scopes)}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--unit', required=True)
    parser.add_argument('--profile', type=Path, required=True)
    parser.add_argument('--program-directory', type=Path, action='append', default=[])
    args = parser.parse_args()
    print(json.dumps(collect(args.unit, args.profile.resolve(), [p.resolve() for p in args.program_directory]), indent=2))
