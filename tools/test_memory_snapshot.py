"""Controls for browser ownership and non-additive memory accounting."""
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import memory_snapshot as m
from benchmark_memory_short import enforce_owned_limits, require_audited


class MemoryAccounting(unittest.TestCase):
    def test_fork_between_membership_and_process_snapshots_stays_in_owned_unit(self):
        root = Path('/sys/fs/cgroup/user.slice/browser.service')
        table = {2: {'cgroup': '/user.slice/browser.service'}}
        with mock.patch.object(m, 'members', return_value={1, 2, 3}) as members:
            roots, contaminated = m.cgroup_roots(root, {2}, table, {1, 2})
        self.assertEqual(roots, [root])
        self.assertEqual(contaminated, [])
        members.assert_not_called()

    def test_new_nested_browser_group_inherits_owned_parent_limit(self):
        root = Path('/sys/fs/cgroup/user.slice/browser.service')
        table = {2: {'cgroup': '/user.slice/browser.service/renderer'}}
        with mock.patch.object(m, 'members', return_value={2, 3}) as members:
            roots, contaminated = m.cgroup_roots(root, {2}, table, {1, 2})
        self.assertEqual(roots, [root])
        self.assertEqual(contaminated, [])
        members.assert_not_called()

    def test_shared_parent_and_similarly_named_sibling_still_refuse_limit_changes(self):
        root = Path('/sys/fs/cgroup/user.slice/browser.service')
        for group in ['/user.slice', '/user.slice/browser.service-extra.scope']:
            with self.subTest(group=group), mock.patch.object(m, 'members', return_value={2, 99}):
                roots, contaminated = m.cgroup_roots(root, {2}, {2: {'cgroup': group}}, {1, 2})
                self.assertEqual(roots, [root])
                self.assertEqual(contaminated, ['/sys/fs/cgroup' + group])
                sample = {'audit': {'contaminated_extra_cgroups': contaminated}, 'cgroups': []}
                applied = {}
                with mock.patch('benchmark_memory_short.subprocess.run') as run:
                    with self.assertRaisesRegex(RuntimeError, 'shared cgroup'):
                        enforce_owned_limits(sample, '3G', applied)
                run.assert_not_called()
                self.assertEqual(applied, {})

    def test_exclusive_escaped_scope_remains_a_separate_accounted_root(self):
        root = Path('/sys/fs/cgroup/user.slice/browser.service')
        extra = Path('/sys/fs/cgroup/user.slice/private-renderer.scope')
        with mock.patch.object(m, 'members', return_value={2}):
            roots, contaminated = m.cgroup_roots(root, {2}, {2: {'cgroup': '/user.slice/private-renderer.scope'}}, {1, 2})
        self.assertEqual(set(roots), {root, extra})
        self.assertEqual(contaminated, [])

    def test_fork_after_process_snapshot_still_invalidates_pss_sample(self):
        audit = {key: [] for key in ['unreadable_pss', 'vanished', 'new_pids_during_sample', 'contaminated_extra_cgroups']}
        audit['new_pids_during_sample'] = [3]
        with self.assertRaisesRegex(RuntimeError, 'new_pids_during_sample'):
            require_audited({'audit': audit})

    def test_linux_rewritten_single_argument_process_title_retains_role(self):
        self.assertEqual(m.process_role([b'/private/chrome --type=gpu-process --flag=1'], 'chrome'), 'gpu-process')
        self.assertEqual(m.process_role([b'/private/chrome', b'--type=renderer'], 'chrome'), 'renderer')


    def test_orphaned_browser_helper_outside_scope_is_discovered(self):
        def row(parent, exe, args=()):
            return {'ppid': parent, 'exe': Path(exe), 'cmd': list(args)}
        table = {100: row(1, '/private/chrome/chrome'),
                 101: row(100, '/private/chrome/chrome'),
                 102: row(1, '/private/chrome/chrome'),
                 103: row(1, '/system/chrome', [b'--user-data-dir=/profiles/test-other']),
                 104: row(1, '/system/chrome', [b'--user-data-dir=/profiles/test/chrome'])}
        owned, profile, program = m.discover(table, {100}, Path('/profiles/test'), [Path('/private/chrome')])
        self.assertEqual(owned, {100, 101, 102, 104})
        self.assertEqual(profile, {104})
        self.assertIn(102, program)

    def test_shared_drm_descriptor_counts_once_across_processes(self):
        clients = {}
        f = {'drm-pdev': '0000:00:02.0', 'drm-client-id': '7',
             'drm-resident-system0': '100 MiB', 'drm-shared-system0': '0'}
        self.assertTrue(m.add_drm_client(clients, f, 10, 3))
        self.assertTrue(m.add_drm_client(clients, f, 10, 9))
        self.assertTrue(m.add_drm_client(clients, f, 11, 5))
        self.assertEqual(len(clients), 1)
        self.assertEqual(len(next(iter(clients.values()))['holders']), 3)
        self.assertEqual(m.drm_bounds(list(clients.values()))['resident_upper_bytes'], 100 * 2**20)

    def test_idle_zero_residency_does_not_erase_gpu_buffer_capacity(self):
        clients = [{'device': 'gpu0', 'fields': {'drm-total-system0': '228 MiB', 'drm-resident-system0': '0', 'drm-shared-system0': '25 MiB'}}]
        self.assertEqual(m.drm_bounds(clients)['resident_upper_bytes'], 0)
        self.assertEqual(m.drm_bounds(clients, 'total')['total_lower_bytes'], 228 * 2**20)

    def test_shared_buffer_residency_remains_a_bound(self):
        clients = [{'device': 'gpu0', 'fields': {'drm-resident-system0': '100 MiB', 'drm-shared-system0': '40 MiB'}} for _ in range(2)]
        b = m.drm_bounds(clients)
        self.assertLessEqual(b['resident_lower_bytes'], 160 * 2**20)
        self.assertGreaterEqual(b['resident_upper_bytes'], 160 * 2**20)
        self.assertNotEqual(b['resident_lower_bytes'], b['resident_upper_bytes'])
        for c in clients:
            c['fields']['drm-shared-system0'] = '0'
        b = m.drm_bounds(clients)
        self.assertEqual(b['resident_lower_bytes'], b['resident_upper_bytes'])

    def test_cache_files_and_nested_scopes_are_not_double_counted(self):
        roots = m.minimal_roots([Path('/cg/app'), Path('/cg/app/renderer'), Path('/cg/other')])
        self.assertEqual(set(roots), {Path('/cg/app'), Path('/cg/other')})
        with tempfile.TemporaryDirectory() as d:
            p = Path(d);cache = p / 'cache' / 'Cache';cache.mkdir(parents=True)
            f = cache / 'entry';f.write_bytes(b'x' * 8192);os.link(f, cache / 'alias')
            s = m.cache_storage(p)
            self.assertEqual(s['logical_bytes'], 8192)
            self.assertEqual(s['files'], 1)


if __name__ == '__main__':
    unittest.main()
