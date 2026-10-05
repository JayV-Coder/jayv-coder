import unittest
from shop.signup import validate


class SignupTest(unittest.TestCase):
    def test_a_missing_or_malformed_email_is_a_problem(self):
        self.assertIn("email", validate({"name": "Ana", "email": "ana-at-example"}))
        self.assertIn("email", validate({"name": "Ana"}))

    def test_a_valid_form_has_no_problems(self):
        self.assertEqual(validate({"name": "Ana", "email": "ana@example.com"}), [])
