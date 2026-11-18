# Myrleo

**Myrleo** este un motor de căutare web clasic, fără reclame și fără AI,
scris în Rust.

Versiunea actuală poate:

- parcurge un site prin BFS;
- respecta regulile din `robots.txt`;
- limita numărul de pagini, adâncimea și viteza cererilor;
- extrage titlul, textul și linkurile din HTML;
- salva paginile și indexul pozițional în SQLite;
- ordona rezultatele cu BM25 și un bonus PageRank;
- compara algoritmii TF, TF-IDF și BM25;
- interpreta expresii booleene cu `AND`, `OR`, `NOT` și paranteze;
- căuta expresii exacte folosind indexul pozițional;
- sugera cuvinte prin Trie;
- corecta termeni prin Levenshtein și BK-tree;
- calcula PageRank pe graful paginilor descărcate.
- oferi o interfață web responsive cu autocompletare și corectare.

Versiunea inițială Python este păstrată în `prototype_python/` doar ca material
educațional. Aplicația principală este cea Rust.

## Compilare și teste

```bash
cargo build
cargo test
```

## Descărcarea unui site

Începe cu un site mic pe care ai permisiunea să îl accesezi:

```bash
cargo run -- crawl https://example.com \
  --max-pages 20 \
  --max-depth 2 \
  --delay-ms 1000
```

Crawlerul rămâne pe același domeniu și acceptă numai pagini HTML. Baza de date
implicită este `myrleo.db`.

## Căutare

```bash
cargo run -- search "cuvinte căutate"
```

Algoritmul implicit este BM25. Celelalte moduri se aleg astfel:

```bash
cargo run -- search "rust rapid" --algorithm tf
cargo run -- search "rust rapid" --algorithm tf-idf
cargo run -- search "rust AND (rapid OR sigur)" --algorithm boolean
cargo run -- search "rust AND NOT lent" --algorithm boolean
cargo run -- search "motor de căutare" --algorithm phrase
```

Autocompletare și corectare:

```bash
cargo run -- suggest mot
cargo run -- correct mtoor --max-distance 2
```

Poți schimba baza de date:

```bash
cargo run -- --database alta.db search "rust"
```

## Statistici

```bash
cargo run -- stats
```

## Interfața web

```bash
cargo run -- serve
```

Deschide apoi `http://127.0.0.1:8080`. Pentru alt port:

```bash
cargo run -- serve --port 3000
```

## Test local fără accesarea internetului

Într-un terminal:

```bash
python -m http.server 8765 --directory test_site
```

În alt terminal:

```bash
cargo run -- --database test.db crawl http://127.0.0.1:8765 --delay-ms 0
cargo run -- --database test.db search "rust"
```

## Reguli de utilizare

- folosește crawlerul numai pe pagini publice;
- respectă `robots.txt` și termenii site-ului;
- nu încerca să ocolești autentificarea sau protecțiile anti-bot;
- păstrează o pauză rezonabilă între cereri;
- setează limite mici în timpul dezvoltării.

## Plan

Următoarele etape vor integra sugestiile în interfața de căutare și vor adăuga
o interfață web locală.
