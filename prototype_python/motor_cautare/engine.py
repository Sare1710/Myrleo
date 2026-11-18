"""Indexul inversat și algoritmul de căutare."""

from collections import Counter, defaultdict
from math import log

from .document import Document, SearchResult
from .tokenizer import tokenize


class SearchEngine:
    """Indexează documente și efectuează căutări clasice în ele."""

    def __init__(self) -> None:
        self.documents: dict[int, Document] = {}

        # Structura este: cuvânt -> {id_document: număr_apariții}.
        self.inverted_index: dict[str, dict[int, int]] = defaultdict(dict)

        # Păstrăm cuvintele fiecărui document pentru a-l putea reindexa.
        self._document_terms: dict[int, Counter[str]] = {}

    def add_document(self, document: Document) -> None:
        """Adaugă un document în index sau îl înlocuiește dacă există."""

        if document.id in self.documents:
            self.remove_document(document.id)

        # Titlul este inclus deoarece și el descrie conținutul documentului.
        terms = tokenize(f"{document.title} {document.text}")
        term_frequencies = Counter(terms)

        self.documents[document.id] = document
        self._document_terms[document.id] = term_frequencies

        for term, frequency in term_frequencies.items():
            self.inverted_index[term][document.id] = frequency

    def add_documents(self, documents: list[Document]) -> None:
        """Adaugă mai multe documente."""

        for document in documents:
            self.add_document(document)

    def remove_document(self, document_id: int) -> bool:
        """Elimină un document. Întoarce False dacă acesta nu există."""

        terms = self._document_terms.pop(document_id, None)
        if terms is None:
            return False

        for term in terms:
            postings = self.inverted_index[term]
            postings.pop(document_id, None)
            if not postings:
                del self.inverted_index[term]

        del self.documents[document_id]
        return True

    def search(self, query: str, limit: int = 10) -> list[SearchResult]:
        """Caută documentele relevante pentru expresia primită.

        Scorul folosește o variantă simplificată de TF-IDF:
        - TF măsoară de câte ori apare cuvântul într-un document;
        - IDF acordă o valoare mai mare cuvintelor rare.
        """

        query_terms = Counter(tokenize(query))
        if not query_terms or limit <= 0 or not self.documents:
            return []

        scores: dict[int, float] = defaultdict(float)
        matches: dict[int, set[str]] = defaultdict(set)
        document_count = len(self.documents)

        for term, query_frequency in query_terms.items():
            postings = self.inverted_index.get(term)
            if not postings:
                continue

            document_frequency = len(postings)
            inverse_document_frequency = (
                log((document_count + 1) / (document_frequency + 1)) + 1
            )

            for document_id, term_frequency in postings.items():
                # Logaritmul împiedică repetarea excesivă a unui cuvânt să
                # domine complet rezultatele.
                normalized_frequency = 1 + log(term_frequency)
                scores[document_id] += (
                    normalized_frequency
                    * inverse_document_frequency
                    * query_frequency
                )
                matches[document_id].add(term)

        ranked_ids = sorted(
            scores,
            key=lambda document_id: (-scores[document_id], document_id),
        )

        return [
            SearchResult(
                document=self.documents[document_id],
                score=scores[document_id],
                matched_terms=tuple(sorted(matches[document_id])),
            )
            for document_id in ranked_ids[:limit]
        ]
