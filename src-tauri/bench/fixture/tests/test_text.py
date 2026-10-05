import unittest
from shop.text import slugify


class SlugifyTest(unittest.TestCase):
    def test_lowercases_and_joins_with_hyphens(self):
        self.assertEqual(slugify("Hello World"), "hello-world")

    def test_drops_punctuation_and_repeated_separators(self):
        self.assertEqual(slugify("  Café, Pão & Leite!  "), "cafe-pao-leite")
