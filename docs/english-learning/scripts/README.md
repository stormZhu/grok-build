# Vocabulary analysis script

`analyze_vocabulary.py` extracts English prose from the Grok Build repository, ranks vocabulary, and prints original contexts with source locations.

## Run it

From the repository root:

```sh
python3 docs/english-learning/scripts/analyze_vocabulary.py
```

Show the top 80 words without excerpts:

```sh
python3 docs/english-learning/scripts/analyze_vocabulary.py \
  --top 80 \
  --examples 0
```

Inspect selected words and show two original contexts for each:

```sh
python3 docs/english-learning/scripts/analyze_vocabulary.py \
  --examples 2 \
  --word session \
  --word compaction \
  --word cancellation
```

Use `--help` for all options. The report is written to standard output, so the script does not modify the repository.

## Corpus

The default corpus contains:

- the root `README.md`;
- prose outside fenced code blocks in `crates/**/*.md`;
- consecutive `//`, `///`, and `//!` comments in `crates/**/*.rs`.

It excludes package wrappers and data that would distort the result, including `npm`, `fuzz`, `fixtures`, `snapshots`, `generated`, `target`, and `third_party` paths. It does not scan identifiers or executable Rust code.

The `docs/english-learning` directory is outside the corpus. Consequently, adding a word to the learning material does not increase that word's measured frequency.

## Counting rules

- Matching is case-insensitive.
- A small, explicit alias table merges selected word forms such as `sessions` into `session`; ambiguous senses remain visible in the quoted contexts.
- Common function words are removed with a built-in stop-word set.
- Occurrences and distinct file coverage are both reported.
- The ranking score is `occurrences * (1 + log2(1 + files))`.
- Rust and Markdown counts remain separate in the report.

This is a project vocabulary analysis, not a general English lemmatizer. For example, `fallback` and the verb phrase `falls back` are not silently merged. The learning article can discuss them as a word family, but the numeric count remains reproducible and explicit.

## Interpreting results

High frequency identifies repository-wide concepts, but does not determine learning priority on its own:

- `session`, `tool`, and `prompt` are both frequent and central;
- `file`, `path`, and `test` are frequent but less specific to Agent architecture;
- `invariant`, `persistence`, and `cancellation` occur less often but carry important design meaning;
- a word such as `terminal` has multiple project senses, so its raw count must be interpreted from context.

The script surfaces evidence; the accompanying [key-word learning material](../06-key-words-in-context.md) performs the semantic curation.
