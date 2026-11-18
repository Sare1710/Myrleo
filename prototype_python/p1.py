doc = {
    1: "Linux este rapid",
    2: "Python merge pe Linux",
    3: "Linux Linux Linux si Python",
    4: "Linux este un sistem de operare"
}

index = {}

for id_doc, text in doc.items():
    cuvinte = text.lower().split()

    for cuvint in cuvinte:
        if cuvint not in index:
            index[cuvint] = []

        if id_doc not in index[cuvint]:
            index[cuvint].append(id_doc)
print(index)


linux = {1, 2, 3}
python = {2, 3}

rezultat = linux & python

# print(rezultat)


def cait(cuvint1, cuvint2):
    cuvint1 = cuvint1.lower();
    cuvint2 = cuvint2.lower();

    lista1 = set(index.get(cuvint1, []))
    lista2 = set(index.get(cuvint2, []))

    rezultat = lista1 & lista2

    return sorted(rezultat)
print(cait("Linux","rapid"))


def cautare(text):
    cuvinte = text.lower().split();
    rezultat = None

    for cuvint in cuvinte:
        documente = set(index.get(cuvint, []))

        if rezultat is None:
            rezultat = documente
        else:
            rezultat = rezultat & documente

    return sorted(rezultat) if rezultat is not None else []

rezultat = cautare("Linux Python")

for id_doc in rezultat:
    print("Document", id_doc, ":", doc[id_doc])

input = input("Ce cauti: ")

rezultate = cautare(input)

if not rezultate:
    print("Nu s-au gasit documente care sa contina toate cuvintele cautate.")
else:
    for id_doc in rezultate:
        print("Document", id_doc, ":", doc[id_doc])
        pass

def ranking(text_cautat):
    cuvinte_cautate = text_cautat.lowe().split()
    rezultat = []

    for id_doc, text in doc.items():
        scor+= cuvinte 
    