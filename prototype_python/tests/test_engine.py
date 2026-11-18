"""Teste pentru motorul de căutare."""

import unittest

from motor_cautare import Document, SearchEngine
from motor_cautare.tokenizer import tokenize


class TokenizerTests(unittest.TestCase):
    def test_tokenize_normalizes_text(self) -> None:
        self.assertEqual(
            tokenize("Python este RAPID, simplu și clar!"),
            ["python", "este", "rapid", "simplu", "și", "clar"],
        )


class SearchEngineTests(unittest.TestCase):
    def setUp(self) -> None:
        self.engine = SearchEngine()
        self.engine.add_documents(
            [
                Document(1, "Python", "Python este simplu și rapid."),
                Document(2, "Linux", "Linux este liber și rapid."),
                Document(3, "Index", "Un index face căutarea rapidă."),
            ]
        )

    def test_search_finds_matching_document(self) -> None:
        results = self.engine.search("python")

        self.assertEqual(len(results), 1)
        self.assertEqual(results[0].document.id, 1)

    def test_search_orders_results_by_relevance(self) -> None:
        results = self.engine.search("python rapid")

        self.assertEqual(results[0].document.id, 1)
        self.assertIn("python", results[0].matched_terms)
        self.assertIn("rapid", results[0].matched_terms)

    def test_unknown_word_returns_no_results(self) -> None:
        self.assertEqual(self.engine.search("necunoscut"), [])

    def test_replacing_document_updates_index(self) -> None:
        self.engine.add_document(Document(1, "Alt titlu", "Numai Java."))

        self.assertEqual(self.engine.search("python"), [])
        self.assertEqual(self.engine.search("java")[0].document.id, 1)

    def test_remove_document(self) -> None:
        self.assertTrue(self.engine.remove_document(2))
        self.assertFalse(self.engine.remove_document(2))
        self.assertEqual(self.engine.search("linux"), [])


if __name__ == "__main__":
    unittest.main()
