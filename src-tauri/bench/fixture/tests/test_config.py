import os
import tempfile
import unittest
from shop.config import DEFAULTS, load_config


class ConfigTest(unittest.TestCase):
    def test_a_missing_file_falls_back_to_the_defaults(self):
        self.assertEqual(load_config(os.path.join(tempfile.gettempdir(), "no-such-shop-config.json")), DEFAULTS)

    def test_a_file_overrides_the_defaults(self):
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
            handle.write('{"page_size": 5}')
        try:
            self.assertEqual(load_config(handle.name)["page_size"], 5)
        finally:
            os.unlink(handle.name)
