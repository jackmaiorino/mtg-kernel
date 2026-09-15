"""The first preparation release cannot dispatch unqualified production work."""
import unittest
from unittest.mock import patch

import cloud_worker


class ExecutionScopeTests(unittest.TestCase):
    def test_production_rejected_before_state_reads_or_native_dispatch(self):
        with patch.object(cloud_worker, 'read') as read, \
                patch.object(cloud_worker.subprocess, 'Popen') as launch:
            with self.assertRaisesRegex(ValueError, 'production execution unavailable'):
                cloud_worker.run_locked('/unavailable/hot', '/unavailable/durable',
                    '/unavailable/guard', 200, mode='production',
                    proof_pin={'passed': True}, local_pin={'complete': True})
            read.assert_not_called()
            launch.assert_not_called()


if __name__ == '__main__':
    unittest.main()
