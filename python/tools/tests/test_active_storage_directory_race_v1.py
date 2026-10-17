"""An active storage census tolerates vanished children, never a missing root."""
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import g115_d4_audit_storage_v1 as storage


@unittest.skipUnless(os.name == 'nt', 'Windows allocation accounting')
class ActiveDirectoryRace(unittest.TestCase):
    def test_removed_empty_child_obeys_active_scan_mode(self):
        for active in (False, True):
            with self.subTest(active=active), tempfile.TemporaryDirectory() as raw:
                root = Path(raw).resolve()
                child = root/'rust-temporary-metadata'
                child.mkdir()
                (root/'retained').write_bytes(b'kept')
                scan = os.scandir

                def disappear(path):
                    if Path(path) == child:
                        child.rmdir()
                    return scan(path)

                with patch.object(storage.os, 'scandir', side_effect=disappear):
                    if active:
                        result = storage.physical_tree(root, allow_atomic_rename=True, use_logical_bound=True)
                        self.assertEqual(result['files'], 1)
                        self.assertEqual(result['logical_bytes'], 4)
                        self.assertEqual(result['charged_bytes'], storage.charge(4096, 1))
                    else:
                        with self.assertRaises(FileNotFoundError):
                            storage.physical_tree(root, use_logical_bound=True)

    def test_missing_root_still_fails_in_active_mode(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw).resolve()
            with patch.object(storage.os, 'scandir', side_effect=FileNotFoundError('root disappeared')):
                with self.assertRaises(FileNotFoundError):
                    storage.physical_tree(root, allow_atomic_rename=True, use_logical_bound=True)
