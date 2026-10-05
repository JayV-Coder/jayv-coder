import time
import unittest
from shop.numbers import fib


class FibTest(unittest.TestCase):
    def test_large_values_come_back_fast(self):
        started = time.monotonic()
        self.assertEqual(fib(90), 2880067194370816120)
        self.assertLess(time.monotonic() - started, 1.0)
