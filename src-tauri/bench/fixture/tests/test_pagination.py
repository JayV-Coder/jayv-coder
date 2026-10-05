import unittest
from shop.pagination import paginate


class PaginateTest(unittest.TestCase):
    def test_the_first_page_starts_at_the_first_item(self):
        self.assertEqual(paginate(list(range(10)), 1, 3), [0, 1, 2])

    def test_the_last_page_may_be_short(self):
        self.assertEqual(paginate(list(range(10)), 4, 3), [9])
