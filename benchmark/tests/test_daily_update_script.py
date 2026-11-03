import contextlib
import io
import socket
import unittest
from pathlib import Path
from unittest.mock import patch

from scripts import benchmark_update_prices as script


class DailyUpdateScriptTest(unittest.TestCase):
    def setUp(self):
        original = socket.getdefaulttimeout()
        self.addCleanup(socket.setdefaulttimeout, original)

    def test_rejects_historical_target_before_any_network_call(self):
        with patch.object(script, "prepare_daily_prices") as prepare, contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(script.main(["--path", "daily-prices"]), 1)
        prepare.assert_not_called()

    def test_only_complete_indices_without_active_source_failures_publish(self):
        for complete, unavailable, expected in ((True, 0, 0), (False, 0, 1), (True, 1, 1)):
            with self.subTest(complete=complete, unavailable=unavailable):
                result = {"stocks": 210, "source_unavailable": 6, "unavailable_active": unavailable,
                          "historical_gap_securities": 1,
                          "indices": {"930050": {"members": 50, "at_last_session": 50 if complete else 49},
                                      "000016": {"members": 50, "at_last_session": 50}}}
                with patch.object(script, "prepare_daily_prices", return_value=(Path("source"), None, "parent", {})), \
                        patch.object(script, "historical_prices_reference", return_value={}), \
                        patch.object(script, "collect_memberships", return_value={}), \
                        patch.object(script, "update_prices", return_value=result), \
                        patch.object(script, "publish_daily_prices", return_value={}) as publish, \
                        contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(script.main([]), expected)
                self.assertEqual(publish.call_count, 1 if expected == 0 else 0)
