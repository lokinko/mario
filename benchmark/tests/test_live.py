import copy
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from benchmark import hub, live
from benchmark.files import Invalid, digest
from benchmark.tests.test_strategy_runner import release, strategy


class LiveTest(unittest.TestCase):
    def test_progress_then_atomic_success_and_failed_update_preserves_scores(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            release(root / 'v8')
            strategy(root / 'strategies', 'cash', 'def decide(sample):\n    return {}\n')
            snapshots = []
            def publish(repo, path, data):
                snapshots.append(copy.deepcopy(data))
            with patch.object(live, 'live_read', return_value=None), patch.object(live, 'live_write', side_effect=publish), \
                    patch.object(live, 'fetch'):
                result = live.evaluate('inputs', 'labels', 'v8', 'first', '', root, root / 'strategies')
            self.assertEqual([x['evaluation']['status'] for x in snapshots], ['running', 'running', 'running', 'completed'])
            self.assertEqual([x['evaluation']['completed'] for x in snapshots], [0, 0, 1, 1])
            self.assertEqual(result['result_commit'], 'first')
            self.assertEqual(result['result_sources']['input_manifest_sha256'], digest(root / 'v8/manifest.json'))
            self.assertIsNone(result['result_sources']['raw_manifest_sha256'])
            self.assertEqual([s['key'] for s in result['strategies']], ['cash'])
            previous = copy.deepcopy(result)
            strategy(root / 'strategies', 'broken', 'import os\ndef decide(sample):\n    return {}\n')
            with patch.object(live, 'live_read', return_value=copy.deepcopy(previous)), \
                    patch.object(live, 'live_write', side_effect=publish), patch.object(live, 'fetch'):
                with self.assertRaises(Invalid):
                    live.evaluate('inputs', 'labels', 'v8', 'second', '', root, root / 'strategies')
            failed = snapshots[-1]
            self.assertEqual(failed['evaluation']['status'], 'failed')
            self.assertIn('broken', failed['evaluation']['error'])
            self.assertEqual(failed['result_commit'], 'first')
            self.assertEqual(failed['periods'], previous['periods'])
            self.assertEqual(failed['strategies'], previous['strategies'])

    def test_failed_result_upload_keeps_last_success_and_removed_strategy_disappears_on_success(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            release(root / 'v8')
            strategy(root / 'strategies', 'cash', 'def decide(sample):\n    return {}\n')
            previous = {'strategies': [{'key': 'retired'}], 'periods': [], 'result_commit': 'old'}
            snapshots = []
            def publish(repo, path, data):
                if data['evaluation']['status'] == 'completed':
                    raise OSError('upload failed')
                snapshots.append(copy.deepcopy(data))
            with patch.object(live, 'live_read', return_value=copy.deepcopy(previous)), \
                    patch.object(live, 'live_write', side_effect=publish), patch.object(live, 'fetch'):
                with self.assertRaises(OSError):
                    live.evaluate('inputs', 'labels', 'v8', 'next', '', root, root / 'strategies')
            self.assertEqual(snapshots[-1]['result_commit'], 'old')
            self.assertEqual(snapshots[-1]['strategies'], previous['strategies'])
            with patch.object(live, 'live_read', return_value=copy.deepcopy(previous)), \
                    patch.object(live, 'live_write'), patch.object(live, 'fetch'):
                result = live.evaluate('inputs', 'labels', 'v8', 'next', '', root, root / 'strategies')
            self.assertEqual([item['key'] for item in result['strategies']], ['cash'])

    def test_sync_rejects_invalid_or_unavailable_remote_without_losing_current_file(self):
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder)
            data = {'strategies': [], 'periods': [], 'evaluation': {'status': 'running'}}
            with patch.object(live, 'live_read', return_value=data):
                live.sync('labels', out)
                modified = (out / 'dashboard.json').stat().st_mtime_ns
                live.sync('labels', out)
                self.assertEqual((out / 'dashboard.json').stat().st_mtime_ns, modified)
            with patch.object(live, 'live_read', return_value={'bad': 'data'}):
                with self.assertRaises(Invalid):
                    live.sync('labels', out)
            self.assertEqual(json.loads((out / 'dashboard.json').read_text()), data)
            self.assertIn('sessionToken()', (out / 'dashboard.js').read_text())
            self.assertFalse(list(out.glob('*.part')))

    def test_hub_pins_public_download_and_only_missing_initializes(self):
        with tempfile.TemporaryDirectory() as folder:
            file = Path(folder) / 'data.json'
            file.write_text('{"periods": []}')
            from unittest.mock import Mock
            api = Mock()
            api.repo_info.return_value = SimpleNamespace(private=False, sha='fixed')
            api.file_exists.return_value = True
            adapter = Mock()
            adapter.HfApi.return_value = api
            adapter.hf_hub_download.return_value = str(file)
            with patch.object(hub, 'huggingface', return_value=adapter):
                self.assertEqual(hub.live_read('labels', live.LIVE_PATH), {'periods': []})
                self.assertEqual(adapter.hf_hub_download.call_args.kwargs['revision'], 'fixed')
                api.file_exists.return_value = False
                self.assertIsNone(hub.live_read('labels', live.LIVE_PATH, missing=True))
                with self.assertRaises(Invalid):
                    hub.live_read('labels', live.LIVE_PATH)
                api.file_exists.return_value = True
                cache = {}
                self.assertEqual(hub.live_read('labels', live.LIVE_PATH, cache=cache), {'periods': []})
                api.repo_info.return_value.private = True
                self.assertEqual(hub.live_read('labels', live.LIVE_PATH, cache=cache), {'periods': []})
                for private in (False, True):
                    api.repo_info.return_value.private = private
                    hub.live_write('labels', live.LIVE_PATH, {'evaluation': {'commit': 'next', 'status': 'running'}})
                self.assertEqual(api.upload_file.call_count, 2)
                self.assertFalse(api.upload_file.call_args.kwargs.get('private', False))

    def test_browser_polling_sends_session_and_keeps_view_until_result_changes(self):
        from benchmark.dashboard import SCRIPT
        import subprocess
        watch = SCRIPT[SCRIPT.index('function watch() {'):SCRIPT.index('\nloadHash();')]
        code = '''
const DATA = {generated_at: 'old', result_commit: 'a'};
const SOURCE = 'api/benchmark/dashboard';
const element = {};
const document = {hidden: false, getElementById: () => element};
let callback, headers, shown, reloads = 0;
const setTimeout = fn => { callback = fn; };
const sessionToken = () => 'account-token';
const showEvaluation = data => { shown = data; };
const location = {reload: () => { reloads++; }};
let responseData = {...DATA, evaluation: {status: 'running'}};
const fetch = async (url, options) => { headers = options.headers; return {ok: true, json: async () => responseData}; };
''' + watch + '''
(async () => {
  watch(); await callback();
  if (headers.Authorization !== 'Bearer account-token' || reloads !== 0 || shown.evaluation.status !== 'running') throw Error('progress');
  responseData = {...DATA, result_commit: 'b'}; await callback();
  if (reloads !== 1) throw Error('new snapshot');
})().catch(e => {console.error(e); process.exitCode = 1;});
'''
        result = subprocess.run(['node', '-e', code], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
