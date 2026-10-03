"""Offline visibility fixtures; no native processes, provider calls or host reads."""
import unittest

from throughput import cgroup_limits


V2_MOUNT = '12 3 0:20 / /sys/fs/cgroup rw - cgroup2 cgroup rw'
V1_MOUNT = '12 3 0:20 / /sys/fs/cgroup/cpu rw - cgroup cgroup rw,cpu,cpuacct'


class QuotaVisibilityTests(unittest.TestCase):
    def v2(self, files, membership='0::/tenant/job', mount=V2_MOUNT, affinity=range(32)):
        return cgroup_limits(membership, mount, files.__getitem__, affinity)

    def full_v2(self):
        return {'/sys/fs/cgroup/tenant/job/cpu.max': 'max 100000',
                '/sys/fs/cgroup/tenant/cpu.max': '200000 100000',
                '/sys/fs/cgroup/cpu.max': 'max 100000'}

    def test_complete_visible_ancestry_is_only_an_upper_bound(self):
        value = self.v2(self.full_v2())
        self.assertEqual(value['eligible_capacity_cpus'], 2)
        self.assertEqual(value['capacity_upper_bound_cpus'], 2)
        self.assertTrue(value['visible_hierarchy_occupancy_eligible'])
        self.assertFalse(value['production_occupancy_eligible'])
        self.assertFalse(value['capacity_is_measured_entitlement'])
        self.assertEqual(value['cpu_quota_observability']['production_rejection_reasons'],
                         ['hidden_ancestor_limits_unknown'])
        boundary = value['cpu_quota_observability']['visible_root_boundaries'][0]
        self.assertEqual(boundary['walked_paths'], ['/sys/fs/cgroup/tenant/job',
                         '/sys/fs/cgroup/tenant', '/sys/fs/cgroup'])
        self.assertEqual(boundary['ancestors_above_visible_root'], 'unknown')

    def test_missing_parent_does_not_establish_visible_entitlement(self):
        files = self.full_v2(); del files['/sys/fs/cgroup/tenant/cpu.max']
        value = self.v2(files)
        self.assertEqual(value['eligible_capacity_cpus'], 32)
        self.assertFalse(value['visible_hierarchy_occupancy_eligible'])
        failures = value['cpu_quota_observability']['read_failures']
        self.assertIn({'path': '/sys/fs/cgroup/tenant/cpu.max', 'controller': 'cpu',
                       'kind': 'missing', 'error_type': 'KeyError', 'errno': None}, failures)

    def test_unreadable_task_quota_retains_observed_parent_bound(self):
        files = self.full_v2()

        def read_text(path):
            if path == '/sys/fs/cgroup/tenant/job/cpu.max':
                raise PermissionError(13, 'fixture denied')
            return files[path]

        value = cgroup_limits('0::/tenant/job', V2_MOUNT, read_text, range(32))
        self.assertEqual(value['eligible_capacity_cpus'], 2)
        self.assertFalse(value['visible_hierarchy_occupancy_eligible'])
        failure = next(row for row in value['cpu_quota_observability']['read_failures']
                       if row['controller'] == 'cpu')
        self.assertEqual((failure['kind'], failure['error_type'], failure['errno']),
                         ('unreadable', 'PermissionError', 13))

    def test_no_quota_observations_are_affinity_only(self):
        for membership, mount in [('0::/', V2_MOUNT), ('', '')]:
            with self.subTest(mount=mount):
                value = cgroup_limits(membership, mount, {}.__getitem__, range(4))
                self.assertEqual(value['eligible_capacity_cpus'], 4)
                self.assertEqual(value['capacity_basis'], 'affinity_only_upper_bound')
                self.assertFalse(value['visible_hierarchy_occupancy_eligible'])

    def test_explicit_unlimited_quota_is_observed_but_not_host_entitlement(self):
        value = self.v2({'/sys/fs/cgroup/cpu.max': 'max 100000'}, membership='0::/')
        self.assertTrue(value['visible_hierarchy_occupancy_eligible'])
        self.assertEqual(value['capacity_basis'], 'visible_quota_and_affinity_upper_bound')
        self.assertFalse(value['production_occupancy_eligible'])

    def test_namespaced_mount_boundary_is_preserved(self):
        mount = '12 3 0:20 /tenant /sys/fs/cgroup rw - cgroup2 cgroup rw'
        value = self.v2({'/sys/fs/cgroup/job/cpu.max': 'max 100000',
                         '/sys/fs/cgroup/cpu.max': '150000 100000'},
                        membership='0::/tenant/job', mount=mount)
        self.assertEqual(value['eligible_capacity_cpus'], 1.5)
        boundary = value['cpu_quota_observability']['visible_root_boundaries'][0]
        self.assertEqual(boundary['mount_root'], '/tenant')
        self.assertEqual(boundary['resolved_task_path'], '/sys/fs/cgroup/job')
        self.assertFalse(value['production_occupancy_eligible'])

    def test_namespace_relative_root_is_visible_not_host_root(self):
        value = self.v2({'/sys/fs/cgroup/cpu.max': '100000 100000'}, membership='0::/',
                        mount='12 3 0:20 /tenant /sys/fs/cgroup rw - cgroup2 cgroup rw')
        self.assertTrue(value['visible_hierarchy_occupancy_eligible'])
        self.assertFalse(value['production_occupancy_eligible'])
        self.assertEqual(value['cpu_quota_observability']['visible_root_boundaries'][0]
                         ['resolved_task_path'], '/sys/fs/cgroup')

    def test_unresolved_and_parent_traversal_memberships_do_not_walk(self):
        for member in ('/other', '/../../tenant', 'relative'):
            with self.subTest(member=member):
                calls = []

                def unexpected(path):
                    calls.append(path)
                    raise AssertionError('unresolved membership must not read quotas')

                value = cgroup_limits('0::' + member,
                    '12 3 0:20 /tenant /sys/fs/cgroup rw - cgroup2 cgroup rw', unexpected, range(2))
                self.assertEqual(calls, [])
                self.assertFalse(value['visible_hierarchy_occupancy_eligible'])
                self.assertIn('cpu_membership_unresolved', value['cpu_quota_observability']
                              ['visible_hierarchy_rejection_reasons'])

    def test_invalid_v2_quota_is_evidence_not_an_unlimited_limit(self):
        for quota in ('', 'max 0', '0 100000', '100 0', 'bad 100000', 'max', 'max 100 extra'):
            with self.subTest(quota=quota):
                value = self.v2({'/sys/fs/cgroup/cpu.max': quota}, membership='0::/')
                self.assertFalse(value['visible_hierarchy_occupancy_eligible'])
                self.assertIn('visible_cpu_quota_invalid', value['cpu_quota_observability']
                              ['visible_hierarchy_rejection_reasons'])

    def test_v1_complete_parent_quota_and_unlimited_task(self):
        files = {'/sys/fs/cgroup/cpu/job/cpu.cfs_quota_us': '-1',
                 '/sys/fs/cgroup/cpu/job/cpu.cfs_period_us': '100000',
                 '/sys/fs/cgroup/cpu/cpu.cfs_quota_us': '250000',
                 '/sys/fs/cgroup/cpu/cpu.cfs_period_us': '100000'}
        value = cgroup_limits('2:cpu,cpuacct:/job', V1_MOUNT, files.__getitem__, range(8))
        self.assertEqual(value['eligible_capacity_cpus'], 2.5)
        self.assertTrue(value['visible_hierarchy_occupancy_eligible'])
        self.assertFalse(value['production_occupancy_eligible'])
        del files['/sys/fs/cgroup/cpu/cpu.cfs_period_us']
        incomplete = cgroup_limits('2:cpu,cpuacct:/job', V1_MOUNT, files.__getitem__, range(8))
        self.assertFalse(incomplete['visible_hierarchy_occupancy_eligible'])

    def test_v1_invalid_negative_quota_is_not_unlimited(self):
        files = {'/sys/fs/cgroup/cpu/cpu.cfs_quota_us': '-2',
                 '/sys/fs/cgroup/cpu/cpu.cfs_period_us': '100000'}
        value = cgroup_limits('2:cpu,cpuacct:/', V1_MOUNT, files.__getitem__, range(8))
        self.assertFalse(value['visible_hierarchy_occupancy_eligible'])

    def test_v1_period_without_quota_does_not_claim_quota_based_estimate(self):
        files = {'/sys/fs/cgroup/cpu/cpu.cfs_period_us': '100000'}
        value = cgroup_limits('2:cpu,cpuacct:/', V1_MOUNT, files.__getitem__, range(8))
        self.assertEqual(value['capacity_basis'], 'affinity_only_upper_bound')
        self.assertFalse(value['visible_hierarchy_occupancy_eligible'])

    def test_missing_memory_limit_does_not_invalidate_cpu_reads(self):
        value = self.v2(self.full_v2())
        self.assertTrue(value['visible_hierarchy_occupancy_eligible'])
        self.assertIsNone(value['visible_memory_limit_bytes'])
        self.assertTrue(any(row['controller'] == 'memory'
                            for row in value['cpu_quota_observability']['read_failures']))

    def test_reported_wsl_hybrid_layout_uses_active_v1_controllers(self):
        # Regression for the observed WSL topology. Numeric limits are fixtures,
        # not a claim about the workstation's actual capacity or memory limit.
        membership = '2:cpu,cpuacct:/\n3:memory:/\n0::/'
        mounts = '\n'.join((V1_MOUNT,
            '13 3 0:21 / /sys/fs/cgroup/memory rw - cgroup cgroup rw,memory',
            '14 3 0:22 / /sys/fs/cgroup/unified rw - cgroup2 cgroup rw'))
        files = {'/sys/fs/cgroup/cpu/cpu.cfs_quota_us': '-1',
                 '/sys/fs/cgroup/cpu/cpu.cfs_period_us': '100000',
                 '/sys/fs/cgroup/memory/memory.limit_in_bytes': '8589934592'}
        calls = []

        def read_text(path):
            calls.append(path)
            return files[path]

        value = cgroup_limits(membership, mounts, read_text, range(32))
        self.assertTrue(value['visible_hierarchy_occupancy_eligible'])
        self.assertFalse(value['production_occupancy_eligible'])
        self.assertEqual(value['eligible_capacity_cpus'], 32)
        self.assertEqual(value['visible_memory_limit_bytes'], 8589934592)
        self.assertEqual(value['cpu_quota_observability']['controller_hierarchies'],
                         {'cpu': 'v1', 'memory': 'v1'})
        self.assertEqual(set(calls), set(files))
        self.assertEqual(value['cpu_quota_observability']['read_failures'], [])
        self.assertEqual(len(value['cpu_quota_observability']['visible_root_boundaries']), 1)

    def test_hybrid_missing_active_cpu_quota_still_rejects_visibility(self):
        files = {'/sys/fs/cgroup/cpu/cpu.cfs_period_us': '100000',
                 # Even present v2 limits cannot replace explicit v1 membership.
                 '/sys/fs/cgroup/cpu.max': 'max 100000',
                 '/sys/fs/cgroup/memory.max': 'max'}
        value = cgroup_limits('2:cpu,cpuacct:/\n0::/', '\n'.join((V1_MOUNT, V2_MOUNT)),
                              files.__getitem__, range(32))
        self.assertFalse(value['visible_hierarchy_occupancy_eligible'])
        self.assertNotIn('/sys/fs/cgroup/cpu.max', value['visible_cgroup_values'])
        self.assertIn('/sys/fs/cgroup/cpu/cpu.cfs_quota_us',
                      [row['path'] for row in value['cpu_quota_observability']['read_failures']])

    def test_cpu_and_memory_route_independently_in_hybrid_hierarchy(self):
        cases = [
            ('2:cpu,cpuacct:/\n0::/', V1_MOUNT,
             {'/sys/fs/cgroup/cpu/cpu.cfs_quota_us': '200000',
              '/sys/fs/cgroup/cpu/cpu.cfs_period_us': '100000',
              '/sys/fs/cgroup/memory.max': '8589934592'},
             {'cpu': 'v1', 'memory': 'v2'}),
            ('3:memory:/\n0::/',
             '13 3 0:21 / /sys/fs/cgroup/memory rw - cgroup cgroup rw,memory',
             {'/sys/fs/cgroup/cpu.max': '200000 100000',
              '/sys/fs/cgroup/memory/memory.limit_in_bytes': '8589934592'},
             {'cpu': 'v2', 'memory': 'v1'})]
        for membership, v1_mount, files, expected_routes in cases:
            with self.subTest(routes=expected_routes):
                value = cgroup_limits(membership, '\n'.join((v1_mount, V2_MOUNT)),
                                      files.__getitem__, range(32))
                self.assertTrue(value['visible_hierarchy_occupancy_eligible'])
                self.assertEqual(value['eligible_capacity_cpus'], 2)
                self.assertEqual(value['visible_memory_limit_bytes'], 8589934592)
                self.assertEqual(value['cpu_quota_observability']['controller_hierarchies'],
                                 expected_routes)
                self.assertEqual(value['cpu_quota_observability']['read_failures'], [])

    def test_missing_active_v1_mount_does_not_fall_back_to_v2(self):
        files = {'/sys/fs/cgroup/cpu.max': 'max 100000',
                 '/sys/fs/cgroup/memory.max': 'max'}
        value = cgroup_limits('2:cpu,cpuacct:/\n0::/', V2_MOUNT, files.__getitem__, range(32))
        self.assertFalse(value['visible_hierarchy_occupancy_eligible'])
        self.assertEqual(value['cpu_quota_observability']['status'], 'cpu_hierarchy_unobserved')
        self.assertNotIn('/sys/fs/cgroup/cpu.max', value['visible_cgroup_values'])

    def test_co_mounted_v1_cpu_and_memory_are_both_read(self):
        files = {'/sys/fs/cgroup/both/cpu.cfs_quota_us': '200000',
                 '/sys/fs/cgroup/both/cpu.cfs_period_us': '100000',
                 '/sys/fs/cgroup/both/memory.limit_in_bytes': '8589934592'}
        value = cgroup_limits('2:cpu,cpuacct,memory:/',
            '12 3 0:20 / /sys/fs/cgroup/both rw - cgroup cgroup rw,cpu,cpuacct,memory',
            files.__getitem__, range(32))
        self.assertTrue(value['visible_hierarchy_occupancy_eligible'])
        self.assertEqual(value['visible_memory_limit_bytes'], 8589934592)


if __name__ == '__main__':
    unittest.main()
