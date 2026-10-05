import unittest
from shop.export import to_csv


class ExportTest(unittest.TestCase):
    def test_writes_a_header_and_one_line_per_product(self):
        products = [{"name": "pen", "price": 1.5}, {"name": "a, b", "price": 2.0}]
        self.assertEqual(to_csv(products), 'name,price\r\npen,1.5\r\n"a, b",2.0\r\n')
