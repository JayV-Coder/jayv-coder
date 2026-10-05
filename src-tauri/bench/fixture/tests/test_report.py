import unittest
from shop import report


class ReportTest(unittest.TestCase):
    def test_the_function_has_a_descriptive_name(self):
        self.assertEqual(report.calculate_total([(2, 1.5), (1, 4.0)]), 7.0)

    def test_the_old_name_still_works(self):
        self.assertEqual(report.calc([(1, 1.0)]), 1.0)
