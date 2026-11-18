"""Funcții pentru normalizarea și împărțirea textului în cuvinte."""

import re


# Sunt acceptate literele Unicode, cifrele și caracterul de subliniere.
WORD_PATTERN = re.compile(r"[^\W_]+", flags=re.UNICODE)


def tokenize(text: str) -> list[str]:
    """Întoarce cuvintele din text scrise cu litere mici.

    Exemplu:
        "Python este rapid!" -> ["python", "este", "rapid"]
    """

    return WORD_PATTERN.findall(text.lower())
