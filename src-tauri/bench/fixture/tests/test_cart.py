import unittest
from shop.cart import total


class CartTest(unittest.TestCase):
    def test_the_discount_is_a_percentage(self):
        self.assertEqual(total([10.0, 30.0], discount_percent=10), 36.0)

    def test_no_discount_keeps_the_sum(self):
        self.assertEqual(total([1.25, 2.5]), 3.75)
