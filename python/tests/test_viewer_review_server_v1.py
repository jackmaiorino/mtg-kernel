import hashlib
import json
import sys
import tempfile
import threading
import unittest
import urllib.error
import urllib.request
from http.server import ThreadingHTTPServer
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / 'tools'
sys.path.insert(0, str(TOOLS))
import viewer_review_server_v1 as review_server


class ReviewServerTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        games, dist = self.root / 'games', self.root / 'dist'
        games.mkdir(); dist.mkdir()
        (dist / 'index.html').write_text('<html>studio</html>')
        (games / 'g-1.json').write_text('{"schema":"x"}')
        (games / 'index.json').write_text(json.dumps([{'id': 'g-1', 'file': 'g-1.json', 'title': 'Burn vs Elves', 'subtitle': 'g115'},
                                                       {'id': 'bad', 'file': '../escape.json'}]))
        self.flags = self.root / 'flags.jsonl'
        self.server = ThreadingHTTPServer(('127.0.0.1', 0), review_server.handler(review_server.Review(games, self.flags, dist)))
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.url = f'http://127.0.0.1:{self.server.server_address[1]}/'

    def tearDown(self):
        self.server.shutdown(); self.server.server_close()

    def get(self, path):
        with urllib.request.urlopen(self.url + path) as response:
            return response.read()

    def post(self, value):
        request = urllib.request.Request(self.url + 'api/flags', data=json.dumps(value).encode(), method='POST',
                                         headers={'content-type': 'application/json'})
        with urllib.request.urlopen(request) as response:
            return json.loads(response.read())

    def test_manifest_lists_hashed_games_in_review_mode_and_refuses_paths(self):
        manifest = json.loads(self.get('demo/manifest.json'))
        self.assertEqual(manifest['review'], {'flags': 'api/flags'})
        self.assertEqual([g['id'] for g in manifest['games']], ['g-1'])
        self.assertEqual(manifest['games'][0]['sha256'], hashlib.sha256(b'{"schema":"x"}').hexdigest())
        self.assertEqual(self.get('demo/g-1.json'), b'{"schema":"x"}')
        self.assertEqual(self.get('../../etc/passwd'), b'<html>studio</html>')

    def test_flags_append_as_jsonl_and_reject_unknown_categories(self):
        flag = {'game': 'g-1', 'decision': 'd4', 'index': 4, 'turn': 2, 'category': 'tactical', 'note': 'missed lethal'}
        self.assertEqual(self.post(flag), {'ok': True})
        self.post({**flag, 'removed': True})
        with self.assertRaises(urllib.error.HTTPError) as error:
            self.post({**flag, 'category': 'vibes'})
        self.assertEqual(error.exception.code, 400)
        lines = [json.loads(line) for line in self.flags.read_text().splitlines()]
        self.assertEqual([(l['decision'], l.get('removed', False)) for l in lines], [('d4', False), ('d4', True)])
        self.assertIn('flagged_at', lines[0])
        self.assertEqual(json.loads(self.get('api/flags')), lines)


if __name__ == '__main__':
    unittest.main()
