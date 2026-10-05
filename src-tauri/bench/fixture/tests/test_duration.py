import unittest
from shop.duration import parse_duration


class DurationTest(unittest.TestCase):
    def test_hours_minutes_and_seconds(self):
        self.assertEqual(parse_duration("1h30m"), 5400)
        self.assertEqual(parse_duration("45s"), 45)

    def test_rejects_garbage(self):
        with self.assertRaises(ValueError):
            parse_duration("soon")
