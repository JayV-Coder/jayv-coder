import io
import json
import unittest
from contextlib import redirect_stdout
from shop.cli import main


class CliTest(unittest.TestCase):
    def test_json_flag_prints_the_products_as_json(self):
        out = io.StringIO()
        with redirect_stdout(out):
            self.assertEqual(main(["--json"]), 0)
        self.assertEqual(json.loads(out.getvalue()), [{"name": "pen", "price": 1.5}, {"name": "notebook", "price": 4.0}])

    def test_without_the_flag_it_prints_the_table(self):
        out = io.StringIO()
        with redirect_stdout(out):
            main([])
        self.assertIn("notebook", out.getvalue())
