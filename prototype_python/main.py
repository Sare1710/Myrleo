"""Interfața în terminal pentru motorul de căutare."""

import json
from pathlib import Path

from motor_cautare import Document, SearchEngine


DATA_FILE = Path(__file__).parent / "data" / "documents.json"


def load_documents(path: Path) -> list[Document]:
    """Citește documentele dintr-un fișier JSON."""

    with path.open(encoding="utf-8") as file:
        raw_documents = json.load(file)

    return [Document(**item) for item in raw_documents]


def show_results(engine: SearchEngine, query: str) -> None:
    """Caută și afișează rezultatele într-o formă ușor de citit."""

    results = engine.search(query)
    if not results:
        print("Nu am găsit niciun rezultat.\n")
        return

    print(f"\nAm găsit {len(results)} rezultat(e):\n")

    for position, result in enumerate(results, start=1):
        document = result.document
        snippet = document.text[:150]
        if len(document.text) > 150:
            snippet += "..."

        print(f"{position}. {document.title}")
        if document.url:
            print(f"   {document.url}")
        print(f"   {snippet}")
        print(f"   Scor: {result.score:.3f}")
        print(f"   Cuvinte găsite: {', '.join(result.matched_terms)}\n")


def main() -> None:
    """Pornește motorul de căutare interactiv."""

    engine = SearchEngine()
    engine.add_documents(load_documents(DATA_FILE))

    print("Motor de căutare clasic")
    print(f"Sunt indexate {len(engine.documents)} documente.")
    print("Scrie ce dorești să cauți sau 'exit' pentru a închide.\n")

    while True:
        try:
            query = input("Caută: ").strip()
        except (EOFError, KeyboardInterrupt):
            print("\nLa revedere!")
            break

        if query.lower() in {"exit", "quit", "iesire", "ieșire"}:
            print("La revedere!")
            break

        if not query:
            continue

        show_results(engine, query)


if __name__ == "__main__":
    main()
