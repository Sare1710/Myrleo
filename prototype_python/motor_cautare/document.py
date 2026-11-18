"""Modelele de date folosite de motorul de căutare."""

from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class Document:
    """Un document care poate fi indexat și găsit."""

    id: int
    title: str
    text: str
    url: str = ""


@dataclass(frozen=True, slots=True)
class SearchResult:
    """Un document găsit împreună cu scorul său de relevanță."""

    document: Document
    score: float
    matched_terms: tuple[str, ...]
